//! First-run model selection: look at the machine, recommend a model, let the user
//! choose, pull it, verify it and remember it. The daemon still owns the settings; this
//! module talks to it through the same Unix socket as the CLI.

use crate::ai_catalog::{Catalog, Model, Recommendation};
use crate::ai_hardware::Hardware;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, IsTerminal, Write};
use std::process::Command;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub enum PullEvent {
    Progress { status: String, completed: Option<u64>, total: Option<u64> },
    Success,
    Error(String),
    Other,
}

/// One newline-delimited event from Ollama's `/api/pull` stream.
pub fn parse_pull_line(line: &str) -> PullEvent {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return PullEvent::Other;
    };
    if let Some(error) = value["error"].as_str() {
        return PullEvent::Error(error.to_string());
    }
    match value["status"].as_str() {
        Some("success") => PullEvent::Success,
        Some(status) => PullEvent::Progress { status: status.to_string(), completed: value["completed"].as_u64(), total: value["total"].as_u64() },
        None => PullEvent::Other,
    }
}

fn needs_from_args(args: &[String]) -> Vec<&'static str> {
    let mut needs = vec!["text", "tools", "json", "bilingual"];
    if args.iter().any(|a| a == "--vision") {
        needs.push("vision");
    }
    if args.iter().any(|a| a == "--audio") {
        needs.push("audio");
    }
    needs
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

/// Licences Buddy can recommend without an extra acceptance step. Unknown or
/// non-commercial licences need the user to pass `--accept-license`.
fn license_is_known_permissive(license: &str) -> bool {
    license.starts_with("Apache-2.0") || license.starts_with("MIT")
}

fn print_hardware(hardware: &Hardware) {
    println!("Hardware");
    println!(
        "  cpu:     {} ({} threads, {})",
        if hardware.cpu.is_empty() { "unknown" } else { &hardware.cpu },
        hardware.cores,
        if hardware.avx2 { "AVX2" } else { "no AVX2" }
    );
    println!("  memory:  {:.1} GB ({:.1} GB available)", hardware.ram_gb, hardware.ram_available_gb);
    match &hardware.gpu {
        Some(gpu) => println!("  gpu:     {} with {:.1} GB VRAM (driver {})", gpu.name, gpu.vram_gb, gpu.driver),
        None => println!("  gpu:     no NVIDIA GPU detected; the model will run on the CPU (AMD/Vulkan probing is not implemented)"),
    }
    println!("  disk:    {:.1} GB free", hardware.free_disk_gb);
    if hardware.system_ollama {
        println!("  note:    another Ollama is listening on 127.0.0.1:11434; it is left untouched");
    }
}

fn print_recommendation(pick: &Recommendation) {
    let model = pick.default;
    println!(
        "Recommended: {} ({}), {:.1} GB download, at least {:.0} GB RAM, licence: {}{}",
        model.name,
        model.params,
        model.download_gb,
        model.min_ram_gb,
        model.license,
        if model.verified { "" } else { "; not evaluated by this project yet" }
    );
    for alternative in &pick.alternatives {
        println!(
            "  alternative: {} ({}), {:.1} GB download, at least {:.0} GB RAM, licence: {}{}",
            alternative.name,
            alternative.params,
            alternative.download_gb,
            alternative.min_ram_gb,
            alternative.license,
            if alternative.verified { "" } else { "; unevaluated" }
        );
    }
}

fn choose<'a>(pick: &Recommendation<'a>, catalog: &'a Catalog) -> Result<&'a Model> {
    let mut choices = vec![pick.default];
    choices.extend(&pick.alternatives);
    println!("Choose a model:");
    for (index, model) in choices.iter().enumerate() {
        println!("  {}. {} ({}), {:.1} GB, licence: {}", index + 1, model.name, model.params, model.download_gb, model.license);
    }
    print!("Model [1]: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let line = line.trim();
    if line.is_empty() || line == "y" || line == "yes" {
        return Ok(pick.default);
    }
    if let Ok(number) = line.parse::<usize>() {
        if let Some(model) = choices.get(number.saturating_sub(1)) {
            return Ok(model);
        }
    }
    if let Some(model) = catalog.find(line) {
        return Ok(model);
    }
    bail!("no such model: {line}")
}

fn progress_line(event: PullEvent) -> bool {
    match event {
        PullEvent::Progress { status, completed, total } => {
            match (completed, total) {
                (Some(completed), Some(total)) if total > 0 => {
                    print!("\r{status}: {:.1}%", completed as f64 / total as f64 * 100.0);
                }
                _ => print!("\r{status}"),
            }
            let _ = std::io::stdout().flush();
            false
        }
        PullEvent::Success => {
            println!();
            true
        }
        PullEvent::Error(why) => {
            println!();
            eprintln!("Ollama could not pull the model: {why}");
            false
        }
        PullEvent::Other => false,
    }
}

/// Pull a model through the managed runtime and verify Ollama recorded it.
pub fn pull(model: &str) -> Result<()> {
    if crate::ai::is_cloud_model(model) {
        bail!("cloud models are not used: Buddy runs a local model");
    }
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(60 * 60))).build().into();
    let mut response = agent
        .post(&crate::ai::url("/api/pull"))
        .send_json(json!({ "model": model, "stream": true }))
        .with_context(|| format!("could not ask Ollama to pull '{model}'"))?;
    let reader = BufReader::new(response.body_mut().as_reader());
    let mut success = false;
    for line in reader.lines() {
        let line = line.context("the model pull stream broke")?;
        let event = parse_pull_line(&line);
        if let PullEvent::Error(why) = &event {
            bail!("Ollama could not pull '{model}': {why}");
        }
        if progress_line(event) {
            success = true;
            break;
        }
    }
    if !success {
        bail!("the pull of '{model}' ended without a success event");
    }
    let _ = crate::ai::digest(model)?;
    Ok(())
}

fn remember(name: &str, digest: Option<&str>) -> Result<bool> {
    let path = crate::ai_runtime::Runtime::path();
    // A corrupt manifest must be repaired, not silently replaced: it holds the port,
    // version and model store the daemon and the installer wrote.
    let mut runtime = if path.exists() { crate::ai_runtime::Runtime::load_from(&path)? } else { crate::ai_runtime::Runtime::default() };
    runtime.ollama_version = crate::ai::version().unwrap_or(runtime.ollama_version);
    let language_ok = crate::client::request("set", json!({ "key": "language_model", "value": name })).is_ok();
    let vision_ok = language_ok && crate::client::request("set", json!({ "key": "vision_model", "value": name })).is_ok();
    if language_ok {
        runtime.model = Some(name.to_string());
        runtime.model_digest = digest.map(str::to_string);
    } else {
        println!("The model is installed, but Buddy is not running, so it was not selected.");
        println!("Start Buddy, then run: buddy ai model {name}");
    }
    runtime.save_to(&path)?;
    if language_ok && !vision_ok {
        println!("The language model was set, but the vision model could not be updated.");
    }
    Ok(language_ok)
}

pub fn setup(args: &[String]) -> Result<()> {
    let hardware = crate::ai_hardware::probe();
    let catalog = Catalog::load()?;
    let needs = needs_from_args(args);
    print_hardware(&hardware);
    let chosen = if let Some(name) = flag(args, "--model") {
        let model = catalog.find(name).with_context(|| format!("'{name}' is not in the model catalogue"))?;
        println!("Using requested model: {} ({}), licence: {}", model.name, model.params, model.license);
        model
    } else {
        let pick = catalog
            .recommend(&hardware, &needs)
            .with_context(|| "no model in the catalogue fits this machine; choose one with: buddy ai setup --model NAME")?;
        print_recommendation(&pick);
        if args.iter().any(|a| a == "--yes") || !std::io::stdin().is_terminal() { pick.default } else { choose(&pick, &catalog)? }
    };
    if !license_is_known_permissive(&chosen.license) && !args.iter().any(|a| a == "--accept-license") {
        bail!("{} is offered under '{}'; check its licence and rerun with --accept-license", chosen.name, chosen.license);
    }
    if hardware.ram_gb < chosen.min_ram_gb {
        println!(
            "warning: {} asks for at least {:.0} GB RAM and this machine has {:.1} GB; it may fail or be very slow",
            chosen.name, chosen.min_ram_gb, hardware.ram_gb
        );
    }
    if hardware.free_disk_gb > 0.0 && hardware.free_disk_gb < chosen.download_gb + 2.0 {
        bail!(
            "{} needs about {:.1} GB free (download plus headroom) and this machine has {:.1} GB",
            chosen.name,
            chosen.download_gb + 2.0,
            hardware.free_disk_gb
        );
    }
    if !chosen.has("vision") {
        println!("note: {} has no vision; `buddy look` will need a vision-capable model later", chosen.name);
    }
    println!("Pulling {} ...", chosen.name);
    pull(&chosen.name)?;
    let digest = crate::ai::digest(&chosen.name).ok();
    if remember(&chosen.name, digest.as_deref())? {
        println!("Selected {}.", chosen.name);
    }
    Ok(())
}

pub fn model(args: &[String]) -> Result<()> {
    let installed = crate::ai::models()?;
    let runtime = crate::ai_runtime::Runtime::load_or_default();
    if let Some(name) = args.first() {
        if crate::ai::is_cloud_model(name) {
            bail!("cloud models are not used: Buddy runs a local model");
        }
        if !installed.iter().any(|model| model == name) {
            bail!("'{name}' is not installed; pull it with: buddy ai setup --model {name}");
        }
        let digest = crate::ai::digest(name).ok();
        if remember(name, digest.as_deref())? {
            println!("Selected {name}.");
        }
        return Ok(());
    }
    if installed.is_empty() {
        println!("No models are installed. Run: buddy ai setup");
        return Ok(());
    }
    for name in installed {
        let marker = if runtime.model.as_deref() == Some(name.as_str()) { "*" } else { " " };
        println!("{marker} {name}");
    }
    Ok(())
}

pub fn repair() -> Result<()> {
    let runtime = crate::ai_runtime::Runtime::load_or_default();
    let name = runtime.model.context("no model is selected; run: buddy ai setup")?;
    println!("Re-pulling {name} ...");
    pull(&name)?;
    let digest = crate::ai::digest(&name).ok();
    if runtime.model_digest.is_some() && runtime.model_digest != digest {
        println!("The recorded digest changed; updating the manifest.");
    }
    remember(&name, digest.as_deref())?;
    let _ = Command::new("systemctl").args(["--user", "restart", "buddy-ollama.service"]).status();
    println!("{name} is ready.");
    Ok(())
}

fn unused_models<'a>(installed: &'a [String], protected: &[String]) -> Vec<&'a String> {
    installed.iter().filter(|model| !protected.iter().any(|kept| kept == model.as_str())).collect()
}

pub fn prune(args: &[String]) -> Result<()> {
    let runtime = crate::ai_runtime::Runtime::load_or_default();
    let endpoint = crate::ai::endpoint();
    let managed = runtime.endpoint.trim_end_matches('/');
    if endpoint != managed {
        bail!("refusing to prune: {endpoint} is not Buddy's managed runtime ({managed})");
    }
    // Ask the daemon which models are actually in use; the vision model may differ from
    // the language model. Without the daemon, there is no safe way to know.
    let providers = crate::client::request("providers", Value::Null).context("start Buddy before pruning so the models in use can be checked")?;
    let mut protected = Vec::new();
    for key in ["understands_language_with", "looks_at_images_with"] {
        if let Some(model) = providers["local"][key].as_str() {
            protected.push(model.to_string());
        }
    }
    if let Some(model) = &runtime.model {
        protected.push(model.clone());
    }
    if protected.is_empty() {
        bail!("no model is selected; run buddy ai setup first");
    }
    let installed = crate::ai::models()?;
    let unused = unused_models(&installed, &protected);
    if unused.is_empty() {
        println!("Nothing to prune.");
        return Ok(());
    }
    if !args.iter().any(|a| a == "--yes") {
        println!("Would remove: {}", unused.iter().map(|m| m.as_str()).collect::<Vec<_>>().join(", "));
        println!("Run with --yes to remove them.");
        return Ok(());
    }
    for model in unused {
        crate::ai::delete(model)?;
        println!("Removed {model}.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pull_stream_is_understood() {
        let progress = parse_pull_line(r#"{"status":"downloading digest","digest":"sha256:abc","total":100,"completed":25}"#);
        assert_eq!(progress, PullEvent::Progress { status: "downloading digest".into(), completed: Some(25), total: Some(100) });
        assert_eq!(parse_pull_line(r#"{"status":"success"}"#), PullEvent::Success);
        assert_eq!(
            parse_pull_line(r#"{"error":"pull model manifest: file does not exist"}"#),
            PullEvent::Error("pull model manifest: file does not exist".into())
        );
        assert_eq!(parse_pull_line("not json"), PullEvent::Other);
    }

    #[test]
    fn vision_and_audio_are_needed_only_when_asked() {
        assert_eq!(needs_from_args(&[]), vec!["text", "tools", "json", "bilingual"]);
        let with_vision = needs_from_args(&["--vision".to_string()]);
        assert!(with_vision.contains(&"vision"));
        let with_audio = needs_from_args(&["--audio".to_string()]);
        assert!(with_audio.contains(&"audio"));
    }

    #[test]
    fn pruning_keeps_every_model_the_daemon_is_using() {
        let installed = vec!["gemma4:e4b".to_string(), "qwen3-vl:8b".to_string(), "lfm3:1.7b".to_string()];
        let protected = vec!["gemma4:e4b".to_string(), "qwen3-vl:8b".to_string()];
        assert_eq!(unused_models(&installed, &protected), vec!["lfm3:1.7b"]);
        assert_eq!(unused_models(&installed, &[]).len(), 3, "an empty protected list is handled by prune before deletion");
    }

    #[test]
    fn only_known_permissive_licences_skip_the_acceptance_step() {
        assert!(license_is_known_permissive("Apache-2.0"));
        assert!(license_is_known_permissive("MIT"));
        assert!(!license_is_known_permissive("check the model page"));
        assert!(!license_is_known_permissive("CC BY-NC 4.0"));
    }

    #[test]
    fn the_model_flag_reads_the_next_argument() {
        let args = vec!["--model".to_string(), "gemma4:e4b".to_string()];
        assert_eq!(flag(&args, "--model"), Some("gemma4:e4b"));
        assert_eq!(flag(&args, "--vision"), None);
    }
}

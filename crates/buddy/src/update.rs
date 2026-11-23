//! Version and update checks. Network access happens only when the user asks for it.
//!
//! Nothing here downloads or installs anything. A future guided upgrade can use the
//! same verified release metadata, but the default remains: local-first, explicit check.

use anyhow::{Context, Result};
use serde_json::Value;
use std::time::Duration;

pub const REPO: &str = "vhidvz/golden-retriever";
const GODOT_REPO: &str = "godotengine/godot";
const OLLAMA_REPO: &str = "ollama/ollama";
const USER_AGENT: &str = "buddy-update-check";

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub url: String,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into()
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn tag_number(tag: &str) -> &str {
    tag.trim().trim_start_matches(['v', 'V'])
}

fn numbers(version: &str) -> Vec<u64> {
    version
        .split(['.', '-', '+'])
        .take(4)
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

/// True when `latest` is a higher numeric release than `current`. Unparseable
/// versions are compared as unequal, and the caller can present both.
pub fn is_newer(current: &str, latest: &str) -> bool {
    let (a, b) = (numbers(tag_number(current)), numbers(tag_number(latest)));
    let len = a.len().max(b.len());
    for index in 0..len {
        let left = a.get(index).copied().unwrap_or(0);
        let right = b.get(index).copied().unwrap_or(0);
        if left != right {
            return right > left;
        }
    }
    false
}

pub fn parse_release(value: &Value) -> Result<Release> {
    let tag = value["tag_name"].as_str().context("the release has no tag")?.to_string();
    Ok(Release {
        name: value["name"].as_str().unwrap_or(&tag).to_string(),
        url: value["html_url"].as_str().unwrap_or("https://github.com").to_string(),
        tag,
    })
}

pub fn latest(repo: &str) -> Result<Release> {
    let mut response = agent(Duration::from_secs(12))
        .get(&format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("could not ask GitHub about {repo}"))?;
    let value: Value = response.body_mut().read_json().context("GitHub sent a release this version does not understand")?;
    parse_release(&value)
}

pub fn repository() -> String {
    std::env::var("BUDDY_UPDATE_REPO").unwrap_or_else(|_| REPO.to_string())
}

pub fn check_buddy() -> Result<String> {
    let repo = repository();
    let current = current_version();
    let release = match latest(&repo) {
        Ok(release) => release,
        Err(error) if format!("{error:#}").contains("404") => {
            return Ok(format!("Buddy {current}\nNo published releases were found for {repo} yet."));
        }
        Err(error) => return Err(error),
    };
    let newer = is_newer(current, &release.tag);
    Ok(format!(
        "Buddy {current}\nlatest release: {} ({})\n{}\n{}",
        release.tag,
        release.name,
        release.url,
        if newer { "A newer release is available. Download it from the release page; nothing was installed." } else { "This is the newest release I can see." }
    ))
}

fn managed_godot() -> Option<String> {
    if let Some(path) = std::env::var_os("BUDDY_GODOT") {
        let name = std::path::Path::new(&path).file_name()?.to_string_lossy().into_owned();
        return name.split('_').find(|part| part.chars().next().is_some_and(|c| c.is_ascii_digit())).map(str::to_string);
    }
    let root = buddy_core::paths::data_dir().join("godot");
    let mut found = None;
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                found = Some(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    found
}

fn managed_ollama() -> Option<String> {
    let manifest = buddy_core::paths::runtime_manifest_path();
    let text = std::fs::read_to_string(manifest).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value["ollama_version"].as_str().map(str::to_string)
}

pub fn check_tools() -> Result<String> {
    let mut lines = vec![format!("Buddy {}", current_version())];
    let godot = managed_godot();
    let ollama = managed_ollama();
    match latest(GODOT_REPO) {
        Ok(release) => lines.push(format!(
            "Godot: {} | latest release: {} ({})",
            godot.as_deref().unwrap_or("not found"),
            release.tag,
            release.url
        )),
        Err(error) => lines.push(format!("Godot: {} | latest check failed: {error}", godot.as_deref().unwrap_or("not found"))),
    }
    match latest(OLLAMA_REPO) {
        Ok(release) => lines.push(format!(
            "Ollama: {} | latest release: {} ({})",
            ollama.as_deref().unwrap_or("not found"),
            release.tag,
            release.url
        )),
        Err(error) => lines.push(format!("Ollama: {} | latest check failed: {error}", ollama.as_deref().unwrap_or("not found"))),
    }
    lines.push("Models are checked with `buddy ai status`; no model is updated automatically.".to_string());
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn release_tags_are_compared_numerically() {
        assert!(is_newer("0.1.0", "v0.2.0"));
        assert!(is_newer("0.1.9", "v0.2.0"));
        assert!(!is_newer("1.0.0", "v1.0.0"));
        assert!(!is_newer("1.2.0", "v1.1.9"));
        assert!(is_newer("1.2", "1.2.1"));
        assert!(!is_newer("1.2.1", "1.2"));
    }

    #[test]
    fn release_metadata_is_read_without_trusting_missing_fields() {
        let release = parse_release(&json!({
            "tag_name": "v0.2.0",
            "name": "Buddy 0.2.0",
            "html_url": "https://example.test/release"
        }))
        .unwrap();
        assert_eq!(release.tag, "v0.2.0");
        assert_eq!(release.name, "Buddy 0.2.0");
        assert!(parse_release(&json!({})).is_err());
    }
}

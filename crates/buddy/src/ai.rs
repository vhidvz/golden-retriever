//! Local models, reached through Ollama on this machine. Optional: nothing in
//! Buddy's ordinary life calls this, and every caller copes with it being absent.

use anyhow::{Context, Result, bail};
use base64::Engine;
use serde_json::{Value, json};
use std::time::Duration;

pub const KEEP_ALIVE: &str = "5m";
pub const NUM_CTX: u32 = 8192;

/// Ollama can list cloud models alongside local ones. Buddy is local-first: a cloud
/// model must never be selected, pulled or sent a recording.
pub fn is_cloud_model(name: &str) -> bool {
    name.contains(":cloud") || name.ends_with("-cloud")
}

/// The capability names from an Ollama `/api/show` reply.
pub fn parse_capabilities(reply: &Value) -> Vec<String> {
    reply["capabilities"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect()
}

/// What an installed model says it can do: completion, vision, audio, tools, thinking.
pub fn capabilities(model: &str) -> Result<Vec<String>> {
    let reply: Value = agent(Duration::from_secs(10))
        .post(&url("/api/show"))
        .send_json(json!({ "model": model }))
        .with_context(|| format!("Ollama could not describe '{model}'"))?
        .body_mut()
        .read_json()?;
    Ok(parse_capabilities(&reply))
}

pub fn supports(model: &str, capability: &str) -> Result<bool> {
    Ok(capabilities(model)?.iter().any(|c| c == capability))
}

/// The endpoint of Buddy's managed runtime, or `BUDDY_OLLAMA_URL` when set.
pub fn endpoint() -> String {
    crate::ai_runtime::Runtime::endpoint()
}

pub(crate) fn url(path: &str) -> String {
    format!("{}{}", endpoint(), path)
}

/// The runtime's version, or a plain error saying it cannot be reached.
pub fn version() -> Result<String> {
    let reply: Value = agent(Duration::from_secs(2))
        .get(&url("/api/version"))
        .call()
        .with_context(|| format!("the local model runtime is not running at {}", endpoint()))?
        .body_mut()
        .read_json()?;
    reply["version"].as_str().map(str::to_string).context("the runtime did not report a version")
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into()
}

/// The models Ollama has installed, or an error saying why it cannot be reached.
pub fn models() -> Result<Vec<String>> {
    let tags: Value = agent(Duration::from_secs(2))
        .get(&url("/api/tags"))
        .call()
        .with_context(|| format!("the local model runtime is not running at {}", endpoint()))?
        .body_mut()
        .read_json()?;
    Ok(tags["models"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m["name"].as_str().map(str::to_string))
        .filter(|name| !is_cloud_model(name))
        .collect())
}

/// The digest Ollama recorded for an installed model.
pub fn digest(model: &str) -> Result<String> {
    let tags: Value = agent(Duration::from_secs(5))
        .get(&url("/api/tags"))
        .call()
        .with_context(|| format!("the local model runtime is not running at {}", endpoint()))?
        .body_mut()
        .read_json()?;
    tags["models"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["name"].as_str() == Some(model) || m["model"].as_str() == Some(model))
        .and_then(|m| m["digest"].as_str())
        .map(str::to_string)
        .with_context(|| format!("Ollama has no digest for '{model}'"))
}

/// Remove a model from the managed store. The caller must have asked the user first.
pub fn delete(model: &str) -> Result<()> {
    agent(Duration::from_secs(30))
        .post(&url("/api/delete"))
        .send_json(json!({ "model": model }))
        .with_context(|| format!("could not remove '{model}'"))?;
    Ok(())
}

/// Asks a local model what something said to the dog meant, when his own vocabulary could not place it.
/// `tricks` are what he can do; `taught` are words the user has taught him.
pub fn interpret(model: &str, said: &str, tricks: &[&str], taught: &[String]) -> Result<buddy_core::Heard> {
    use buddy_core::Heard;
    let installed = models()?;
    if !installed.iter().any(|m| m == model) {
        bail!("the model '{model}' is not installed in Ollama");
    }
    let system = format!(
        "A person said something to their pet dog Buddy (بادی), in English or Persian. Decide what it meant. Reply with JSON only.\n\
         kind is one of:\n\
         - cue: they want him to do one of his tricks. Put the trick in `trick`. Tricks: {}.{}\n\
         - praise, treat, approval (\"that's right\"), correction (\"that's wrong\"), stop (\"stop doing that\"), quiet.\n\
         - remember: they want him to keep a fact or preference. Put it in `text`, in their own words and language.\n\
         - teach: they are saying a new word should mean a trick (\"when I say X, do Y\"). Put the new word in `cue` and the trick in `trick`.\n\
         - task: they want something done on the computer: finding or opening a file or an application, taking a note, running something, a programming job.\n\
         - yes / no_help: an answer to something he offered (\"go ahead\" / \"not now, don't help me with this\").\n\
         - chat: said to him, but nothing to do. unknown: none of these.",
        tricks.join(", "),
        if taught.is_empty() {
            String::new()
        } else {
            format!(" Words they have taught him, which also count as cues (put the word in `cue`): {}.", taught.join(", "))
        }
    );
    let kinds = [
        "cue",
        "praise",
        "treat",
        "approval",
        "correction",
        "stop",
        "quiet",
        "remember",
        "teach",
        "task",
        "yes",
        "not_now",
        "no_help",
        "chat",
        "unknown",
    ];
    let schema = json!({ "type": "object", "required": ["kind", "trick", "cue", "text"], "properties": {
        "kind": { "type": "string", "enum": kinds }, "trick": { "type": "string" }, "cue": { "type": "string" }, "text": { "type": "string" } } });
    let body = json!({
        "model": model, "stream": false, "think": false, "format": schema, "keep_alive": KEEP_ALIVE, "options": { "temperature": 0, "num_predict": 120, "num_ctx": NUM_CTX },
        "messages": [{ "role": "system", "content": system }, { "role": "user", "content": said }],
    });
    let mut response = agent(Duration::from_secs(120)).post(&url("/api/chat")).send_json(&body).context("the local model did not answer")?;
    let reply: Value = response.body_mut().read_json()?;
    let answer: Value = serde_json::from_str(reply["message"]["content"].as_str().unwrap_or("{}")).unwrap_or(Value::Null);
    let field = |name: &str| answer[name].as_str().unwrap_or_default().trim().to_string();
    let (trick, cue) = (field("trick"), buddy_core::language::normalise(&field("cue")));
    Ok(match field("kind").as_str() {
        "cue" if taught.contains(&cue) => Heard::Cue(cue),
        "cue" if tricks.contains(&trick.as_str()) => Heard::Cue(trick),
        "praise" => Heard::Praise,
        "treat" => Heard::Treat,
        "approval" => Heard::Approval,
        "correction" => Heard::Correction,
        "stop" => Heard::Stop,
        "quiet" => Heard::Quiet,
        "remember" if !field("text").is_empty() => Heard::Remember(field("text")),
        "teach" if tricks.contains(&trick.as_str()) && !cue.is_empty() => Heard::Teach { cue, trick },
        "task" => Heard::Task { request: said.to_string(), skill: None, argument: None },
        "yes" => Heard::Yes,
        "no_help" => Heard::NoHelp,
        "not_now" => Heard::NotNow,
        "chat" => Heard::Chat,
        _ => Heard::Unknown,
    })
}

/// Build one model tool for an owner-registered helper. The tool name is an index
/// (`helper_0`, …), while the description carries the human name; this avoids invalid
/// function names and keeps the mapping unambiguous.
pub fn helper_tool(index: usize, helper: &crate::helpers::Helper) -> Value {
    let properties: serde_json::Map<String, Value> = helper.arguments.iter().map(|arg| (arg.clone(), json!({ "type": "string" }))).collect();
    json!({
        "type": "function",
        "function": {
            "name": format!("helper_{index}"),
            "description": format!(
                "Run the owner-registered helper '{}': {}. Arguments are separate strings; this is not a shell command.",
                helper.name, helper.description
            ),
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": helper.arguments.clone()
            }
        }
    })
}

/// Asks a local model which one skill or helper, if any, fulfils a request, and with
/// what arguments. Helpers are exposed as `helper_0`, `helper_1`, … and are converted
/// to the always-ask `run_helper` skill. The model never gets a shell command tool here.
pub fn plan(model: &str, request: &str, helpers: &[crate::helpers::Helper]) -> Result<Option<(String, Value)>> {
    let installed = models()?;
    if !installed.iter().any(|m| m == model) {
        bail!("the model '{model}' is not installed in Ollama");
    }
    let mut tools = crate::skills::as_tools().as_array().cloned().unwrap_or_default();
    tools.extend(helpers.iter().enumerate().map(|(index, helper)| helper_tool(index, helper)));
    let body = json!({
        "model": model, "stream": false, "think": false, "keep_alive": KEEP_ALIVE, "options": { "temperature": 0, "num_predict": 200, "num_ctx": NUM_CTX },
        "tools": tools,
        "messages": [
            { "role": "system", "content": "A person asked their assistant, a dog living on their Linux desktop, to do something on the computer, in English or Persian. \
               Choose the one tool that does it and fill in its arguments from their words. \
               The helper_* tools are scripts the owner registered; use one only when its description matches the request, and pass its arguments separately. \
               If no tool fits, call none and reply with the single word: none." },
            { "role": "user", "content": request }],
    });
    let mut response = agent(Duration::from_secs(120)).post(&url("/api/chat")).send_json(&body).context("the local model did not answer")?;
    let reply: Value = response.body_mut().read_json()?;
    let call = &reply["message"]["tool_calls"][0]["function"];
    let name = call["name"].as_str().unwrap_or_default();
    if let Some(index) = name.strip_prefix("helper_").and_then(|value| value.parse::<usize>().ok()) {
        if let Some(helper) = helpers.get(index) {
            return Ok(Some(("run_helper".to_string(), json!({ "helper": helper.name, "args": call["arguments"].clone() }))));
        }
    }
    Ok(crate::skills::skill(name).map(|_| (name.to_string(), call["arguments"].clone())))
}

/// Has a local model write down what was said in a short recording (16 kHz WAV).
/// Returns an empty string if it heard no speech.
pub fn transcribe(model: &str, wav: &[u8]) -> Result<String> {
    if is_cloud_model(model) {
        bail!("cloud models are not used: Buddy's recording stays on this machine");
    }
    if !supports(model, "audio")? {
        bail!("the model '{model}' has no audio input; choose one that does");
    }
    let body = json!({
        "model": model, "temperature": 0, "reasoning_effort": "none", "max_tokens": 120,
        "messages": [{ "role": "user", "content": [
            { "type": "text", "text": "Write down exactly what is said in this recording, in the language spoken (English or Persian). Reply with the words only. If nobody speaks, reply with nothing." },
            { "type": "input_audio", "input_audio": { "data": base64::engine::general_purpose::STANDARD.encode(wav), "format": "wav" } } ] }],
    });
    let mut response =
        agent(Duration::from_secs(120)).post(&url("/v1/chat/completions")).send_json(&body).context("the local model did not answer")?;
    let reply: Value = response.body_mut().read_json()?;
    Ok(reply["choices"][0]["message"]["content"].as_str().unwrap_or_default().trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_are_read_from_the_show_reply() {
        let reply = json!({ "capabilities": ["completion", "vision", "audio", "tools", "thinking"] });
        assert_eq!(parse_capabilities(&reply), vec!["completion", "vision", "audio", "tools", "thinking"]);
        assert!(parse_capabilities(&json!({})).is_empty());
    }

    #[test]
    fn helper_tools_expose_their_name_and_declared_arguments() {
        let helper = crate::helpers::Helper {
            name: "project-tests".into(),
            description: "Run tests".into(),
            arguments: vec!["path".into()],
            command: vec!["/bin/echo".into()],
            cwd: None,
            timeout_secs: 10,
            risk: "changes".into(),
            dry_run: None,
            source: std::path::PathBuf::new(),
        };
        let tool = helper_tool(0, &helper);
        assert_eq!(tool["function"]["name"], "helper_0");
        assert!(tool["function"]["description"].as_str().unwrap().contains("project-tests"));
        assert_eq!(tool["function"]["parameters"]["required"][0], "path");
    }

    #[test]
    fn cloud_models_are_not_local() {
        assert!(is_cloud_model("deepseek-v4.1-flash:1017-cloud"));
        assert!(is_cloud_model("kimi-k3.1-pro:cloud"));
        assert!(is_cloud_model("some-model-cloud"));
        assert!(!is_cloud_model("gemma4:e4b"));
        assert!(!is_cloud_model("qwen3-vl:8b"));
    }
}

/// Asks a local vision model about an image and returns its answer as JSON matching `schema`.
pub fn ask_about_image(model: &str, image: &[u8], question: &str, schema: Value) -> Result<Value> {
    let installed = models()?;
    if !installed.iter().any(|m| m == model) {
        bail!("the model '{model}' is not installed in Ollama (installed: {})", installed.join(", "));
    }
    if !supports(model, "vision")? {
        bail!("the model '{model}' has no vision; choose one that does");
    }
    let body = json!({
        "model": model, "stream": false, "think": false, "format": schema, "keep_alive": KEEP_ALIVE, "options": { "temperature": 0, "num_predict": 160, "num_ctx": NUM_CTX },
        "messages": [{ "role": "user", "content": question, "images": [base64::engine::general_purpose::STANDARD.encode(image)] }],
    });
    let mut response = agent(Duration::from_secs(180)).post(&url("/api/chat")).send_json(&body).context("the local model did not answer")?;
    let reply: Value = response.body_mut().read_json()?;
    let text = reply["message"]["content"].as_str().context("the local model sent no answer")?;
    serde_json::from_str(text).with_context(|| format!("the local model's answer was not the JSON asked for: {text}"))
}

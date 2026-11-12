//! Local models, reached through Ollama on this machine. Optional: nothing in
//! Buddy's ordinary life calls this, and every caller copes with it being absent.

use anyhow::{Context, Result, bail};
use base64::Engine;
use serde_json::{Value, json};
use std::time::Duration;

const URL: &str = "http://127.0.0.1:11434";

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into()
}

/// The models Ollama has installed, or an error saying why it cannot be reached.
pub fn models() -> Result<Vec<String>> {
    let tags: Value = agent(Duration::from_secs(2))
        .get(&format!("{URL}/api/tags"))
        .call()
        .context("Ollama is not running on this machine")?
        .body_mut()
        .read_json()?;
    Ok(tags["models"].as_array().into_iter().flatten().filter_map(|m| m["name"].as_str().map(str::to_string)).collect())
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
         - chat: said to him, but nothing to do. unknown: none of these.",
        tricks.join(", "),
        if taught.is_empty() {
            String::new()
        } else {
            format!(" Words they have taught him, which also count as cues (put the word in `cue`): {}.", taught.join(", "))
        }
    );
    let kinds = ["cue", "praise", "treat", "approval", "correction", "stop", "quiet", "remember", "teach", "chat", "unknown"];
    let schema = json!({ "type": "object", "required": ["kind", "trick", "cue", "text"], "properties": {
        "kind": { "type": "string", "enum": kinds }, "trick": { "type": "string" }, "cue": { "type": "string" }, "text": { "type": "string" } } });
    let body = json!({
        "model": model, "stream": false, "think": false, "format": schema, "options": { "temperature": 0, "num_predict": 120 },
        "messages": [{ "role": "system", "content": system }, { "role": "user", "content": said }],
    });
    let mut response = agent(Duration::from_secs(120)).post(&format!("{URL}/api/chat")).send_json(&body).context("the local model did not answer")?;
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
        "chat" => Heard::Chat,
        _ => Heard::Unknown,
    })
}

/// Has a local model write down what was said in a short recording (16 kHz WAV).
/// Returns an empty string if it heard no speech.
pub fn transcribe(model: &str, wav: &[u8]) -> Result<String> {
    let body = json!({
        "model": model, "temperature": 0, "reasoning_effort": "none", "max_tokens": 120,
        "messages": [{ "role": "user", "content": [
            { "type": "text", "text": "Write down exactly what is said in this recording, in the language spoken (English or Persian). Reply with the words only. If nobody speaks, reply with nothing." },
            { "type": "input_audio", "input_audio": { "data": base64::engine::general_purpose::STANDARD.encode(wav), "format": "wav" } } ] }],
    });
    let mut response =
        agent(Duration::from_secs(120)).post(&format!("{URL}/v1/chat/completions")).send_json(&body).context("the local model did not answer")?;
    let reply: Value = response.body_mut().read_json()?;
    Ok(reply["choices"][0]["message"]["content"].as_str().unwrap_or_default().trim().to_string())
}

/// Asks a local vision model about an image and returns its answer as JSON matching `schema`.
pub fn ask_about_image(model: &str, image: &[u8], question: &str, schema: Value) -> Result<Value> {
    let installed = models()?;
    if !installed.iter().any(|m| m == model) {
        bail!("the model '{model}' is not installed in Ollama (installed: {})", installed.join(", "));
    }
    let body = json!({
        "model": model, "stream": false, "think": false, "format": schema, "options": { "temperature": 0, "num_predict": 160 },
        "messages": [{ "role": "user", "content": question, "images": [base64::engine::general_purpose::STANDARD.encode(image)] }],
    });
    let mut response = agent(Duration::from_secs(180)).post(&format!("{URL}/api/chat")).send_json(&body).context("the local model did not answer")?;
    let reply: Value = response.body_mut().read_json()?;
    let text = reply["message"]["content"].as_str().context("the local model sent no answer")?;
    serde_json::from_str(text).with_context(|| format!("the local model's answer was not the JSON asked for: {text}"))
}

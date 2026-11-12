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

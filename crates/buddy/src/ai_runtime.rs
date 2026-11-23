//! Buddy's managed Ollama runtime: where it lives, which port it uses, and what the
//! installer recorded about it. The daemon never starts the runtime itself; systemd owns
//! the process. This module only reads the manifest and reports state.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11435";
pub const DEFAULT_SERVICE: &str = "buddy-ollama.service";

/// What the installer knows about the runtime. Written by `scripts/install.sh`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Runtime {
    pub schema: u32,
    pub ollama_version: String,
    pub tarball_sha256: String,
    pub endpoint: String,
    pub service: String,
    pub models_dir: PathBuf,
    pub model: Option<String>,
    pub model_digest: Option<String>,
    pub installed_at: Option<i64>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            schema: 1,
            ollama_version: String::new(),
            tarball_sha256: String::new(),
            endpoint: DEFAULT_ENDPOINT.to_string(),
            service: DEFAULT_SERVICE.to_string(),
            models_dir: buddy_core::paths::ollama_models_dir(),
            model: None,
            model_digest: None,
            installed_at: None,
        }
    }
}

impl Runtime {
    pub fn path() -> PathBuf {
        buddy_core::paths::runtime_manifest_path()
    }

    /// Read the manifest at the usual place, or return the default managed settings.
    pub fn load_or_default() -> Self {
        Self::load_from(&Self::path()).unwrap_or_default()
    }

    /// Read a manifest from `path`. Missing files are an error; callers that want a
    /// default use `load_or_default`.
    pub fn load_from(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("no runtime manifest at {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("the runtime manifest at {} is not valid JSON", path.display()))
    }

    /// Write the manifest atomically, so a crash cannot leave half a selection behind.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("could not create {}", parent.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec(self)?).with_context(|| format!("could not write {}", tmp.display()))?;
        fs::rename(&tmp, path).with_context(|| format!("could not replace {}", path.display()))
    }

    /// The endpoint Buddy should talk to. `BUDDY_OLLAMA_URL` wins so tests and a
    /// developer can point at another instance without touching the manifest.
    pub fn endpoint() -> String {
        if let Ok(url) = std::env::var("BUDDY_OLLAMA_URL") {
            let url = url.trim().trim_end_matches('/');
            if !url.is_empty() {
                return url.to_string();
            }
        }
        Self::load_or_default().endpoint.trim_end_matches('/').to_string()
    }
}

/// Pick the first free loopback port in `start..=end`. Used by the Rust-side setup
/// path and by tests; the shell installer has its own probe. The caller persists the
/// answer; this function never reserves the port, so another process can still take it.
#[allow(dead_code)]
pub fn choose_port(start: u16, end: u16) -> Option<u16> {
    (start..=end).find(|port| TcpListener::bind(("127.0.0.1", *port)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_port_in_use_is_skipped() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert_eq!(choose_port(port, port), None, "a port someone else holds is not chosen");
        drop(listener);
        assert_eq!(choose_port(port, port), Some(port), "once free, the port is chosen");
    }

    #[test]
    fn the_manifest_is_read_and_missing_ones_have_a_default() {
        let mut path = std::env::temp_dir();
        path.push(format!("buddy-runtime-{}.json", std::process::id()));
        let mut file = fs::File::create(&path).unwrap();
        write!(
            file,
            r#"{{"endpoint":"http://127.0.0.1:11435","service":"buddy-ollama.service","models_dir":"/tmp/buddy-models","model":"gemma4:e4b"}}"#
        )
        .unwrap();
        drop(file);

        let runtime = Runtime::load_from(&path).unwrap();
        assert_eq!(runtime.endpoint, "http://127.0.0.1:11435");
        assert_eq!(runtime.service, "buddy-ollama.service");
        assert_eq!(runtime.models_dir, PathBuf::from("/tmp/buddy-models"));
        assert_eq!(runtime.model.as_deref(), Some("gemma4:e4b"));
        fs::remove_file(&path).unwrap();

        let missing = Runtime::load_from(&path);
        assert!(missing.is_err(), "a missing manifest is reported, not invented");
        assert_eq!(Runtime::default().endpoint, DEFAULT_ENDPOINT);
    }

    #[test]
    fn saving_creates_the_runtime_directory() {
        let dir = std::env::temp_dir().join(format!("buddy-runtime-dir-{}", std::process::id()));
        let path = dir.join("runtime.json");
        let mut runtime = Runtime::default();
        runtime.model = Some("lfm3:1.7b".into());
        runtime.save_to(&path).unwrap();
        assert!(path.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_selected_model_is_saved_and_read_back() {
        let mut path = std::env::temp_dir();
        path.push(format!("buddy-runtime-selected-{}.json", std::process::id()));
        let mut runtime = Runtime::default();
        runtime.model = Some("gemma4:e4b".into());
        runtime.model_digest = Some("sha256:abc".into());
        runtime.save_to(&path).unwrap();
        let read = Runtime::load_from(&path).unwrap();
        assert_eq!(read.model.as_deref(), Some("gemma4:e4b"));
        assert_eq!(read.model_digest.as_deref(), Some("sha256:abc"));
        fs::remove_file(&path).unwrap();
    }
}

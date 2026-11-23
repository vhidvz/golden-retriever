use std::env;
use std::path::PathBuf;

fn home() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_else(|| "/".into()))
}

/// Where Buddy's database and backups live. `BUDDY_DATA_DIR` overrides it (tests, second instances).
pub fn data_dir() -> PathBuf {
    if let Some(d) = env::var_os("BUDDY_DATA_DIR") {
        return PathBuf::from(d);
    }
    env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".local/share")).join("buddy")
}

pub fn db_path() -> PathBuf {
    data_dir().join("buddy.db")
}

/// The runtime, models and manifest Buddy installs for itself.
pub fn ollama_dir() -> PathBuf {
    data_dir().join("ollama")
}

pub fn ollama_models_dir() -> PathBuf {
    ollama_dir().join("models")
}

pub fn runtime_manifest_path() -> PathBuf {
    ollama_dir().join("runtime.json")
}

/// Optional motion data, kept separate from the runtime and never a dependency of the daemon.
pub fn ai4animation_dir() -> PathBuf {
    data_dir().join("ai4animation")
}

/// Control socket. Lives in the per-user runtime dir so only this user can reach it.
pub fn socket_path() -> PathBuf {
    if let Some(p) = env::var_os("BUDDY_SOCKET") {
        return PathBuf::from(p);
    }
    env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(env::temp_dir).join("buddy").join("buddy.sock")
}

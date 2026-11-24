//! Records the git tag/commit in `buddy version`, including when built from a dirty tree.
//! A source tarball without `.git` falls back to `unknown`; BUDDY_GIT_DESCRIBE overrides it.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/tags");
    println!("cargo:rerun-if-env-changed=BUDDY_GIT_DESCRIBE");
    let describe = std::env::var("BUDDY_GIT_DESCRIBE").ok().or_else(git_describe).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BUDDY_GIT_DESCRIBE={describe}");
}

fn git_describe() -> Option<String> {
    let output = Command::new("git").args(["describe", "--tags", "--always", "--dirty"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

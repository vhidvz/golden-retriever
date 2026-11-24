//! Keeps the renderer process running. Buddy's life does not depend on it: if
//! it crashes or Godot is missing, the daemon carries on and tries again.

use buddy_core::paths;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const FIRST_RETRY: Duration = Duration::from_secs(2);
const MAX_RETRY: Duration = Duration::from_secs(60);

pub struct Renderer {
    child: Option<Child>,
    started: Instant,
    retry_at: Instant,
    backoff: Duration,
    complained: bool,
    wanted: bool,
}

pub fn find_godot() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("BUDDY_GODOT") {
        return Some(PathBuf::from(p));
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).flat_map(|dir| ["godot", "godot4"].map(|name| dir.join(name))).find(|p| p.is_file())
}

/// Where the renderer project may live, in precedence order. A checkout has
/// `target/<profile>/buddy` and the project at the repository root; a system package has
/// `/usr/bin/buddy` and `/usr/share/buddy/renderer`; a user-local install keeps it in the
/// data directory.
fn project_candidates(exe: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(exe) = exe {
        for ancestor in exe.ancestors() {
            candidates.push(ancestor.join("renderer"));
            candidates.push(ancestor.join("share/buddy/renderer"));
        }
    }
    candidates.push(paths::data_dir().join("renderer"));
    candidates
}

pub fn find_project() -> Option<PathBuf> {
    let is_project = |dir: &Path| dir.join("project.godot").is_file();
    if let Some(dir) = std::env::var_os("BUDDY_RENDERER_DIR").map(PathBuf::from) {
        return is_project(&dir).then_some(dir);
    }
    let exe = std::env::current_exe().ok();
    project_candidates(exe.as_deref()).into_iter().find(|dir| is_project(dir))
}

impl Renderer {
    pub fn new() -> Self {
        let now = Instant::now();
        Self { child: None, started: now, retry_at: now, backoff: FIRST_RETRY, complained: false, wanted: true }
    }

    pub fn status(&mut self) -> &'static str {
        if !self.wanted {
            return "stopped while he is hidden";
        }
        match self.child.as_mut().map(|c| c.try_wait()) {
            Some(Ok(None)) => "running",
            Some(_) => "restarting",
            None if self.complained => "unavailable (Godot or the renderer project was not found)",
            None => "starting",
        }
    }

    /// Call regularly: keeps the renderer running while it is wanted (restarting it with a
    /// growing delay if it exits) and stops it while it is not.
    pub fn poll(&mut self, wanted: bool) {
        self.wanted = wanted;
        if !wanted {
            self.stop();
            self.backoff = FIRST_RETRY;
            self.retry_at = Instant::now();
            return;
        }
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(None) => return,
                outcome => {
                    // A long healthy run earns a quick restart; a crash loop backs off.
                    if self.started.elapsed() > MAX_RETRY {
                        self.backoff = FIRST_RETRY;
                    }
                    eprintln!("buddy: renderer exited ({outcome:?}); restarting in {:?}", self.backoff);
                    self.retry_at = Instant::now() + self.backoff;
                    self.backoff = (self.backoff * 2).min(MAX_RETRY);
                    self.child = None;
                }
            }
        }
        if Instant::now() < self.retry_at {
            return;
        }
        let (Some(godot), Some(project)) = (find_godot(), find_project()) else {
            if !self.complained {
                eprintln!("buddy: no renderer (set BUDDY_GODOT / BUDDY_RENDERER_DIR); Buddy keeps living without being drawn");
                self.complained = true;
            }
            self.retry_at = Instant::now() + MAX_RETRY;
            return;
        };
        // The X11 driver is deliberate. Under XWayland the renderer's unfocusable window is
        // override-redirect, which on GNOME is what makes it always on top, on every
        // workspace, self-positioning and never focused (measured in spikes/overlay-gnome).
        let spawned = Command::new(&godot)
            .arg("--path")
            .arg(&project)
            .args(["--display-driver", "x11"])
            .env("BUDDY_SOCKET", paths::socket_path())
            .env("BUDDY_SUPERVISED", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn();
        match spawned {
            Ok(child) => {
                self.child = Some(child);
                self.started = Instant::now();
                self.complained = false;
            }
            Err(e) => {
                eprintln!("buddy: could not start {}: {e}", godot.display());
                self.retry_at = Instant::now() + self.backoff;
            }
        }
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_prefix_and_checkout_are_searched_for_the_project() {
        let prefix = project_candidates(Some(Path::new("/usr/bin/buddy")));
        assert!(prefix.contains(&PathBuf::from("/usr/share/buddy/renderer")), "a packaged binary must find /usr/share/buddy/renderer");
        let checkout = project_candidates(Some(Path::new("/work/golden-retriever/target/release/buddy")));
        assert!(checkout.contains(&PathBuf::from("/work/golden-retriever/renderer")), "a checkout must still find its renderer");
    }
}

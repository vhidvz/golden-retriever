//! Draws his pictures with a real coat (tools/bake_sprites.py) while the user is away.
//!
//! Drawing a stage of his growth takes hours of the processor, so it is only done
//! when nobody is using the computer: it starts when the user has gone (the screen
//! is locked, or nothing has been touched for ten minutes), is paused the moment they
//! are back (the process is stopped, not killed, and goes on later where it was), and
//! never runs at more than the lowest priority. `buddy set pictures off` stops it
//! altogether; `buddy set pictures always` lets it run while the user is here too.
//!
//! His growth is divided into a few stages. The pictures for the stage he is in come
//! first, then those for the next, so they are ready before he gets there. Each set is
//! named after the stage and after what changes every picture in it (the model, the
//! drawing script, his size): when any of those changes, the set is drawn again, and
//! the old one stays in use until the new one is complete. A change to how he moves
//! (the pose recorder, the animation code) is not that drastic: the set is brought up
//! to date in place, and only the movements whose poses changed, or that are new, are
//! drawn again.

use crate::renderer;
use buddy_core::paths;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The stages of his growth that get pictures of their own. The renderer uses a set
/// while he is within 0.08 of it, so neighbouring stages overlap a little.
pub const STAGES: [f64; 9] = [0.02, 0.15, 0.28, 0.41, 0.54, 0.67, 0.80, 0.92, 1.0];
const CHECK_EVERY: Duration = Duration::from_secs(60);
const RETRY_AFTER_FAILURE: Duration = Duration::from_secs(30 * 60);

/// The stage whose pictures fit him best at this growth.
pub fn stage_for(growth: f64) -> f64 {
    let mut best = STAGES[0];
    for stage in STAGES {
        if (stage - growth).abs() < (best - growth).abs() {
            best = stage;
        }
    }
    best
}

/// The stage to draw next: his own if it is not done, else the one after it if that is
/// not done, else none.
pub fn wanted_stage(growth: f64, done: impl Fn(f64) -> bool) -> Option<f64> {
    let now = stage_for(growth);
    if !done(now) {
        return Some(now);
    }
    STAGES.iter().copied().find(|&s| s > now).filter(|&s| !done(s))
}

/// The folder name of a set: the stage in hundredths, and what it was drawn from.
pub fn set_name(stage: f64, version: &str) -> String {
    format!("growth-{:03}-{version}", (stage * 100.0).round() as u32)
}

/// A short, stable fingerprint (FNV-1a, 32 bits) of the given byte strings.
pub fn fingerprint(parts: &[&[u8]]) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for part in parts {
        for &byte in part.iter().chain(&[0xff]) {
            hash ^= u32::from(byte);
            hash = hash.wrapping_mul(0x0100_0193);
        }
    }
    format!("{hash:08x}")
}

/// What the drawing needs: Godot (to record the poses), Blender (to draw), the renderer
/// project and the drawing script.
struct Tools {
    godot: PathBuf,
    blender: PathBuf,
    project: PathBuf,
    script: PathBuf,
}

fn find_tools() -> Result<Tools, &'static str> {
    let on_path = |name: &str| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).map(|dir| dir.join(name)).find(|p| p.is_file())
    };
    let godot = renderer::find_godot().ok_or("Godot is not installed")?;
    let blender = std::env::var_os("BUDDY_BLENDER").map(PathBuf::from).or_else(|| on_path("blender")).ok_or("Blender is not installed")?;
    let project = renderer::find_project().ok_or("the renderer project was not found")?;
    // Next to the renderer: tools/ in a checkout, or ~/.local/share/buddy/tools when installed.
    let script = project.parent().map(|p| p.join("tools").join("bake_sprites.py")).filter(|p| p.is_file()).ok_or("tools/bake_sprites.py was not found")?;
    Ok(Tools { godot, blender, project, script })
}

#[derive(PartialEq)]
enum Step {
    Recording,
    Drawing,
}

struct Job {
    stage: f64,
    name: String,
    child: Child,
    step: Step,
    paused: bool,
    work: PathBuf,
}

pub struct Baker {
    job: Option<Job>,
    next_check: Instant,
    /// What to tell the user about the pictures, kept up to date.
    note: String,
    /// When the progress of a running job was last counted (it means listing folders).
    counted: Instant,
}

fn sprites_dir() -> PathBuf {
    paths::data_dir().join("sprites")
}

fn work_dir(name: &str) -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache"))
        .join("buddy/bake")
        .join(name)
}

fn manifest(dir: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(dir.join("manifest.json")).ok()?).ok()
}

/// How many pictures a set has, and how many it will have.
fn progress(dir: &Path) -> (usize, usize) {
    let Some(m) = manifest(dir) else { return (0, 0) };
    let clips = m["clips"].as_array().cloned().unwrap_or_default();
    let total = clips.iter().map(|c| c["frames"].as_u64().unwrap_or(0) as usize).sum();
    let done = clips
        .iter()
        .filter_map(|c| c["name"].as_str())
        .map(|clip| fs::read_dir(dir.join(clip)).map(|d| d.filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().ends_with(".webp") && !e.file_name().to_string_lossy().contains(".part")).count()).unwrap_or(0))
        .sum();
    (done, total)
}

/// Whether a set has every picture, drawn from the movements as they are now.
fn complete(dir: &Path, poses_version: &str) -> bool {
    manifest(dir).is_some_and(|m| m["complete"] == true && m["poses_version"] == poses_version)
}

impl Baker {
    pub fn new() -> Self {
        Self { job: None, next_check: Instant::now(), note: "not drawn yet".into(), counted: Instant::now() - CHECK_EVERY }
    }

    /// The fingerprint of what every picture of a set depends on: the body, the way it is
    /// drawn, his size. A change means a new set.
    fn version(tools: &Tools, size_px: f64) -> String {
        let read = |p: PathBuf| fs::read(p).unwrap_or_default();
        let size = format!("{size_px:.0}");
        fingerprint(&[&read(tools.project.join("dog/retriever.json")), &read(tools.script.clone()), size.as_bytes()])
    }

    /// The fingerprint of how he moves: the pose recorder and the animation code. A change
    /// means the set is brought up to date, redrawing only the movements that changed.
    fn poses_version(tools: &Tools) -> String {
        let read = |name: &str| fs::read(tools.project.join(name)).unwrap_or_default();
        fingerprint(&[&read("export_poses.gd"), &read("dog.gd"), &read("retriever.gd")])
    }

    /// Call regularly. `growth` and `size_px` are his; `away` is whether the user has gone;
    /// `mode` is the setting: "away" (default), "always" or "off".
    pub fn poll(&mut self, growth: f64, size_px: f64, away: bool, mode: &str) {
        if mode == "off" {
            if self.job.is_some() {
                self.stop();
            }
            self.note = "off (turn on with: buddy set pictures away)".into();
            return;
        }
        let may_run = away || mode == "always";
        if let Some(job) = self.job.as_mut() {
            match job.child.try_wait() {
                Ok(None) => {
                    if may_run == job.paused {
                        // Stopped, not killed: it carries on later exactly where it was.
                        let signal = if may_run { libc::SIGCONT } else { libc::SIGSTOP };
                        // SAFETY: the pid is that of our own child, which has not been reaped.
                        unsafe { libc::kill(job.child.id() as libc::pid_t, signal) };
                        job.paused = !may_run;
                        self.counted = Instant::now() - Duration::from_secs(10);     // say so at once
                    }
                    if self.counted.elapsed() < Duration::from_secs(10) {
                        return;
                    }
                    self.counted = Instant::now();
                    let (done, total) = progress(&sprites_dir().join(&job.name));
                    self.note = format!(
                        "drawing the pictures for {:.0}% grown: {} of {}{}",
                        job.stage * 100.0,
                        done,
                        total,
                        if job.paused { ", paused while you are here" } else if job.step == Step::Recording { ", recording his movements" } else { "" }
                    );
                    return;
                }
                Ok(Some(exit)) if exit.success() && job.step == Step::Recording => {
                    let job = self.job.take().expect("a job");
                    self.draw(job);
                    return;
                }
                Ok(Some(exit)) => {
                    let job = self.job.take().expect("a job");
                    let finished = find_tools().is_ok_and(|tools| complete(&sprites_dir().join(&job.name), &Self::poses_version(&tools)));
                    if finished {
                        self.retire_older(job.stage, &job.name);
                        self.next_check = Instant::now();
                    } else {
                        eprintln!("buddy: drawing pictures stopped ({exit}); see {}", job.work.join("bake.log").display());
                        self.note = format!("drawing stopped with an error; trying again later (log: {})", job.work.join("bake.log").display());
                        self.next_check = Instant::now() + RETRY_AFTER_FAILURE;
                    }
                    return;
                }
                Err(e) => {
                    eprintln!("buddy: lost track of the drawing process: {e}");
                    self.job = None;
                    return;
                }
            }
        }
        if Instant::now() < self.next_check {
            return;
        }
        self.next_check = Instant::now() + CHECK_EVERY;
        let tools = match find_tools() {
            Ok(tools) => tools,
            Err(why) => {
                self.note = format!("unavailable: {why}");
                return;
            }
        };
        let version = Self::version(&tools, size_px);
        let poses_version = Self::poses_version(&tools);
        let done = |stage: f64| complete(&sprites_dir().join(set_name(stage, &version)), &poses_version);
        let Some(stage) = wanted_stage(growth, done) else {
            self.note = format!("ready for {:.0}% grown and the next stage", stage_for(growth) * 100.0);
            return;
        };
        if !may_run {
            let (drawn, total) = progress(&sprites_dir().join(set_name(stage, &version)));
            self.note = if total > 0 {
                format!("drawing the pictures for {:.0}% grown: {drawn} of {total}, waiting until you are away", stage * 100.0)
            } else {
                format!("the pictures for {:.0}% grown will be drawn while you are away", stage * 100.0)
            };
            return;
        }
        self.record(&tools, stage, size_px, &version);
    }

    /// First step: record his movements at that stage of growth (a few seconds).
    fn record(&mut self, tools: &Tools, stage: f64, size_px: f64, version: &str) {
        let name = set_name(stage, version);
        let work = work_dir(&name);
        if let Err(e) = fs::create_dir_all(&work) {
            self.note = format!("could not create {}: {e}", work.display());
            return;
        }
        let spawned = Command::new("nice")
            .args(["-n", "19"])
            .arg(&tools.godot)
            .args(["--headless", "--path"])
            .arg(&tools.project)
            .args(["-s", "res://export_poses.gd", "--", "--growth", &format!("{stage}"), "--size", &format!("{size_px}"), "--out"])
            .arg(work.join("poses.json"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match spawned {
            Ok(child) => self.job = Some(Job { stage, name, child, step: Step::Recording, paused: false, work }),
            Err(e) => self.note = format!("could not start Godot: {e}"),
        }
    }

    /// Second step: draw them (hours, resumable).
    fn draw(&mut self, recorded: Job) {
        let Ok(tools) = find_tools() else { return };
        let out = sprites_dir().join(&recorded.name);
        let log = fs::File::create(recorded.work.join("bake.log"));
        let version = recorded.name.rsplit('-').next().unwrap_or_default().to_string();
        let poses_version = Self::poses_version(&tools);
        let spawned = log.and_then(|log| {
            Command::new("nice")
                .args(["-n", "19"])
                .arg(&tools.blender)
                .args(["-b", "--factory-startup", "--python"])
                .arg(&tools.script)
                .arg("--")
                .arg(recorded.work.join("poses.json"))
                .arg(&out)
                .args(["--version", &version, "--poses-version", &poses_version])
                .stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log)
                .spawn()
        });
        match spawned {
            Ok(child) => self.job = Some(Job { child, step: Step::Drawing, ..recorded }),
            Err(e) => self.note = format!("could not start Blender: {e}"),
        }
    }

    /// A new set for a stage is complete: the sets it replaces (drawn from older versions
    /// of the model or the script) are removed.
    fn retire_older(&mut self, stage: f64, keep: &str) {
        let prefix = format!("growth-{:03}", (stage * 100.0).round() as u32);
        let Ok(entries) = fs::read_dir(sprites_dir()) else { return };
        for entry in entries.filter_map(|e| e.ok()) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) && name != keep {
                let path = entry.path();
                // A link (made by hand, for testing) is removed, never what it points at.
                let gone = if path.is_symlink() { fs::remove_file(&path) } else { fs::remove_dir_all(&path) };
                if let Err(e) = gone {
                    eprintln!("buddy: could not remove the old pictures {}: {e}", path.display());
                }
            }
        }
    }

    pub fn note(&self) -> &str {
        &self.note
    }

    pub fn stop(&mut self) {
        if let Some(mut job) = self.job.take() {
            // SAFETY: as above; a stopped process must be woken for the kill to be delivered at once.
            unsafe { libc::kill(job.child.id() as libc::pid_t, libc::SIGCONT) };
            let _ = job.child.kill();
            let _ = job.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_growth_has_a_stage_close_enough_for_the_renderer() {
        for k in 0..=100 {
            let growth = f64::from(k) / 100.0;
            assert!((stage_for(growth) - growth).abs() <= 0.08, "{growth}");
        }
        assert_eq!(stage_for(0.0175), 0.02);
        assert_eq!(stage_for(0.5), 0.54);
        assert_eq!(stage_for(1.0), 1.0);
    }

    #[test]
    fn his_own_stage_comes_first_then_the_next() {
        assert_eq!(wanted_stage(0.03, |_| false), Some(0.02));
        assert_eq!(wanted_stage(0.03, |s| s == 0.02), Some(0.15));
        assert_eq!(wanted_stage(0.03, |s| s <= 0.15), None);
        assert_eq!(wanted_stage(1.0, |_| true), None);
        assert_eq!(wanted_stage(1.0, |s| s < 1.0), Some(1.0));
    }

    #[test]
    fn a_set_is_complete_only_for_the_movements_as_they_are_now() {
        let dir = std::env::temp_dir().join(format!("buddy-baker-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let write = |json: &str| fs::write(dir.join("manifest.json"), json).unwrap();
        write(r#"{"complete": true, "poses_version": "aaaa1111"}"#);
        assert!(complete(&dir, "aaaa1111"));
        assert!(!complete(&dir, "bbbb2222"), "drawn from older movements: to be brought up to date");
        write(r#"{"complete": false, "poses_version": "aaaa1111"}"#);
        assert!(!complete(&dir, "aaaa1111"));
        write(r#"{"complete": true}"#);
        assert!(!complete(&dir, "aaaa1111"), "a set from before poses were versioned is brought up to date");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_set_is_named_after_its_stage_and_what_it_was_drawn_from() {
        assert_eq!(set_name(0.02, "abcd1234"), "growth-002-abcd1234");
        assert_eq!(set_name(1.0, "abcd1234"), "growth-100-abcd1234");
        let a = fingerprint(&[b"model", b"script"]);
        assert_eq!(a, fingerprint(&[b"model", b"script"]));
        assert_ne!(a, fingerprint(&[b"model", b"script2"]));
        assert_ne!(fingerprint(&[b"ab", b"c"]), fingerprint(&[b"a", b"bc"]));
        assert_eq!(a.len(), 8);
    }
}

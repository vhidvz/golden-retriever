//! Phase 1's exit criterion as a test: Buddy's identity, age, experience and
//! settings survive the daemon being stopped and started again, whether it was
//! asked to quit or terminated by the session ending.

use serde_json::Value;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("buddy-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn buddy(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_buddy"));
        cmd.env("BUDDY_DATA_DIR", self.dir.join("data")).env("BUDDY_SOCKET", self.dir.join("s.sock"));
        cmd
    }

    fn start(&self) -> Child {
        let mut child = self.buddy().args(["daemon", "--no-renderer"]).stderr(Stdio::null()).spawn().unwrap();
        for _ in 0..100 {
            if self.buddy().arg("status").output().unwrap().status.success() {
                return child;
            }
            sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("daemon did not come up");
    }

    fn run(&self, args: &[&str]) -> String {
        let out = self.buddy().args(args).output().unwrap();
        assert!(out.status.success(), "buddy {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    }

    fn json(&self, args: &[&str]) -> Value {
        serde_json::from_str(&self.run(args)).unwrap()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = self.buddy().arg("quit").output();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn buddy_is_the_same_dog_after_a_restart() {
    let sandbox = Sandbox::new("restart");

    let mut first = sandbox.start();
    sandbox.run(&["set", "name", "Rex"]);
    sandbox.run(&["set", "adult_days", "30"]);
    sandbox.run(&["privacy", "on"]);
    sandbox.run(&["sleep"]);
    let born = sandbox.json(&["export"])["state"]["identity"]["born_at"].clone();
    assert!(born.as_i64().unwrap() > 0);
    sandbox.run(&["quit"]);
    assert!(first.wait().unwrap().success());

    // A second daemon on the same data must refuse to start while one is running.
    let mut second = sandbox.start();
    assert!(!sandbox.buddy().args(["daemon", "--no-renderer"]).stderr(Stdio::null()).status().unwrap().success());

    let status = sandbox.json(&["status", "--json"]);
    assert_eq!(status["state"]["name"], "Rex");
    assert_eq!(status["life"]["sessions"], 2);
    assert_eq!(status["settings"]["adult_days"], 30.0);
    assert_eq!(status["state"]["privacy"], true);
    assert_eq!(status["state"]["activity"], "sleep", "he was sent to sleep and stays asleep");
    assert_eq!(sandbox.json(&["export"])["state"]["identity"]["born_at"], born, "restarting must never rebirth him");

    // The session ending (SIGTERM) is a clean shutdown too.
    assert!(Command::new("kill").args(["-TERM", &second.id().to_string()]).status().unwrap().success());
    assert!(second.wait().unwrap().success());

    let mut third = sandbox.start();
    let kinds: Vec<String> =
        sandbox.json(&["export"])["journal"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap().to_string()).collect();
    assert_eq!(kinds.iter().filter(|k| *k == "born").count(), 1);
    assert_eq!(kinds.iter().filter(|k| *k == "session_start").count(), 3);
    assert_eq!(kinds.iter().filter(|k| *k == "session_end").count(), 2, "both earlier sessions closed cleanly: {kinds:?}");
    sandbox.run(&["quit"]);
    third.wait().unwrap();
}

#[test]
fn what_he_was_taught_and_told_survives_a_restart() {
    let sandbox = Sandbox::new("lessons");

    let mut first = sandbox.start();
    sandbox.run(&["say", "وقتی می‌گم بپر، یعنی بچرخ"]);
    sandbox.run(&["teach", "bang", "roll_over"]);
    sandbox.run(&["say", "Remember that I prefer working late."]);
    assert!(!sandbox.buddy().args(["teach", "bang", "moonwalk"]).output().unwrap().status.success(), "a trick he does not have is refused");
    sandbox.run(&["quit"]);
    first.wait().unwrap();

    let mut second = sandbox.start();
    let knows = sandbox.run(&["knows"]);
    assert!(knows.lines().any(|l| l.contains("بپر") && l.contains("spin") && l.contains("told")), "{knows}");
    assert!(knows.lines().any(|l| l.starts_with("bang") && l.contains("roll_over")), "{knows}");
    let memories = sandbox.run(&["memories"]);
    assert!(memories.contains("I prefer working late."), "{memories}");
    assert_eq!(sandbox.json(&["status", "--json"])["settings"]["night_shift_hours"], 2.0, "and it still shapes his day");

    // The user can take back both a word and a memory.
    sandbox.run(&["unlearn", "bang"]);
    sandbox.run(&["forget", "1"]);
    assert!(!sandbox.run(&["knows"]).contains("bang"));
    assert!(sandbox.run(&["memories"]).contains("has not been told to remember anything"));
    sandbox.run(&["quit"]);
    second.wait().unwrap();
}

#[test]
fn he_does_computer_tasks_with_no_model_at_all_and_only_with_a_yes() {
    let sandbox = Sandbox::new("skills");
    let docs = sandbox.dir.join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("tax report 2025.txt"), "numbers").unwrap();

    let mut daemon = sandbox.start();
    // No local model: only his own vocabulary is left to work things out.
    sandbox.run(&["set", "language_model", "no-such-model"]);

    // Looking is allowed, but only where he has been let in.
    let refused = sandbox.buddy().args(["say", "find my tax report"]).output().unwrap();
    assert!(!refused.status.success() && String::from_utf8_lossy(&refused.stderr).contains("any folders"));
    sandbox.run(&["folders", "add", docs.to_str().unwrap()]);
    assert!(sandbox.run(&["say", "find my tax report"]).contains("tax report 2025.txt"));
    assert!(sandbox.run(&["say", "tax report رو پیدا کن"]).contains("tax report 2025.txt"));

    // Changing something waits for a yes, and can be taken back.
    let asked = sandbox.run(&["say", "make a note that the build is broken"]);
    assert!(asked.contains("waiting for your answer"), "{asked}");
    let notes = sandbox.dir.join("data/notes.md");
    assert!(!notes.exists(), "nothing is written before the yes");
    assert!(sandbox.run(&["say", "yes"]).contains("Done"));
    assert!(std::fs::read_to_string(&notes).unwrap().contains("the build is broken"));
    sandbox.run(&["undo"]);
    assert!(!std::fs::read_to_string(&notes).unwrap().contains("the build is broken"));

    // A no is a no, and "always" for a sensitive skill is refused.
    sandbox.run(&["say", "یادداشت کن فردا جلسه دارم"]);
    assert!(sandbox.run(&["say", "نه ممنون"]).contains("will not"));
    assert!(!sandbox.run(&["waiting"]).contains("جلسه"));
    assert!(!sandbox.buddy().args(["skill", "run_command", "allow"]).output().unwrap().status.success());

    // What his vocabulary cannot place, with no model to ask, leaves him honestly puzzled.
    let lost = sandbox.run(&["say", "would you kindly reorganise my holiday photographs by year"]);
    assert!(lost.contains("does not know what you want"), "{lost}");

    // Permissions survive a restart.
    sandbox.run(&["skill", "make_note", "allow"]);
    sandbox.run(&["quit"]);
    daemon.wait().unwrap();
    let mut again = sandbox.start();
    assert!(sandbox.run(&["skills"]).lines().any(|l| l.starts_with("make_note") && l.contains("allow")));
    assert!(sandbox.run(&["folders"]).contains("docs"));
    sandbox.run(&["quit"]);
    again.wait().unwrap();
}

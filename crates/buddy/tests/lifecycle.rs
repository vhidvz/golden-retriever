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

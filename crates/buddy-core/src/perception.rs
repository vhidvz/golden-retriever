//! What Buddy can perceive, who allowed it, and what each perception means to him.
//!
//! Every source has its own switch. Sources that only see *that* something is
//! happening (the user is active, a window is fullscreen) are on by default;
//! sources that see *content* (the screen, the camera, the microphone, window
//! titles) are off until the user turns them on. Privacy mode overrides all of them.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Senses {
    /// Idle time, screen lock, system load. No content of any kind.
    pub system: bool,
    /// Which application is in front, fullscreen state, pointer position, that a notification arrived.
    pub desktop: bool,
    /// The titles of windows, which can contain document names and page titles.
    pub window_titles: bool,
    /// What is drawn on the screen.
    pub screen: bool,
    pub camera: bool,
    pub microphone: bool,
    /// The semantic accessibility tree, used only by guided workflow capture.
    pub accessibility: bool,
}

impl Default for Senses {
    fn default() -> Self {
        Self { system: true, desktop: true, window_titles: false, screen: false, camera: false, microphone: false, accessibility: false }
    }
}

pub const SOURCES: [&str; 7] = ["system", "desktop", "window_titles", "screen", "camera", "microphone", "accessibility"];

impl Senses {
    pub fn get(&self, source: &str) -> Option<bool> {
        Some(match source {
            "system" => self.system,
            "desktop" => self.desktop,
            "window_titles" => self.window_titles,
            "screen" => self.screen,
            "camera" => self.camera,
            "microphone" => self.microphone,
            "accessibility" => self.accessibility,
            _ => return None,
        })
    }

    pub fn set(&mut self, source: &str, on: bool) -> bool {
        let slot = match source {
            "system" => &mut self.system,
            "desktop" => &mut self.desktop,
            "window_titles" => &mut self.window_titles,
            "screen" => &mut self.screen,
            "camera" => &mut self.camera,
            "microphone" => &mut self.microphone,
            "accessibility" => &mut self.accessibility,
            _ => return false,
        };
        *slot = on;
        true
    }
}

/// Something perceived, already reduced to what it means for him.
#[derive(Clone, Debug, PartialEq)]
pub enum Stimulus {
    /// Seconds since the user last touched the keyboard or pointer.
    UserIdle(f64),
    Locked(bool),
    Fullscreen(bool),
    Overview(bool),
    /// A different application came to the front.
    Focus {
        app: String,
    },
    /// A pointer click, reduced to a button and a coarse region; never a click history.
    Click {
        button: u32,
        region: String,
    },
    Notification {
        app: String,
    },
    /// The camera sees someone, or no longer does.
    Presence(bool),
    /// A sound, 0 (barely audible) ..= 1 (very loud).
    Sound {
        level: f64,
    },
    /// He looked at the screen and this is what he made of it.
    Glimpse {
        summary: String,
    },
    /// How hard the machine is working, 0..=1.
    Load(f64),
}

impl Stimulus {
    /// The sense a stimulus needs to be allowed.
    pub fn source(&self) -> &'static str {
        match self {
            Stimulus::UserIdle(_) | Stimulus::Locked(_) | Stimulus::Load(_) => "system",
            Stimulus::Fullscreen(_) | Stimulus::Overview(_) | Stimulus::Focus { .. } | Stimulus::Notification { .. } | Stimulus::Click { .. } => {
                "desktop"
            }
            Stimulus::Presence(_) => "camera",
            Stimulus::Sound { .. } => "microphone",
            Stimulus::Glimpse { .. } => "screen",
        }
    }

    /// Reads a stimulus from a provider's message. Unknown kinds are ignored, so
    /// providers can grow without breaking an older daemon.
    pub fn parse(kind: &str, data: &Value) -> Option<Self> {
        let text = |key: &str| data[key].as_str().map(str::to_string);
        Some(match kind {
            "idle" => Stimulus::UserIdle(data["secs"].as_f64()?),
            "locked" => Stimulus::Locked(data["on"].as_bool()?),
            "fullscreen" => Stimulus::Fullscreen(data["on"].as_bool()?),
            "overview" => Stimulus::Overview(data["on"].as_bool()?),
            "focus" => Stimulus::Focus { app: text("app")? },
            "click" => Stimulus::Click { button: data["button"].as_u64()? as u32, region: text("region").unwrap_or_default() },
            "notification" => Stimulus::Notification { app: text("app").unwrap_or_default() },
            "presence" => Stimulus::Presence(data["on"].as_bool()?),
            "sound" => Stimulus::Sound { level: data["level"].as_f64()?.clamp(0.0, 1.0) },
            "glimpse" => Stimulus::Glimpse { summary: text("summary")? },
            "load" => Stimulus::Load(data["level"].as_f64()?.clamp(0.0, 1.0)),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn content_sources_are_off_until_asked_for() {
        let s = Senses::default();
        assert!(s.system && s.desktop);
        assert!(!s.window_titles && !s.screen && !s.camera && !s.microphone);
        for source in SOURCES {
            assert!(s.get(source).is_some());
        }
    }

    #[test]
    fn every_stimulus_names_the_sense_that_must_allow_it() {
        let seen = [
            Stimulus::parse("idle", &json!({ "secs": 3.0 })),
            Stimulus::parse("focus", &json!({ "app": "org.gnome.Nautilus" })),
            Stimulus::parse("click", &json!({ "button": 272, "region": "3:2" })),
            Stimulus::parse("presence", &json!({ "on": true })),
            Stimulus::parse("sound", &json!({ "level": 7.0 })),
            Stimulus::parse("glimpse", &json!({ "summary": "a terminal" })),
        ];
        let sources: Vec<_> = seen.iter().map(|s| s.as_ref().unwrap().source()).collect();
        assert_eq!(sources, ["system", "desktop", "desktop", "camera", "microphone", "screen"]);
        assert_eq!(seen[4], Some(Stimulus::Sound { level: 1.0 }), "levels are clamped");
        assert_eq!(Stimulus::parse("something_new", &json!({})), None);
        assert_eq!(Stimulus::parse("focus", &json!({})), None, "a malformed message is dropped, not guessed at");
    }

    #[test]
    fn accessibility_is_off_by_default_and_has_its_own_switch() {
        let mut senses = Senses::default();
        assert!(!senses.accessibility);
        assert!(senses.set("accessibility", true));
        assert!(senses.accessibility);
        assert!(SOURCES.contains(&"accessibility"));
    }
}

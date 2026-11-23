//! `buddy` — the daemon that keeps Buddy alive, and the command-line remote for it.

mod ai;
mod ai_catalog;
mod ai_hardware;
mod ai_runtime;
mod ai_setup;
mod baker;
mod client;
mod daemon;
mod renderer;
mod skills;
mod system;

use anyhow::Result;
use serde_json::{Value, json};

const USAGE: &str = "\
buddy — a dog who lives on your desktop

Usage:
  buddy daemon [--no-renderer]   run Buddy (normally started by the systemd user service)
  buddy status [--json]          how he is, what is running, what is being observed
  buddy hide [MINUTES]           hide him, for a while or until `buddy show`
  buddy show                     bring him back
  buddy sleep | wake             send him to sleep / wake him
  buddy feed | water | praise | treat
                                 put food out, put water out, tell him he is good, give a treat
  buddy say 'WORDS'              say something to him, in English or Persian
  buddy teach 'WORD' TRICK       tell him outright what a new word means
  buddy knows                    the words he knows and how well
  buddy unlearn 'WORD'           make him forget a word
  buddy memories | forget N      what he was told to remember; forget one of them

On the computer (try: buddy say 'find my tax report'):
  buddy waiting                  what he would like to do and is waiting for a yes to
  buddy approve [N] [--always]   yes (the latest, or number N); --always: no need to ask again
  buddy deny [N] [--never]       no; --never: and do not offer or ask that again
  buddy skills                   what he can do and whether he asks first
  buddy workflows                the application routines he has recorded
  buddy workflow record NAME     record the applications you work through, then stop
  buddy workflow stop | cancel
  buddy workflow show NAME|ID    one routine and how it has gone
  buddy workflow replay NAME|ID  replay it, checking each application came to the front
  buddy workflow delete NAME|ID
  buddy skill NAME ask|allow|deny
  buddy folders [add|remove DIR] the folders he may search, read and move files in
  buddy undo                     take back the last change he made
  buddy result                   the full output of the last thing he did
  buddy providers                which local AI runtime and model are available
  buddy ai status | doctor       how Buddy's managed local AI runtime is doing
  buddy ai setup [--yes] [--vision] [--audio] [--model NAME] [--accept-license]
                                 recommend and install a model for this machine
  buddy ai model [NAME]          show or change the selected model
  buddy ai repair | prune        re-pull the selected model / remove unused models
  buddy voice on|off             listen through the selected audio model / stop listening
  buddy quiet on|off             no sounds and no asking for attention
  buddy why                      what he is doing, why, and how he feels
  buddy privacy on|off           suspend / resume every perception source
  buddy senses                   what he may perceive, and what is actually reporting
  buddy look [--file IMAGE]      have him look at the screen once (or at an image) and say what he makes of it
  buddy sense SOURCE on|off      system | desktop | window_titles | screen | camera | microphone
  buddy set KEY VALUE            name | size_px (adult height) | adult_days (growth timeline) |
                                 stay_in_fullscreen (on/off) | vision_model | language_model | audio_model |
                                 pictures (away: draw his pictures only while you are away,
                                 the default; always; off)
  buddy snapshot [FILE.png]      save exactly what his window shows right now (nothing else on
                                 the screen), with what he was doing, to report a drawing problem
  buddy journal [N]              the last N things that happened (default 20)
  buddy export                   everything he knows, as JSON
  buddy quit                     stop the daemon
  buddy simulate [DAYS] [--user attentive|absent|office] [--age DAYS] [--seed N]
                                 live a stretch of his life offline and report how varied it was
";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    if arg(0) != Some("daemon") {
        // A command whose output is piped into `head` should end quietly when the pipe closes.
        // (Not for the daemon: there a client hanging up must never be fatal.)
        // SAFETY: called once, before any other thread exists; SIG_DFL is always a valid handler.
        unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
    }
    let command: (&str, Value) = match (arg(0), arg(1)) {
        (Some("daemon"), flag) => return daemon::run(flag == Some("--no-renderer")),
        (Some("status"), flag) => return client::status(flag == Some("--json")),
        (Some("hide"), minutes) => ("hide", json!({ "minutes": minutes.and_then(|m| m.parse::<f64>().ok()) })),
        (Some("show"), _) => ("show", Value::Null),
        (Some("simulate"), _) => return client::simulate(&args[1..]),
        (Some("why"), _) => return client::why(),
        (Some("senses"), _) => return client::senses(),
        (Some("look"), flag) => return client::look(if flag == Some("--file") { arg(2) } else { None }),
        (Some("sense"), Some(source)) if matches!(arg(2), Some("on" | "off")) => ("sense", json!({ "source": source, "on": arg(2) == Some("on") })),
        (Some(simple @ ("feed" | "water" | "praise" | "treat")), _) => (simple, Value::Null),
        (Some("say"), Some(_)) => return client::say(&args[1..].join(" ")),
        (Some("teach"), Some(cue)) if arg(2).is_some() => ("teach", json!({ "cue": cue, "trick": arg(2) })),
        (Some("knows"), _) => return client::knows(),
        (Some("waiting"), _) => return client::listing("waiting"),
        (Some("skills"), _) => return client::listing("skills"),
        (Some("workflows"), _) => return client::workflows(),
        (Some("workflow"), Some("record")) => return client::workflow_record("start", arg(2)),
        (Some("workflow"), Some("stop")) => return client::workflow_record("stop", None),
        (Some("workflow"), Some("cancel")) => return client::workflow_record("cancel", None),
        (Some("workflow"), Some("show")) if arg(2).is_some() => return client::workflow_show(arg(2).unwrap()),
        (Some("workflow"), Some("replay")) if arg(2).is_some() => return client::act("replay_workflow", json!({ "workflow": arg(2).unwrap() })),
        (Some("workflow"), Some("delete")) if arg(2).is_some() => return client::workflow_delete(arg(2).unwrap()),
        (Some("providers"), _) => return client::providers(),
        (Some("ai"), Some("status")) => return client::ai_status(),
        (Some("ai"), Some("doctor")) => return client::ai_doctor(),
        (Some("ai"), Some("setup")) => return ai_setup::setup(&args[2..]),
        (Some("ai"), Some("model")) => return ai_setup::model(&args[2..]),
        (Some("ai"), Some("repair")) => return ai_setup::repair(),
        (Some("ai"), Some("prune")) => return ai_setup::prune(&args[2..]),
        (Some(which @ ("approve" | "deny")), _) => {
            let id = args[1..].iter().find_map(|a| a.parse::<u64>().ok());
            let lasting = args[1..].iter().any(|a| a == "--always" || a == "--never");
            return client::act(which, json!({ "id": id, "lasting": lasting }));
        }
        (Some("skill"), Some(skill)) if arg(2).is_some() => ("skill", json!({ "name": skill, "policy": arg(2) })),
        (Some("folders"), Some(how @ ("add" | "remove"))) if arg(2).is_some() => ("folders", json!({ how: arg(2).map(client::absolute) })),
        (Some("folders"), None) => ("folders", Value::Null),
        (Some("undo"), _) => ("undo", Value::Null),
        (Some("result"), _) => return client::result(),
        (Some("unlearn"), Some(cue)) => ("unlearn", json!({ "cue": cue })),
        (Some("memories"), _) => return client::memories(),
        (Some("forget"), Some(id)) => ("forget", json!({ "id": id.parse::<i64>().ok() })),
        (Some("voice"), Some(v @ ("on" | "off"))) => return client::voice(v == "on"),
        (Some("quiet"), Some(v @ ("on" | "off"))) => ("quiet", json!({ "on": v == "on" })),
        (Some("sleep"), _) => ("sleep", Value::Null),
        (Some("wake"), _) => ("wake", Value::Null),
        (Some("privacy"), Some(v @ ("on" | "off"))) => ("privacy", json!({ "on": v == "on" })),
        (Some("set"), Some(key)) if arg(2).is_some() => ("set", json!({ "key": key, "value": arg(2) })),
        (Some("journal"), n) => ("journal", json!({ "limit": n.and_then(|n| n.parse::<u64>().ok()).unwrap_or(20) })),
        (Some("export"), _) => ("export", Value::Null),
        (Some("snapshot"), file) => ("snapshot", json!({ "path": client::absolute(file.unwrap_or("buddy-snapshot.png")) })),
        (Some("quit"), _) => ("quit", Value::Null),
        _ => {
            print!("{USAGE}");
            return Ok(());
        }
    };
    client::run(command.0, command.1)
}

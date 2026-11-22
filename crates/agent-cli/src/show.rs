//! The status as the person reads it in a terminal.

use std::fmt::Write;

use serde_json::Value;

fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}

fn on(v: &Value, key: &str) -> &'static str {
    if v[key].as_bool() == Some(true) {
        "on"
    } else {
        "off"
    }
}

/// The agent as `label (name)`.
pub fn agent(st: &Value) -> String {
    format!("{} ({})", s(&st["agent"], "label"), s(&st["agent"], "name"))
}

/// A program's or a skill's standing, as a tag.
fn tag(status: &str) -> &'static str {
    match status {
        "approved" => "allowed",
        "waiting" => "WAITING",
        "changed" => "CHANGED",
        "missing-program" => "MISSING",
        _ => "?",
    }
}

pub fn folders(st: &Value) -> String {
    let mut out = String::new();
    let list = st["folders"].as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        out.push_str("No folders. Add one: wysebee-agent folder add <path> [--write]\n");
    }
    for f in list {
        let mode = if s(&f, "mode") == "write" { "read and write" } else { "read only" };
        let gone = if f["present"].as_bool() == Some(false) { "  (not there now)" } else { "" };
        let _ = writeln!(out, "  {}. {} — {} [{mode}] id {}{gone}", f["number"], s(&f, "name"), s(&f, "path"), s(&f, "id"));
        if !s(&f, "description").is_empty() {
            let _ = writeln!(out, "     {}", s(&f, "description"));
        }
    }
    out
}

/// Where an address came from, as the person reads it.
fn source(st: &Value) -> &'static str {
    match s(st, "cloud_url_source") {
        "saved" => "saved with `wysebee-agent server`",
        "env" => "WYSEBEE_CLOUD_URL",
        _ => "Wysebee Cloud's own",
    }
}

/// Which Wysebee Cloud server this machine pairs with, and how to change it.
pub fn server(st: &Value) -> String {
    let mut out = String::new();
    if s(st, "state") == "paired" {
        let _ = writeln!(out, "Server: {} (this pairing's; it keeps it until unpaired)", s(st, "cloud_url"));
        let _ = writeln!(out, "To use another, unpair first: wysebee-agent unpair, then wysebee-agent server <url>.");
        return out;
    }
    let _ = writeln!(out, "Server: {} ({})", s(st, "cloud_url"), source(st));
    if s(st, "cloud_url_source") == "saved" {
        let _ = writeln!(out, "Default: {} (wysebee-agent server default goes back to it)", s(st, "cloud_default"));
    } else {
        let _ = writeln!(out, "Change it: wysebee-agent server <url> (https://…, or http:// to this machine only)");
    }
    out
}

pub fn status(st: &Value) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Wysebee Agent for Linux {} on {}", s(st, "version"), s(st, "mac_name"));
    let _ = writeln!(out, "Server: {} ({})", s(st, "cloud_url"), if s(st, "state") == "paired" { "this pairing's" } else { source(st) });
    match s(st, "state") {
        "paired" => {
            let link = if st["connected"].as_bool() == Some(true) { "connected" } else { "NOT connected (retrying)" };
            let _ = writeln!(out, "Paired with the agent {} — {link}", agent(st));
        }
        "waiting" => {
            let minutes = st["expires_in"].as_u64().unwrap_or(0).div_ceil(60);
            let _ = writeln!(out, "Pairing as \"{}\": type {} in Wysebee, under Devices ({minutes} min left)", s(st, "name"), s(st, "code"));
        }
        "refused" => {
            let _ = writeln!(out, "Pairing refused: {}", s(st, "message"));
        }
        _ => {
            let _ = writeln!(out, "Not paired. Pair: wysebee-agent pair \"<agent name>\"");
            if !s(st, "note").is_empty() {
                let _ = writeln!(out, "  {}", s(st, "note"));
            }
        }
    }
    if st["paused"].as_bool() == Some(true) {
        let _ = writeln!(out, "PAUSED: nothing runs for Wysebee (wysebee-agent resume)");
    }
    let _ = writeln!(out, "Switches: any-program {}, developer {}, android {}", on(st, "any_program"), on(st, "dev_access"), on(st, "android_dev"));

    let programs = st["programs"].as_array().cloned().unwrap_or_default();
    let _ = writeln!(out, "\nPrograms ({}):", programs.len());
    for p in &programs {
        let _ = writeln!(out, "  [{}] {} — {}", tag(s(p, "status")), s(p, "name"), s(p, "command_line"));
    }
    let skills = st["skills"].as_array().cloned().unwrap_or_default();
    let _ = writeln!(out, "Skills ({}):", skills.len());
    for k in &skills {
        let scripts: Vec<&str> = k["scripts"].as_array().map(|l| l.iter().map(|x| s(x, "name")).collect()).unwrap_or_default();
        let _ = writeln!(out, "  [{}] {} — scripts: {}", tag(s(k, "status")), s(k, "name"), if scripts.is_empty() { "-".into() } else { scripts.join(", ") });
        if let Some(e) = k["download_error"].as_str() {
            let _ = writeln!(out, "     download failed: {e}");
        }
    }
    let _ = writeln!(out, "Folders:");
    out.push_str(&folders(st));
    let waiting = st["waiting"].as_u64().unwrap_or(0);
    if waiting > 0 {
        let _ = writeln!(out, "\n{waiting} wait for your Allow: wysebee-agent allow <name>");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_paired_status_lists_what_waits() {
        let st = json!({"version": "0.1.0", "mac_name": "vm1", "cloud_url": "https://app.wysebee.com", "cloud_url_source": "default", "state": "paired", "connected": true, "agent": {"name": "ops", "label": "Ops"}, "any_program": true, "dev_access": false, "android_dev": false, "paused": false,
            "programs": [{"name": "greet", "command_line": "echo hi <who>", "status": "waiting"}], "skills": [{"name": "w", "status": "approved", "scripts": [{"name": "run"}]}],
            "folders": [{"number": 1, "id": "a1", "name": "Code", "path": "/srv/code", "mode": "write", "description": "", "present": true}], "waiting": 1});
        let out = status(&st);
        assert!(out.contains("Paired with the agent Ops (ops) — connected"), "{out}");
        assert!(out.contains("Switches: any-program on, developer off, android off"));
        assert!(out.contains("[WAITING] greet — echo hi <who>"));
        assert!(out.contains("[allowed] w — scripts: run"));
        assert!(out.contains("1. Code — /srv/code [read and write] id a1"));
        assert!(out.contains("1 wait for your Allow"));
        assert!(out.contains("Server: https://app.wysebee.com (this pairing's)"), "{out}");
    }

    #[test]
    fn the_server_says_where_it_comes_from_and_how_to_change_it() {
        let unpaired = |source: &str| json!({"state": "unpaired", "cloud_url": "https://cloud.example", "cloud_url_source": source, "cloud_default": "https://app.wysebee.com"});
        let saved = server(&unpaired("saved"));
        assert!(saved.contains("Server: https://cloud.example (saved with `wysebee-agent server`)"), "{saved}");
        assert!(saved.contains("Default: https://app.wysebee.com (wysebee-agent server default goes back to it)"));
        assert!(server(&unpaired("env")).contains("(WYSEBEE_CLOUD_URL)"));
        let builtin = server(&unpaired("default"));
        assert!(builtin.contains("(Wysebee Cloud's own)") && builtin.contains("Change it: wysebee-agent server <url>"), "{builtin}");
        let paired = server(&json!({"state": "paired", "cloud_url": "https://cloud.example", "cloud_url_source": "saved"}));
        assert!(paired.contains("Server: https://cloud.example (this pairing's; it keeps it until unpaired)") && paired.contains("unpair first"), "{paired}");
        assert!(status(&unpaired("env")).contains("Server: https://cloud.example (WYSEBEE_CLOUD_URL)"));
    }
}

//! `wysebee-agent`: Wysebee Agent for Linux's command line, over `agent-core`.
//!
//! - **`wysebee-agent run`** is the agent itself: what systemd starts (`service.rs`), one per
//!   account and state folder. It holds the [`Agent`], polls Wysebee Cloud and answers the
//!   person's commands on the state folder's socket (`agent_core::control`).
//! - **Every other command** is the person's, what the Mac's and Windows' window does:
//!   pairing, Allow, folders, the switches, pausing. Each asks the running agent; none of
//!   them widens what runs, and Allow and the switches ask the person to confirm first
//!   (`--yes` for scripts and cloud-init).
//! - **`service install`** sets the agent up to start with the machine: a systemd unit run as
//!   the person, never as root unless asked for by name.
//!
//! What is real here and handed to the core: the state folder
//! (`~/.local/state/wysebee-agent`; `WYSEBEE_AGENT_STATE_DIR` moves it), the person's home,
//! the `PATH` plus the system's, the person's and the Android SDK's folders, the secret file,
//! the machine's host name and `WYSEBEE_CLOUD_URL`.

mod run;
mod service;
mod show;

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::time::Duration;

use agent_core::approvals;
use agent_core::control;
use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};

/// Moves the state folder (a second copy beside the installed one, for development).
pub const STATE_DIR_ENV: &str = "WYSEBEE_AGENT_STATE_DIR";

#[derive(Parser)]
#[command(name = "wysebee-agent", version, about = "Wysebee Agent for Linux: lets one Wysebee agent run the programs, skill scripts and folder tools you allow on this machine.", after_help = "Start here:\n  sudo wysebee-agent service install    start the agent with the machine\n  wysebee-agent pair \"My server\"        pair it with Wysebee\n  wysebee-agent status                  see what waits for your Allow")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the agent in the foreground (what the service starts).
    Run,
    /// Show the pairing, the switches, the programs, skills and folders.
    Status {
        /// The whole status as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Pair with Wysebee as a new agent: shows a code to type in Wysebee (Devices).
    Pair {
        /// The agent's name in Wysebee.
        name: String,
        /// Show the code and return, without waiting for it to be typed.
        #[arg(long)]
        no_wait: bool,
    },
    /// Stop waiting for the pairing code to be typed.
    Cancel,
    /// Unpair this machine: everything allowed here is forgotten.
    Unpair {
        #[arg(long, short)]
        yes: bool,
    },
    /// Allow a program or a skill (by its name in Wysebee) to run on this machine.
    Allow {
        name: String,
        /// The name is a skill's (when a program has the same name).
        #[arg(long)]
        skill: bool,
        #[arg(long, short)]
        yes: bool,
    },
    /// Take a program's or a skill's approval back.
    Revoke {
        name: String,
        #[arg(long)]
        skill: bool,
    },
    /// The folders the agent's folder tools may use.
    #[command(subcommand)]
    Folder(FolderCommand),
    /// Turn on one of the wider permissions (asks first).
    Enable {
        switch: Switch,
        #[arg(long, short)]
        yes: bool,
    },
    /// Turn one of the wider permissions off.
    Disable { switch: Switch },
    /// Stop running anything for Wysebee (stops what runs now).
    Pause,
    /// Run programs for Wysebee again.
    Resume,
    /// Show or set which Wysebee Cloud a new pairing uses (only while unpaired).
    Cloud {
        /// The address (https://…); `default` for Wysebee Cloud's own.
        url: Option<String>,
    },
    /// Start the agent with the machine (systemd), or stop doing so. Needs sudo.
    #[command(subcommand)]
    Service(service::ServiceCommand),
    /// Show the agent's log (journalctl).
    Logs {
        #[arg(long, short)]
        follow: bool,
    },
}

#[derive(Subcommand)]
enum FolderCommand {
    /// List the folders.
    List,
    /// Add a folder (read only unless --write).
    Add {
        path: PathBuf,
        #[arg(long, default_value = "")]
        name: String,
        /// Let the agent write in it too.
        #[arg(long)]
        write: bool,
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Change a folder's name, permission or description (by its id).
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, conflicts_with = "read")]
        write: bool,
        #[arg(long)]
        read: bool,
        #[arg(long)]
        description: Option<String>,
    },
    /// Remove a folder (by its id); nothing in it is touched.
    Remove { id: String },
}

#[derive(Clone, Copy, ValueEnum)]
enum Switch {
    /// Let Wysebee run any program on this machine (the AI names it).
    AnyProgram,
    /// Let Wysebee use this machine as a developer (shell, files, jobs anywhere).
    Developer,
    /// Let Wysebee use the Android phones plugged in to this machine (adb).
    Android,
}

impl Switch {
    fn key(self) -> &'static str {
        match self {
            Switch::AnyProgram => "any_program",
            Switch::Developer => "dev_access",
            Switch::Android => "android_dev",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Switch::AnyProgram => "Let Wysebee run any program on this machine",
            Switch::Developer => "Let Wysebee use this machine as a developer",
            Switch::Android => "Let Wysebee use Android phones on this machine",
        }
    }

    /// What the person is told before turning it on.
    fn warning(self) -> &'static str {
        match self {
            Switch::AnyProgram => "The AI will name programs and their arguments itself, and they run as you, in your home folder, for up to 60 seconds. Disk, boot, account, package and shell tools are refused, but this is a safeguard, not a sandbox.",
            Switch::Developer => "The AI will run bash commands and read, write and edit files anywhere this account can, as you, and start background jobs. Nothing checks what it runs.",
            Switch::Android => "The AI will use adb on the phones plugged in to this machine with USB debugging on: read their screens and logs, tap and type, install and remove apps, and run shell commands on them.",
        }
    }
}

/// The person's home folder: `$HOME`, which systemd sets for the service's user too.
pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| PathBuf::from("/"))
}

/// The state folder: `WYSEBEE_AGENT_STATE_DIR`, else `~/.local/state/wysebee-agent`.
pub fn state_dir() -> PathBuf {
    std::env::var_os(STATE_DIR_ENV).map(PathBuf::from).unwrap_or_else(|| approvals::default_dir(&home()))
}

/// Ask the running agent.
fn ask(req: Value) -> Result<Value, String> {
    ask_waiting(req, Duration::from_secs(30))
}

fn ask_waiting(req: Value, wait: Duration) -> Result<Value, String> {
    control::ask(&state_dir(), &req, wait)
}

/// Ask the person a yes-or-no question; `yes` answers it. Without a terminal, only `yes` does.
pub fn confirm(question: &str, yes: bool) -> Result<bool, String> {
    if yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        return Err("Nobody is there to confirm: add --yes.".into());
    }
    print!("{question} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer).map_err(|e| e.to_string())?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn main() {
    let cli = Cli::parse();
    let done = match cli.command {
        Command::Run => run::run(),
        Command::Status { json } => status(json),
        Command::Pair { name, no_wait } => pair(&name, no_wait),
        Command::Cancel => ask(json!({"cmd": "cancel"})).map(|_| println!("Pairing cancelled.")),
        Command::Unpair { yes } => unpair(yes),
        Command::Allow { name, skill, yes } => allow(&name, skill, yes),
        Command::Revoke { name, skill } => revoke(&name, skill),
        Command::Folder(f) => folder(f),
        Command::Enable { switch, yes } => enable(switch, yes),
        Command::Disable { switch } => ask(json!({"cmd": "switch", "name": switch.key(), "on": false})).map(|_| println!("Off: {}.", switch.label())),
        Command::Pause => ask(json!({"cmd": "pause"})).map(|_| println!("Paused: nothing runs for Wysebee until `wysebee-agent resume`.")),
        Command::Resume => ask(json!({"cmd": "resume"})).map(|_| println!("Running programs for Wysebee again.")),
        Command::Cloud { url } => cloud(url),
        Command::Service(s) => service::command(s),
        Command::Logs { follow } => service::logs(follow),
    };
    if let Err(e) = done {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn status(as_json: bool) -> Result<(), String> {
    let s = ask(json!({"cmd": "status"}))?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&s).unwrap_or_default());
    } else {
        print!("{}", show::status(&s));
    }
    Ok(())
}

fn pair(name: &str, no_wait: bool) -> Result<(), String> {
    ask(json!({"cmd": "pair", "name": name}))?;
    let mut shown = false;
    loop {
        let s = ask(json!({"cmd": "status"}))?;
        match s["state"].as_str().unwrap_or("") {
            "paired" => {
                println!("Paired: this machine runs programs for the agent {}.", show::agent(&s));
                println!("Programs and skills wait for your Allow: see `wysebee-agent status`.");
                return Ok(());
            }
            "waiting" if !shown => {
                let minutes = s["expires_in"].as_u64().unwrap_or(0).div_ceil(60);
                println!("Type this code in Wysebee, under Devices:\n\n    {}\n", s["code"].as_str().unwrap_or("?"));
                println!("It runs out in {minutes} minute{}.", if minutes == 1 { "" } else { "s" });
                if no_wait {
                    println!("`wysebee-agent status` shows whether it was typed.");
                    return Ok(());
                }
                println!("Waiting for it to be typed (Ctrl+C stops waiting; the code stays good)…");
                shown = true;
            }
            "waiting" => {}
            "refused" => return Err(format!("Wysebee refused the pairing: {}", s["message"].as_str().unwrap_or(""))),
            _ => return Err(s["note"].as_str().unwrap_or("The pairing ended.").to_string()),
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn unpair(yes: bool) -> Result<(), String> {
    if !confirm("Unpair this machine? Everything allowed here is forgotten.", yes)? {
        return Ok(());
    }
    ask(json!({"cmd": "unpair"}))?;
    println!("Unpaired. The agent stays in Wysebee, without this machine.");
    Ok(())
}

/// Allow a program (or a skill), after showing what it will run.
fn allow(name: &str, skill: bool, yes: bool) -> Result<(), String> {
    let s = ask(json!({"cmd": "status"}))?;
    let found = |list: &str| s[list].as_array().and_then(|l| l.iter().find(|p| p["name"] == name)).cloned();
    if let Some(p) = found("programs").filter(|_| !skill) {
        println!("Program '{name}': {}", p["description"].as_str().unwrap_or(""));
        println!("  runs   {}", p["command_line"].as_str().unwrap_or(""));
        println!("  found  {}", p["path"].as_str().unwrap_or("nowhere on this machine yet"));
        println!("  Blanks in <angle brackets> are filled by the AI; the rest is fixed.");
        if !confirm("Allow it to run on this machine?", yes)? {
            return Ok(());
        }
        ask(json!({"cmd": "allow", "name": name}))?;
        println!("Allowed: '{name}'.");
        return Ok(());
    }
    if let Some(k) = found("skills") {
        println!("Skill '{name}'. Its scripts:");
        for script in k["scripts"].as_array().into_iter().flatten() {
            println!("  {}  runs  {}", script["name"].as_str().unwrap_or(""), script["command"].as_str().unwrap_or(""));
        }
        if k["scripts"].is_null() {
            println!("  (not read yet: they are read from its SKILL.md when it is allowed)");
        }
        for c in k["changes"].as_array().into_iter().flatten() {
            println!("  {} {}", c["change"].as_str().unwrap_or(""), c["path"].as_str().unwrap_or(""));
        }
        if !confirm("Allow its scripts to run on this machine, with the arguments the AI gives?", yes)? {
            return Ok(());
        }
        // Its files may need downloading first.
        ask_waiting(json!({"cmd": "allow_skill", "name": name}), Duration::from_secs(600))?;
        println!("Allowed: the skill '{name}'.");
        return Ok(());
    }
    Err(format!("This agent has no program or skill '{name}'. `wysebee-agent status` lists them."))
}

fn revoke(name: &str, skill: bool) -> Result<(), String> {
    let s = ask(json!({"cmd": "status"}))?;
    let is_program = !skill && s["programs"].as_array().is_some_and(|l| l.iter().any(|p| p["name"] == name));
    ask(json!({"cmd": if is_program { "revoke" } else { "revoke_skill" }, "name": name}))?;
    println!("'{name}' is no longer allowed here.");
    Ok(())
}

fn folder(f: FolderCommand) -> Result<(), String> {
    match f {
        FolderCommand::List => {
            let s = ask(json!({"cmd": "status"}))?;
            print!("{}", show::folders(&s));
        }
        FolderCommand::Add { path, name, write, description } => {
            let path = std::path::absolute(&path).map_err(|e| e.to_string())?;
            let added = ask(json!({"cmd": "add_folder", "path": path.display().to_string(), "name": name, "mode": if write { "write" } else { "read" }, "description": description}))?;
            println!("Added {} ({}), id {}.", path.display(), if write { "read and write" } else { "read only" }, added["id"].as_str().unwrap_or("?"));
        }
        FolderCommand::Update { id, name, write, read, description } => {
            let s = ask(json!({"cmd": "status"}))?;
            let now = s["folders"].as_array().and_then(|l| l.iter().find(|f| f["id"] == id.as_str())).cloned().ok_or(format!("There is no folder '{id}' here."))?;
            let mode = if write { "write" } else if read { "read" } else { now["mode"].as_str().unwrap_or("read") };
            let name = name.unwrap_or_else(|| now["name"].as_str().unwrap_or("").into());
            let description = description.unwrap_or_else(|| now["description"].as_str().unwrap_or("").into());
            ask(json!({"cmd": "update_folder", "id": id, "name": name, "mode": mode, "description": description}))?;
            println!("Changed the folder {id}.");
        }
        FolderCommand::Remove { id } => {
            ask(json!({"cmd": "remove_folder", "id": id}))?;
            println!("Removed the folder {id}; nothing in it was touched.");
        }
    }
    Ok(())
}

fn enable(switch: Switch, yes: bool) -> Result<(), String> {
    println!("{}.\n{}", switch.label(), switch.warning());
    println!("It stays on until `wysebee-agent disable`, and is forgotten on unpairing.");
    if !confirm("Turn it on?", yes)? {
        return Ok(());
    }
    ask(json!({"cmd": "switch", "name": switch.key(), "on": true}))?;
    println!("On. The agent in Wysebee needs the matching tool too (its Tools tab).");
    Ok(())
}

fn cloud(url: Option<String>) -> Result<(), String> {
    if let Some(url) = url {
        let url = if url == "default" { String::new() } else { url };
        ask(json!({"cmd": "cloud_url", "url": url}))?;
    }
    let s = ask(json!({"cmd": "status"}))?;
    println!("{} ({})", s["cloud_url"].as_str().unwrap_or(""), s["cloud_url_source"].as_str().unwrap_or(""));
    Ok(())
}

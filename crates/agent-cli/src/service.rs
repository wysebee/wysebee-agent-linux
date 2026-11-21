//! Starting with the machine: a systemd unit, the Linux counterpart of the Mac's LaunchAgent
//! and Windows' `Run` value.
//!
//! **A template unit, `wysebee-agent@.service`, run as the person** (`User=%i`): the
//! instance `wysebee-agent@alice` is Alice's agent, with her rights, her home and her state
//! folder, started at boot whether or not she is logged in — which a server (an Azure or AWS
//! VM) needs, and a user unit would not do without lingering. Writing it needs root (sudo);
//! the agent itself never runs as root unless `--user root` asks for it by name.
//!
//! `/etc/default/wysebee-agent` (optional) is read by every instance: an `HTTPS_PROXY` for a
//! machine behind a proxy, or `WYSEBEE_CLOUD_URL`.

use std::path::Path;
use std::process::Command;

use clap::Subcommand;

pub const UNIT_DIR: &str = "/etc/systemd/system";
pub const UNIT: &str = "wysebee-agent@.service";
pub const ENV_FILE: &str = "/etc/default/wysebee-agent";

#[derive(Subcommand)]
pub enum ServiceCommand {
    /// Start the agent now and at every boot, as you (or --user). Needs sudo.
    Install {
        /// The account it runs as (default: the one that ran sudo).
        #[arg(long)]
        user: Option<String>,
    },
    /// Stop the agent and no longer start it at boot. Needs sudo.
    Uninstall {
        #[arg(long)]
        user: Option<String>,
    },
    /// Whether the service runs.
    Status {
        #[arg(long)]
        user: Option<String>,
    },
}

/// The unit, its program the given one.
pub fn unit(exe: &Path) -> String {
    format!(
        "[Unit]
Description=Wysebee Agent for Linux (as %i)
Documentation=https://github.com/wysebee/wysebee-agent-linux
Wants=network-online.target
After=network-online.target

[Service]
Type=simple
User=%i
ExecStart=\"{}\" run
WorkingDirectory=~
Restart=always
RestartSec=5
TimeoutStopSec=10
# An HTTPS_PROXY, or WYSEBEE_CLOUD_URL, for every instance:
EnvironmentFile=-{ENV_FILE}

[Install]
WantedBy=multi-user.target
",
        exe.display()
    )
}

fn sh(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program).args(args).output().map_err(|e| format!("{program} could not be run: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!("{program} {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn is_root() -> bool {
    sh("id", &["-u"]).is_ok_and(|u| u == "0")
}

/// An account name as a unit's instance may hold it.
fn checked_user(name: &str) -> Result<String, String> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) || name.starts_with('-') {
        return Err(format!("'{name}' is not an account name."));
    }
    sh("id", &["-u", name]).map_err(|_| format!("There is no account '{name}' on this machine."))?;
    Ok(name.to_string())
}

/// Whose agent: `--user`, else the account that ran sudo, else this one — never root
/// unless named.
fn whose(user: Option<String>) -> Result<String, String> {
    if let Some(u) = user {
        return checked_user(&u);
    }
    if let Some(u) = std::env::var("SUDO_USER").ok().filter(|u| !u.is_empty() && u != "root") {
        return checked_user(&u);
    }
    let me = sh("id", &["-un"])?;
    if me == "root" {
        return Err("Say whose agent this is: --user <account>. It runs with that account's rights; `--user root` gives the AI root's, so name it only if you mean it.".into());
    }
    checked_user(&me)
}

fn instance(user: &str) -> String {
    format!("wysebee-agent@{user}.service")
}

fn systemd() -> Result<(), String> {
    if Path::new("/run/systemd/system").is_dir() {
        Ok(())
    } else {
        Err("This machine does not run systemd. Start the agent another way, for instance from cron: `@reboot wysebee-agent run >> ~/.local/state/wysebee-agent/agent.log 2>&1` (crontab -e).".into())
    }
}

pub fn command(c: ServiceCommand) -> Result<(), String> {
    match c {
        ServiceCommand::Install { user } => {
            systemd()?;
            let user = whose(user)?;
            if !is_root() {
                return Err("Setting up the service needs root: run it with sudo.".into());
            }
            let exe = std::env::current_exe().and_then(|e| e.canonicalize()).map_err(|e| format!("This program's path could not be read: {e}"))?;
            let path = Path::new(UNIT_DIR).join(UNIT);
            std::fs::write(&path, unit(&exe)).map_err(|e| format!("{} could not be written: {e}", path.display()))?;
            sh("systemctl", &["daemon-reload"])?;
            // Restarted, so a new version of the program is the one running.
            sh("systemctl", &["enable", &instance(&user)])?;
            sh("systemctl", &["restart", &instance(&user)])?;
            println!("The agent runs now as {user}, and starts with the machine ({}).", instance(&user));
            println!("Next, as {user}: wysebee-agent pair \"<agent name>\" (shows a code to type in Wysebee, under Devices), then wysebee-agent status.");
            Ok(())
        }
        ServiceCommand::Uninstall { user } => {
            systemd()?;
            let user = whose(user)?;
            if !is_root() {
                return Err("Removing the service needs root: run it with sudo.".into());
            }
            sh("systemctl", &["disable", "--now", &instance(&user)])?;
            println!("Stopped; {} no longer starts with the machine. Its pairing and approvals stay in ~{user}/.local/state/wysebee-agent.", instance(&user));
            Ok(())
        }
        ServiceCommand::Status { user } => {
            systemd()?;
            let user = whose(user)?;
            // Its own output, as systemctl prints it; its exit code says whether it runs.
            let _ = Command::new("systemctl").args(["status", "--no-pager", &instance(&user)]).status();
            Ok(())
        }
    }
}

/// The agent's log: the journal of this account's instance.
pub fn logs(follow: bool) -> Result<(), String> {
    let user = whose(None)?;
    let unit = instance(&user);
    let mut args = vec!["-u", unit.as_str(), "-n", "200", "--no-pager"];
    if follow {
        args.push("-f");
    }
    Command::new("journalctl").args(&args).status().map_err(|e| format!("journalctl could not be run: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unit_runs_this_program_as_the_instance_user_at_boot() {
        let u = unit(Path::new("/usr/local/bin/wysebee-agent"));
        assert!(u.contains("User=%i\n"));
        assert!(u.contains("ExecStart=\"/usr/local/bin/wysebee-agent\" run\n"));
        assert!(u.contains("WantedBy=multi-user.target"));
        assert!(u.contains("EnvironmentFile=-/etc/default/wysebee-agent"));
        assert_eq!(instance("alice"), "wysebee-agent@alice.service");
        assert!(checked_user("a b").is_err() && checked_user("-x").is_err() && checked_user("").is_err());
    }
}

//! How the command line reaches the running agent: a Unix socket in the state folder.
//!
//! Linux has no window: the service (`wysebee-agent run`, started by systemd) holds the
//! [`Agent`], and the person's commands (`wysebee-agent pair`, `allow`, `folder add`, …) ask
//! it through [`SOCKET`], in the state folder, which is this account's only (mode 0700, the
//! socket 0600). **What a command may do is what the Mac's and Windows' window may do,
//! nothing more**: each request is one of [`handle`]'s, mapped to one [`Agent`] method.
//!
//! One request per connection: a JSON object on one line (`{"cmd": "...", ...}`), answered
//! with one line, `{"ok": true, "result": ...}` or `{"ok": false, "error": "..."}`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::agent::Agent;

/// The socket's file name in the state folder.
pub const SOCKET: &str = "control.sock";
/// A request, at most (a folder's description is the longest thing sent).
pub const MAX_REQUEST: usize = 64 * 1024;

/// Where the socket of the state folder `state_dir` is.
pub fn socket_path(state_dir: &Path) -> PathBuf {
    state_dir.join(SOCKET)
}

fn text<'a>(req: &'a Value, key: &str) -> &'a str {
    req.get(key).and_then(Value::as_str).unwrap_or("")
}

/// One request, done by the agent. Everything the window does elsewhere, as a command.
pub async fn handle(agent: &Agent, req: &Value) -> Result<Value, String> {
    let done = Ok(json!({}));
    match text(req, "cmd") {
        "status" => serde_json::to_value(agent.status()).map_err(|e| e.to_string()),
        "pair" => agent.start_pairing(text(req, "name")).await.and(done),
        "cancel" => {
            agent.cancel_pairing().await;
            done
        }
        "unpair" => {
            agent.unpair().await;
            done
        }
        "allow" => agent.allow(text(req, "name")).and(done),
        "revoke" => {
            agent.revoke(text(req, "name"));
            done
        }
        "allow_skill" => agent.allow_skill(text(req, "name")).await.and(done),
        "revoke_skill" => {
            agent.revoke_skill(text(req, "name"));
            done
        }
        "skill_folder" => agent.skill_folder(text(req, "name")).map(|p| json!({"path": p.display().to_string()})),
        "add_folder" => agent.add_folder(text(req, "path"), text(req, "name"), text(req, "mode"), text(req, "description")).map(|id| json!({"id": id})),
        "update_folder" => agent.update_folder(text(req, "id"), text(req, "name"), text(req, "mode"), text(req, "description")).and(done),
        "remove_folder" => agent.remove_folder(text(req, "id")).and(done),
        "pause" | "resume" => {
            agent.set_paused(text(req, "cmd") == "pause").await;
            done
        }
        "switch" => {
            let on = req.get("on").and_then(Value::as_bool).ok_or("Say 'on': true or false.")?;
            match text(req, "name") {
                "any_program" => agent.set_any_program(on),
                "dev_access" => agent.set_dev_access(on),
                "android_dev" => agent.set_android_dev(on),
                other => return Err(format!("There is no switch '{other}': any_program, dev_access or android_dev.")),
            }
            done
        }
        "cloud_url" => agent.set_cloud_url(text(req, "url")).and(done),
        other => Err(format!("Wysebee Agent cannot do '{other}'.")),
    }
}

/// Answer requests on the state folder's socket for as long as the process runs. A socket
/// left by an agent that ended is replaced; the caller holds the single-instance lock, so
/// no live agent's socket is.
pub fn serve(agent: Agent, state_dir: &Path) -> std::io::Result<()> {
    crate::secrets::private_folder(state_dir)?;
    let path = socket_path(state_dir);
    match std::fs::remove_file(&path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let listener = tokio::net::UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
    tokio::spawn(async move {
        loop {
            let accepted = listener.accept().await;
            // A failed accept (too many files open) is tried again after a moment, not in a spin.
            let pause = Duration::from_millis(if accepted.is_err() { 100 } else { 0 });
            accepted.into_iter().for_each(|(stream, _)| {
                let agent = agent.clone();
                tokio::spawn(async move { answer(agent, stream).await });
            });
            tokio::time::sleep(pause).await;
        }
    });
    Ok(())
}

/// One connection: one request line, one answer line.
async fn answer(agent: Agent, stream: tokio::net::UnixStream) {
    let (read, mut write) = stream.into_split();
    let mut line = String::new();
    let mut reader = BufReader::new(read.take(MAX_REQUEST as u64));
    let body = match reader.read_line(&mut line).await {
        Err(e) => json!({"ok": false, "error": format!("The request could not be read: {e}")}),
        Ok(_) => match serde_json::from_str::<Value>(&line) {
            Err(_) => json!({"ok": false, "error": "The request is not JSON."}),
            Ok(req) => match handle(&agent, &req).await {
                Ok(result) => json!({"ok": true, "result": result}),
                Err(error) => json!({"ok": false, "error": error}),
            },
        },
    };
    let _ = write.write_all(format!("{body}\n").as_bytes()).await;
}

/// What the command line says when no agent answers on the state folder.
pub const NOT_RUNNING: &str = "Wysebee Agent is not running. Start it with `sudo systemctl start wysebee-agent@$USER` (set it up with `sudo wysebee-agent service install`), or run `wysebee-agent run` in another terminal.";

/// Ask the agent running on `state_dir` one request, and wait at most `wait` for its answer
/// (allowing a skill downloads its files: give it time). The command line's side.
pub fn ask(state_dir: &Path, req: &Value, wait: Duration) -> Result<Value, String> {
    use std::io::{BufRead, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(socket_path(state_dir)).map_err(|_| NOT_RUNNING.to_string())?;
    stream.set_read_timeout(Some(wait)).map_err(|e| e.to_string())?;
    stream.write_all(format!("{req}\n").as_bytes()).map_err(|e| format!("Wysebee Agent could not be asked: {e}"))?;
    let mut line = String::new();
    std::io::BufReader::new(stream).read_line(&mut line).map_err(|e| format!("Wysebee Agent did not answer: {e}"))?;
    let answer: Value = serde_json::from_str(&line).map_err(|_| "Wysebee Agent's answer could not be read.".to_string())?;
    match answer.get("ok") {
        Some(Value::Bool(true)) => Ok(answer.get("result").cloned().unwrap_or(Value::Null)),
        _ => Err(answer.get("error").and_then(Value::as_str).unwrap_or("Wysebee Agent refused.").to_string()),
    }
}

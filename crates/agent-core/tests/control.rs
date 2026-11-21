//! The command line's way in (control.rs): every request the window does elsewhere, through
//! the state folder's socket, and nothing else.

mod common;

use std::time::Duration;

use agent_core::control::{self, ask, handle, NOT_RUNNING};
use agent_core::Step;
use common::{fake, program, Bench};
use serde_json::{json, Value};

const WAIT: Duration = Duration::from_secs(10);

/// Ask on a blocking thread, as the command line (a separate process) does.
async fn asked(dir: &std::path::Path, req: Value) -> Result<Value, String> {
    let dir = dir.to_path_buf();
    tokio::task::spawn_blocking(move || ask(&dir, &req, WAIT)).await.unwrap()
}

#[tokio::test]
async fn the_command_line_pairs_allows_and_switches_through_the_socket() {
    let (fake, bench) = (fake().await, Bench::new());
    let agent = bench.agent(&fake);
    let dir = bench.state_dir();
    // Nothing listens yet.
    assert_eq!(asked(&dir, json!({"cmd": "status"})).await.unwrap_err(), NOT_RUNNING);

    control::serve(agent.clone(), &dir).unwrap();
    // A socket left behind is replaced; the folder and the socket are this account's only.
    control::serve(agent.clone(), &dir).unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        assert_eq!(std::fs::metadata(control::socket_path(&dir)).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(asked(&dir, json!({"cmd": "status"})).await.unwrap()["state"], "unpaired");

    // The cloud's address, while unpaired.
    asked(&dir, json!({"cmd": "cloud_url", "url": ""})).await.unwrap();
    assert!(asked(&dir, json!({"cmd": "cloud_url", "url": "http://example.com"})).await.unwrap_err().contains("https"));

    // Pairing: a code, cancelled, started again and claimed.
    assert!(asked(&dir, json!({"cmd": "pair", "name": ""})).await.unwrap_err().starts_with("Type a name"));
    asked(&dir, json!({"cmd": "pair", "name": "Build server"})).await.unwrap();
    assert_eq!(asked(&dir, json!({"cmd": "status"})).await.unwrap()["state"], "waiting");
    asked(&dir, json!({"cmd": "cancel"})).await.unwrap();
    asked(&dir, json!({"cmd": "pair", "name": "Build server"})).await.unwrap();
    let exe = bench.script("greet", "echo hi");
    fake.set(|c| {
        c.mode = common::Mode::Paired;
        c.programs = Some(vec![program("greet", "greet", json!([]))]);
    });
    assert_eq!(agent.step().await, Step::Paired { calls: 0 });
    let status = asked(&dir, json!({"cmd": "status"})).await.unwrap();
    assert_eq!((status["state"].as_str(), status["programs"][0]["status"].as_str(), status["programs"][0]["path"].as_str()), (Some("paired"), Some("waiting"), Some(exe.as_str())));

    // Allow and revoke a program; a skill that is not there.
    asked(&dir, json!({"cmd": "allow", "name": "greet"})).await.unwrap();
    assert_eq!(agent.status().programs[0].status, agent_core::approvals::Standing::Approved);
    asked(&dir, json!({"cmd": "revoke", "name": "greet"})).await.unwrap();
    assert_eq!(agent.status().programs[0].status, agent_core::approvals::Standing::Waiting);
    assert!(asked(&dir, json!({"cmd": "allow", "name": "nope"})).await.is_err());
    assert!(asked(&dir, json!({"cmd": "allow_skill", "name": "nope"})).await.is_err());
    asked(&dir, json!({"cmd": "revoke_skill", "name": "nope"})).await.unwrap();
    assert!(asked(&dir, json!({"cmd": "skill_folder", "name": "nope"})).await.is_err());

    // Folders: added, changed, removed.
    let picked = bench.home().join("projects");
    std::fs::create_dir(&picked).unwrap();
    let added = asked(&dir, json!({"cmd": "add_folder", "path": picked.display().to_string(), "name": "", "mode": "read", "description": ""})).await.unwrap();
    let id = added["id"].as_str().unwrap().to_string();
    asked(&dir, json!({"cmd": "update_folder", "id": id, "name": "Code", "mode": "write", "description": "The repos"})).await.unwrap();
    assert_eq!(agent.status().folders[0].name, "Code");
    asked(&dir, json!({"cmd": "remove_folder", "id": id})).await.unwrap();
    assert!(asked(&dir, json!({"cmd": "remove_folder", "id": id})).await.is_err());

    // The switches, pausing.
    for name in ["any_program", "dev_access", "android_dev"] {
        asked(&dir, json!({"cmd": "switch", "name": name, "on": true})).await.unwrap();
    }
    let s = agent.state();
    assert!(s.any_program && s.dev_access && s.android_dev);
    assert!(asked(&dir, json!({"cmd": "switch", "name": "screen_control", "on": true})).await.unwrap_err().starts_with("There is no switch 'screen_control'"));
    assert_eq!(asked(&dir, json!({"cmd": "switch", "name": "dev_access"})).await.unwrap_err(), "Say 'on': true or false.");
    asked(&dir, json!({"cmd": "pause"})).await.unwrap();
    assert!(agent.state().paused);
    asked(&dir, json!({"cmd": "resume"})).await.unwrap();
    assert!(!agent.state().paused);

    // Anything else is refused; so is what is not a request.
    assert_eq!(asked(&dir, json!({"cmd": "format_disk"})).await.unwrap_err(), "Wysebee Agent cannot do 'format_disk'.");
    let raw = |line: &'static [u8]| {
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || {
            use std::io::{Read, Write};
            let mut s = std::os::unix::net::UnixStream::connect(control::socket_path(&dir)).unwrap();
            s.write_all(line).unwrap();
            s.shutdown(std::net::Shutdown::Write).unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        })
    };
    assert_eq!(raw(b"not json\n").await.unwrap(), "{\"ok\":false,\"error\":\"The request is not JSON.\"}\n");
    assert!(raw(b"\xff\xfe\n").await.unwrap().starts_with("{\"ok\":false,\"error\":\"The request could not be read: "));

    // Unpaired: everything goes.
    asked(&dir, json!({"cmd": "unpair"})).await.unwrap();
    assert_eq!(asked(&dir, json!({"cmd": "status"})).await.unwrap()["state"], "unpaired");
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_is_said_so() {
    let bench = Bench::new();
    let dir = bench.state_dir();
    std::fs::create_dir_all(&dir).unwrap();
    // A stand-in that answers badly, then not at all, then with a refusal of no words.
    let listener = std::os::unix::net::UnixListener::bind(control::socket_path(&dir)).unwrap();
    let server = std::thread::spawn(move || {
        use std::io::{BufRead, Write};
        for answer in [&b"garbage\n"[..], b"", b"{\"ok\": false}\n", b"{\"ok\": true}\n"] {
            let (mut s, _) = listener.accept().unwrap();
            let mut line = String::new();
            std::io::BufReader::new(&s).read_line(&mut line).unwrap();
            s.write_all(answer).unwrap();
        }
    });
    let req = json!({"cmd": "status"});
    assert_eq!(ask(&dir, &req, WAIT).unwrap_err(), "Wysebee Agent's answer could not be read.");
    assert_eq!(ask(&dir, &req, WAIT).unwrap_err(), "Wysebee Agent's answer could not be read.");
    assert_eq!(ask(&dir, &req, WAIT).unwrap_err(), "Wysebee Agent refused.");
    assert_eq!(ask(&dir, &req, WAIT).unwrap(), Value::Null);
    server.join().unwrap();

    // A folder that cannot be made: no socket.
    let blocked = bench.dir.path().join("file");
    std::fs::write(&blocked, "").unwrap();
    let fake = fake().await;
    assert!(control::serve(bench.agent(&fake), &blocked.join("state")).is_err());
    // handle() itself, with nothing asked.
    assert_eq!(handle(&bench.agent(&fake), &json!({})).await.unwrap_err(), "Wysebee Agent cannot do ''.");
}

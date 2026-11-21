//! `wysebee-agent run`: the agent, for as long as the process runs.
//!
//! One per state folder (the core's file lock). It logs to stderr, which systemd puts in the
//! journal (`wysebee-agent logs`); never the secret, a blank's value or a program's output.
//! SIGTERM (systemd stopping it) and Ctrl+C end it; whatever it started is stopped with it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use agent_core::secrets::FileSecrets;
use agent_core::{agent, cloud, control, programs, Agent, Options, Timing};

/// This machine's name, as the kernel has it; "Linux" when it cannot be read.
fn host_name() -> String {
    ["/proc/sys/kernel/hostname", "/etc/hostname"]
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .map(|n| n.trim().to_string())
        .find(|n| !n.is_empty())
        .unwrap_or_else(|| "Linux".into())
}

/// The `PATH` programs are looked for in: the process's own, the system's folders
/// (`programs::search_path`), the person's own (`~/.local/bin`, `~/bin`), Linuxbrew's, and
/// the Android SDK's `platform-tools` (adb) — a service started by systemd has a short `PATH`.
fn program_path(home: &Path) -> std::ffi::OsString {
    let base = programs::search_path(std::env::var_os("PATH").as_deref());
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&base).collect();
    let mut extra = vec![home.join(".local/bin"), home.join("bin"), PathBuf::from("/home/linuxbrew/.linuxbrew/bin"), home.join(".linuxbrew/bin")];
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(sdk) = std::env::var_os(var) {
            extra.push(PathBuf::from(sdk).join("platform-tools"));
        }
    }
    extra.push(home.join("Android/Sdk/platform-tools"));
    for dir in extra {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    std::env::join_paths(dirs).unwrap_or(base)
}

pub fn run() -> Result<(), String> {
    let home = crate::home();
    let state_dir = crate::state_dir();
    agent_core::secrets::private_folder(&state_dir).map_err(|e| format!("The state folder {} could not be made: {e}", state_dir.display()))?;
    // Two would take each other's calls.
    let Some(_lock) = agent::single_instance(&state_dir) else {
        return Err(format!("Wysebee Agent is already running for {}.", state_dir.display()));
    };
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).with_ansi(false).with_writer(std::io::stderr).init();

    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let agent = Agent::open(Options {
            state_dir: state_dir.clone(),
            path: Some(program_path(&home)),
            home,
            cloud_url: std::env::var(cloud::ENV).ok(),
            secrets: Arc::new(FileSecrets::in_folder(&state_dir)),
            mac_name: host_name(),
            version: agent_core::VERSION.into(),
            timing: Timing::default(),
            // A Linux server has no screen to show: screen control is not offered here.
            screen: None,
        });
        tracing::info!(version = agent_core::VERSION, state_dir = %state_dir.display(), cloud = %agent.cloud().0, "agent_starting");
        control::serve(agent.clone(), &state_dir).map_err(|e| format!("The control socket could not be opened in {} (its path must stay under about 100 characters): {e}", state_dir.display()))?;
        // No window to bring forward: the log says it, and `status` shows it.
        agent.set_attention_hook(Some(Arc::new(|| tracing::warn!("A program or skill waits for your Allow: see `wysebee-agent status`."))));
        let first = agent.status();
        if first.state == "unpaired" {
            tracing::info!("Not paired: run `wysebee-agent pair \"<agent name>\"`.");
        }

        let looping = agent.clone();
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).map_err(|e| e.to_string())?;
        tokio::select! {
            _ = looping.run() => {}
            _ = term.recv() => tracing::info!("agent_stopping"),
            _ = tokio::signal::ctrl_c() => tracing::info!("agent_stopping"),
        }
        let _ = std::fs::remove_file(control::socket_path(&state_dir));
        Ok::<(), String>(())
    })?;
    // Whatever runs (calls, background jobs) ends with the runtime.
    runtime.shutdown_timeout(std::time::Duration::from_secs(2));
    Ok(())
}

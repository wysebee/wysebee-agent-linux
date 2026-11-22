# CLAUDE.md

Guidance for working in this repository: **Wysebee Agent for Linux**, a service with a
command line (no window) that pairs with Wysebee Cloud and runs the "Run local program"
tools one agent's settings hold, and the scripts its skills declare, after the person
allowed each on this machine, and the four folder tools over the folders the person added
here — plus, behind switches, Run any program, Developer access and the Android toolkit.
It runs no model. It is the Linux build of **Wysebee Agent for Mac**, ported from its Unix
core, and does what **Wysebee Agent for Windows** does: the three must do the same things
the same way. README.md says how to build, run,
install and test it.

## What binds

- **`wysebee-cloud/docs/protocol/computer.md` is the contract** with Wysebee Cloud (the Mac and
  Windows agents speak the same link); `docs/protocol.md` here describes it as this agent
  speaks it, and must agree with it. A change to the contract is a change in the cloud and the
  other agents too; do not change it to suit this code — raise it with the owner.
- **Never widen what runs without the owner's say.** The person names the program and its
  fixed arguments in Wysebee and allows it here (`wysebee-agent allow`); the model only
  fills blanks. No shell, no model- or cloud-chosen program, no input, no working folder a
  call picks, no new op — except behind the switches the owner decided (below). For skills:
  **only scripts a skill's `SKILL.md` declares**.
- **A call carries a name and the blanks' values, and nothing else is read from it**
  (`agent.rs` `local_run`). What runs is the approved copy (`approvals.rs`). Approval is
  never taken from the cloud.
- **The command line can do what the Mac's window does, nothing more** (`control.rs`
  `handle`: one request, one `Agent` method). It asks before Allow and before turning a
  switch on (`--yes` for scripts). The socket is in the state folder (0700), itself 0600.
- **The secret is only in its own file** (`secrets.rs` `FileSecrets`: `secret`, mode 0600,
  in the 0700 state folder), never in the state file or a log line, and is sent only to the
  cloud the link was made with. Blank values, program output, typed text and the AI's
  arguments are never logged either.
- **A `folder_call` acts only in a folder the person added here**, with its permission. The
  path rules (`folders::resolve`), limits and readers are Wysebee Desktop's, ported.
- **Tests inject everything and touch nothing real**: every folder (`Options`), the secret
  store (`MemorySecrets`, or `FileSecrets` in a temporary folder), the cloud (a fake on
  127.0.0.1, `tests/common`). No test reaches the internet, systemd, `/etc` or the real
  `PATH`.
- **Never run the agent as root unless asked for by name** (`--user root`). The service is
  `wysebee-agent@<account>`, `User=%i`.
- **Keep the apps one app.** `agent-core` is the Mac app's with the Linux differences below
  and nothing else; a change of behaviour belongs in all three repositories.
- **Do not bump the version unless asked.** It lives in the workspace `Cargo.toml` only.

## What is Linux here (and why)

- **No window: a command line** (`crates/agent-cli`). `wysebee-agent run` is the agent;
  every other command asks it over `control.sock`. Where the Mac brings its window forward
  for a program waiting for Allow, this logs a line (journal) and `status` shows it.
- **The agent type is `wysebee-linux`** (`link::KIND`), "Wysebee Linux Agent": link field
  `mac_link`, the Windows type's ops and tool kinds without the screen's. A cloud that does
  not know it refuses pairing (`invalid_kind`).
- **The secret is a file**, not a keyring: a server (Azure, AWS, headless Ubuntu) has no
  Secret Service a systemd service can reach.
- **State** is `~/.local/state/wysebee-agent` (`approvals::default_dir`); `$XDG_STATE_HOME`
  is not read, so the service and the person's shell always agree.
- **Start at boot** is a systemd template unit (`service.rs`), a system unit run as the
  person, so it starts without anyone logged in (a user unit would need lingering). No
  systemd: `service install` and `install.sh` say how to start it from cron instead.
- **Programs** are any executable file on the `PATH` plus `programs::SYSTEM_FOLDERS`
  (a service's `PATH` is short) and the person's own (`~/.local/bin`, Linuxbrew,
  the Android SDK's `platform-tools`: `run.rs` `program_path`).
- **Run any program's safeguard** (`guard.rs`) is Linux's list: disk, boot and file-system
  tools, every shell and program that runs another (`sudo`, `systemd-run`, `nsenter`,
  `busybox`…), services/accounts/permissions/firewall/packages (`systemctl`, `apt`, `snap`,
  `chmod`…), power, `rm`/`kill`, inline interpreter code, and git/rsync/journalctl with the
  arguments that destroy data. `journalctl` reading logs is allowed.
- **Developer access**'s shell is `bash` (the default), `zsh` or `sh`, as login shells.
- **No screen control and no iPhone toolkit**: `Options::screen` is None (the core keeps the
  screen's code, shared with the Mac, unreachable: the command line has no switch for it),
  and `ios.rs` is not here.
- **One static binary** (musl) per architecture, x86_64 and aarch64 (AWS Graviton, Azure
  Cobalt): any distribution. `install.sh` is the one-line installer.
- **Open source** (MIT, LICENSE): the release workflow publishes this repository's GitHub
  releases, which the installer downloads (docs/distribute.md); every package ships
  `THIRD_PARTY_LICENSES.txt`. **This repository is public**: nothing internal goes in it
  (cloud internals, other repositories' paths, test endpoints, private names).

## The owner's decisions

The Mac's and Windows' (`docs/protocol.md`, "The rules it keeps") hold here: pairing with
Wysebee Cloud only, creating a new agent (or reconnecting one of this type with no device);
programs set up in Wysebee and allowed here; argument list, no input, 60 s, 20,000
characters; skills' declared scripts downloaded at once, run only once allowed; folders
the person's own; Run any program, Developer access and the Android toolkit each behind a
switch, off by default, confirmed, forgotten on unpairing, reported in the poll.

2026-11-18: **a Linux agent**, with a command line instead of a window, a systemd service,
and a one-line installer, for desktops, servers and Azure/AWS VMs.

## Testing

```bash
./scripts/check.sh            # clippy with no warning, every test, 100% of agent-core's lines
./scripts/check.sh --docker   # the same in a Linux container, from a Mac
```

**Every test passes, every time, and every line of `agent-core` is covered** — no
exclusions. Cover new code with a real test. The core's tests run on macOS too (Unix), but
only the Linux run counts.

## Known limits

- A program is killed by killing the process the app started; children it started are not
  hunted down (stopping the service does kill them all: systemd's control group).
- English only. x86_64 and aarch64 only.
- A socket path is limited to about 100 characters: keep `WYSEBEE_AGENT_STATE_DIR` short.
- The release workflow and the installer have not yet run against a real release.

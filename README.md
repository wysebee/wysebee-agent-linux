# Wysebee Agent for Linux

A small service with a command line that lets **one Wysebee agent run the programs you
allow on this Linux machine**, the scripts of the skills you allow, and **find, read and
(where you say so) write files in the folders you pick here** — and, behind switches you
turn on yourself, run any program, work as a developer, and use Android phones over adb.

It is the Linux build of Wysebee Agent for Mac and Wysebee Agent for Windows: the same
link, the same tools, the same rules. It has **no window**: everything is a command, so it
runs the same on a desktop Ubuntu, a headless server, or an **Azure or AWS virtual
machine**. It runs no model, opens no port (it only connects out to Wysebee Cloud over
HTTPS), and runs as an ordinary account, never root unless you ask for that by name.

## Install (one line)

```bash
curl -fsSL https://github.com/wysebee/wysebee-agent-linux/releases/latest/download/install.sh | sh
```

It downloads the build for this machine (x86_64 or aarch64, static: any distribution),
checks its SHA-256, installs `/usr/local/bin/wysebee-agent` and sets up a systemd service
that runs the agent as you, now and at every boot (it asks for sudo for those two steps).
Then pair it:

```bash
wysebee-agent pair "Build server"
```

It prints a code; type it in Wysebee, under **Devices**. Pairing creates a new agent (or
reconnects one of this kind that has no machine now, if you choose so in Wysebee).

### On a cloud VM (Azure, AWS) at first boot

As root, in the VM's custom data / user data (cloud-init), name the account it runs as:

```bash
#!/bin/sh
curl -fsSL https://github.com/wysebee/wysebee-agent-linux/releases/latest/download/install.sh | sh -s -- --user azureuser --pair "Build VM"
```

(`ubuntu` or `ec2-user` on AWS.) With no terminal it does not wait: the pairing code is in
the boot log and in `wysebee-agent status` (SSH in as that account), and stays good until it
runs out. Behind a proxy, set `HTTPS_PROXY` for the install: it is written to
`/etc/default/wysebee-agent` for the service too.

## Using it

```text
wysebee-agent status                     the pairing, the switches, what waits for your Allow
wysebee-agent allow <name>               allow a program or a skill (shows what it runs, asks first)
wysebee-agent revoke <name>              take an approval back
wysebee-agent folder add <path> [--write] [--name N] [--description D]
wysebee-agent folder list | update <id> | remove <id>
wysebee-agent enable any-program | developer | android     (each asks first)
wysebee-agent disable any-program | developer | android
wysebee-agent pause | resume             stop running anything / run again
wysebee-agent unpair                     everything allowed here is forgotten
wysebee-agent logs [-f]                  the service's journal
sudo wysebee-agent service install | uninstall [--user U]
```

1. In Wysebee, on the agent's **Tools** tab, add a "Run local program" tool: a program, its
   fixed arguments and the blanks the agent fills.
2. `wysebee-agent status` shows it **WAITING** with its whole command line;
   `wysebee-agent allow <name>` allows it. Change it in Wysebee and it waits again.
3. A skill that declares scripts waits the same way; its files are downloaded and checked
   as soon as it changes in Wysebee, and its scripts run only once allowed here.
4. Folders are only the ones you add here (once paired), read only unless `--write`.

What runs: the program found when you allowed it, as an argument list (never a shell), no
input, in your home folder, stopped after 60 s, its output cut at 20,000 characters. A
program is any executable file on the `PATH` (plus `/usr/local/bin`, `/snap/bin`,
`~/.local/bin`, Linuxbrew's and the Android SDK's `platform-tools`) or a full path.

### The wider switches (off by default, forgotten on unpairing)

- **any-program**: the AI names the program and its arguments. Disk, boot, account,
  service, package and shell tools (`fdisk`, `systemctl`, `apt`, `sudo`, `bash`, …),
  `rm`, inline interpreter code and data-destroying git/rsync/journalctl arguments are
  refused. A safeguard, not a sandbox.
- **developer**: `bash` (or `zsh`, `sh`) commands and file reads, writes and edits
  anywhere this account can, up to 220 s, or background jobs (at most 8).
- **android**: the Android toolkit through `adb` (`sudo apt install adb`, or Android
  Studio's platform-tools), for phones plugged in with USB debugging on.

There is no screen control on Linux (servers have no screen).

## Where things are

| | |
|---|---|
| The program | `/usr/local/bin/wysebee-agent` |
| The service | `/etc/systemd/system/wysebee-agent@.service`, instance `wysebee-agent@<account>` |
| Settings for the service | `/etc/default/wysebee-agent` (`HTTPS_PROXY`, `WYSEBEE_CLOUD_URL`) |
| State, approvals, skills | `~/.local/state/wysebee-agent/` (mode 0700) |
| The pairing key | `~/.local/state/wysebee-agent/secret` (mode 0600; never logged or sent elsewhere) |
| The command line's socket | `~/.local/state/wysebee-agent/control.sock` (mode 0600) |

`WYSEBEE_AGENT_STATE_DIR` moves the state folder (for development: keep its path short,
a socket's path is limited to about 100 characters).

Uninstall: `curl -fsSL …/install.sh | sh -s -- --uninstall` (the pairing stays until you
unpair; delete the state folder to forget it).

## Build and test

```bash
./scripts/check.sh            # clippy with no warning, every test, 100% of agent-core's lines
./scripts/check.sh --docker   # the same, in a Linux container (from a Mac)
./scripts/build_linux.sh      # dist/: the static packages, install.sh and SHA256SUMS
cargo run -p agent-cli -- run # the agent in the foreground (WYSEBEE_CLOUD_URL for a local cloud)
```

`docs/distribute.md` is the release walkthrough.

## Layout

```
crates/agent-core   everything testable: the link, approvals, programs, skills, folders,
                    the safeguard, developer access, adb, the secret file, the control socket
crates/agent-cli    the `wysebee-agent` binary: run (the service), the person's commands,
                    systemd (service.rs), the status as printed (show.rs)
scripts/            install.sh (the one-liner), build_linux.sh, check.sh
.github/workflows   ci.yml (check.sh on x86_64 and aarch64), release.yml (a tag → a release)
```

## Security

Found a way to run something the person did not allow, or to reach the pairing secret?
Please report it privately: see [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE). Release packages include `LICENSE` and `THIRD_PARTY_LICENSES.txt`, the
licenses of the Rust crates built into the binary.

"Wysebee" and the Wysebee logo are trademarks of Wysebee. The license covers the code, not
the name: a fork must not present itself as Wysebee or as an official Wysebee agent.

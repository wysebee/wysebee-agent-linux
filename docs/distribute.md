# Distributing Wysebee Agent for Linux

How a release reaches a machine: one static binary per architecture, a GitHub release, and
the one-line installer that fetches it.

There is no signing or notarization step on Linux. What protects the download is HTTPS to
GitHub and the SHA-256 the installer checks against the release's `SHA256SUMS` (both from
the same release; a later step could sign `SHA256SUMS` with minisign or GPG).

## State of things (2026-11-18)

| Step | State |
| --- | --- |
| The binary (`scripts/build_linux.sh`) | Static musl builds for x86_64 and aarch64; built in Docker on a Mac. |
| The tests on Linux | `scripts/check.sh --docker`, and CI (`.github/workflows/ci.yml`) on both architectures. |
| The release (`.github/workflows/release.yml`) | Written; **never run** (no tag pushed yet). |
| The installer (`scripts/install.sh`) | Written; **not yet run against a real release**, nor on Azure or AWS. |
| Pairing | Needs a Wysebee Cloud that knows the agent type `wysebee-linux`; one that does not answers `/start` with `invalid_kind`. |
| Hosting | GitHub releases of this repository, `wysebee/wysebee-agent-linux` (public), or the files hosted elsewhere (`WYSEBEE_AGENT_DOWNLOAD_URL`). |
| Updates | Run the one-liner again: it replaces the binary and restarts the service. No automatic update. |

## Once

In this repository's Settings → Security, turn on **Private vulnerability reporting**
(SECURITY.md sends reports there).

## Each release

1. **Wysebee Cloud first**: make sure it knows the agent type `wysebee-linux` and every op
   this release answers (docs/protocol.md).
2. **Set the version** in the workspace `Cargo.toml` (only there), commit.
3. **Check**: `./scripts/check.sh` (or `--docker` from a Mac).
4. **Tag and push**: `git tag v0.1.0 && git push origin v0.1.0`. The release workflow
   checks the tag is the version, builds both architectures on their own runners, checks
   each binary runs and is static, and publishes the release with
   `wysebee-agent-linux-x86_64.tar.gz`, `wysebee-agent-linux-aarch64.tar.gz`, `install.sh`
   and `SHA256SUMS`. The names never change, so
   `releases/latest/download/install.sh` is always the newest.

To build the same files by hand: `./scripts/build_linux.sh` (Docker on a Mac), then upload
`dist/*` to a release.

## Check it on a machine that has never seen it

On a fresh Ubuntu 24.04 VM (an Azure B1s or an AWS t4g.small covers both architectures):

1. `curl -fsSL https://github.com/wysebee/wysebee-agent-linux/releases/latest/download/install.sh | sh`
   — asks for sudo once, says the agent runs.
2. `systemctl status wysebee-agent@$USER` — active; `wysebee-agent status` — not paired.
3. `wysebee-agent pair "Release check"`, type the code in Wysebee: it says paired.
4. Add a "Run local program" tool (`uname -a`) in Wysebee; `wysebee-agent allow` it; ask
   the agent to run it.
5. `sudo reboot`; once back, `wysebee-agent status` shows it paired and connected, without
   logging in at the console.
6. Repeat with cloud-init user data as root (`--user ubuntu --pair "…"`): the code is in
   `/var/log/cloud-init-output.log`.
7. `… | sh -s -- --uninstall`: the service and the program are gone.

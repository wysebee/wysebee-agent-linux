#!/bin/sh
# Wysebee Agent for Linux: download, install (or update) and start it with the machine.
#
#   curl -fsSL https://github.com/wysebee/wysebee-agent-linux/releases/latest/download/install.sh | sh
#
# With options (after `sh -s --`):
#
#   ... | sh -s -- --pair "Build server"          also start pairing: prints the code to type in Wysebee
#   ... | sudo sh -s -- --user azureuser --pair "Build VM"
#                                                 as root (cloud-init, user data): name the account
#   ... | sh -s -- --version 0.1.0                a given release instead of the latest
#   ... | sh -s -- --uninstall                    stop it and remove the program (the pairing stays)
#
# What it does: picks the build for this machine (x86_64 or aarch64; static, so any
# distribution), checks its SHA-256 against the release's SHA256SUMS, puts it at
# /usr/local/bin/wysebee-agent, and runs `wysebee-agent service install`: a systemd service
# that runs the agent as that account (never as root unless --user root), now and at boot.
# It needs sudo for those two steps. The agent only connects out, over HTTPS, to Wysebee
# Cloud: no port is opened on this machine.
#
# Environment:
#   WYSEBEE_AGENT_DOWNLOAD_URL  where the release files are (default: GitHub's latest release)
#   WYSEBEE_CLOUD_URL           another Wysebee Cloud; written to /etc/default/wysebee-agent
#   HTTPS_PROXY                 used for the download, and written there too for the agent

set -eu

REPO_URL="https://github.com/wysebee/wysebee-agent-linux"
BIN=/usr/local/bin/wysebee-agent
ENV_FILE=/etc/default/wysebee-agent
UNIT=/etc/systemd/system/wysebee-agent@.service

say() { printf '%s\n' "$*"; }
fail() { printf 'wysebee-agent install: %s\n' "$*" >&2; exit 1; }

user=""
pair_name=""
version=""
uninstall=0
while [ $# -gt 0 ]; do
  case "$1" in
    --user) [ $# -ge 2 ] || fail "--user needs an account name"; user="$2"; shift 2 ;;
    --pair) [ $# -ge 2 ] || fail "--pair needs the agent's name"; pair_name="$2"; shift 2 ;;
    --version) [ $# -ge 2 ] || fail "--version needs a version, like 0.1.0"; version="${2#v}"; shift 2 ;;
    --uninstall) uninstall=1; shift ;;
    -h|--help) sed -n '2,28p' "$0" 2>/dev/null || true; exit 0 ;;
    *) fail "unknown option: $1" ;;
  esac
done

[ "$(uname -s)" = "Linux" ] || fail "this is for Linux; Wysebee Agent has its own downloads for Mac and Windows."

# Root, or sudo for the steps that need it.
if [ "$(id -u)" = "0" ]; then
  SUDO=""
elif command -v sudo >/dev/null 2>&1; then
  SUDO="sudo"
else
  fail "installing needs root: run this as root, or install sudo."
fi

# Whose agent: --user, else the account that ran sudo, else this one. Never root unnamed.
if [ -z "$user" ]; then
  if [ "$(id -u)" = "0" ]; then
    if [ -n "${SUDO_USER:-}" ] && [ "${SUDO_USER}" != "root" ]; then
      user="$SUDO_USER"
    else
      fail "run as root: say whose agent this is with --user <account> (it runs with that account's rights). --user root gives the AI root's rights: name it only if you mean it."
    fi
  else
    user="$(id -un)"
  fi
fi
id -u "$user" >/dev/null 2>&1 || fail "there is no account '$user' on this machine."

has_systemd() { [ -d /run/systemd/system ]; }

# A command as that account (pairing talks to its agent, through its own state folder).
as_user() {
  if [ "$(id -un)" = "$user" ]; then
    "$@"
  elif command -v runuser >/dev/null 2>&1 && [ "$(id -u)" = "0" ]; then
    runuser -u "$user" -- env HOME="$(getent passwd "$user" | cut -d: -f6)" "$@"
  else
    $SUDO -H -u "$user" "$@"
  fi
}

if [ "$uninstall" = 1 ]; then
  if has_systemd && [ -x "$BIN" ]; then
    $SUDO "$BIN" service uninstall --user "$user" || true
  fi
  $SUDO rm -f "$BIN"
  # The unit goes once no account's instance is left enabled.
  if has_systemd && ! ls /etc/systemd/system/multi-user.target.wants/wysebee-agent@*.service >/dev/null 2>&1; then
    $SUDO rm -f "$UNIT"
    $SUDO systemctl daemon-reload
  fi
  say "Wysebee Agent is removed. Its pairing and approvals stay in ~$user/.local/state/wysebee-agent:"
  say "unpair it in Wysebee (Devices), and delete that folder if it is not coming back."
  exit 0
fi

case "$(uname -m)" in
  x86_64|amd64) arch=x86_64 ;;
  aarch64|arm64) arch=aarch64 ;;
  *) fail "there is no build for $(uname -m) yet: x86_64 and aarch64 only." ;;
esac

if [ -n "${WYSEBEE_AGENT_DOWNLOAD_URL:-}" ]; then
  base="${WYSEBEE_AGENT_DOWNLOAD_URL%/}"
elif [ -n "$version" ]; then
  base="$REPO_URL/releases/download/v$version"
else
  base="$REPO_URL/releases/latest/download"
fi
package="wysebee-agent-linux-$arch.tar.gz"

# HTTPS only, unless WYSEBEE_AGENT_DOWNLOAD_URL names another address on purpose (a mirror
# inside a private network, a test).
proto="=https"
case "$base" in https://*) ;; *) proto="=https,http" ;; esac
fetch() {
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --proto "$proto" -o "$2" "$1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    fail "curl or wget is needed to download."
  fi
}
command -v sha256sum >/dev/null 2>&1 || fail "sha256sum is needed to check the download."

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM
say "Downloading $package..."
fetch "$base/$package" "$tmp/$package" || fail "could not download $base/$package"
fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || fail "could not download $base/SHA256SUMS"
( cd "$tmp" && grep " $package\$" SHA256SUMS | sha256sum -c - >/dev/null 2>&1 ) || fail "the download does not match its SHA-256: not installed."
tar -xzf "$tmp/$package" -C "$tmp"
[ -f "$tmp/wysebee-agent" ] || fail "the package holds no wysebee-agent."

$SUDO install -m 0755 "$tmp/wysebee-agent" "$BIN"
say "Installed $("$BIN" --version) at $BIN."

# Settings every instance of the service reads.
set_env() {
  [ -n "$2" ] || return 0
  if [ -f "$ENV_FILE" ]; then
    $SUDO sed -i "/^$1=/d" "$ENV_FILE"
  fi
  printf '%s=%s\n' "$1" "$2" | $SUDO tee -a "$ENV_FILE" >/dev/null
}
set_env WYSEBEE_CLOUD_URL "${WYSEBEE_CLOUD_URL:-}"
set_env HTTPS_PROXY "${HTTPS_PROXY:-${https_proxy:-}}"
set_env NO_PROXY "${NO_PROXY:-${no_proxy:-}}"

if has_systemd; then
  $SUDO "$BIN" service install --user "$user"
else
  say ""
  say "This machine does not run systemd, so nothing starts the agent at boot."
  say "Start it as $user with:  nohup wysebee-agent run >> ~/.local/state/wysebee-agent/agent.log 2>&1 &"
  say "and add that line to $user's crontab as @reboot (crontab -e) to start it with the machine."
  [ -z "$pair_name" ] || say "Then pair it: wysebee-agent pair \"$pair_name\""
  exit 0
fi

if [ -n "$pair_name" ]; then
  # The service opens its socket within a moment of starting.
  tries=0
  until as_user "$BIN" status >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -lt 30 ] || fail "the agent did not start: see sudo journalctl -u wysebee-agent@$user"
    sleep 1
  done
  say ""
  if [ -t 1 ]; then
    as_user "$BIN" pair "$pair_name"
  else
    # No one at a terminal (cloud-init): show the code and leave it waiting; it stays in
    # `wysebee-agent status` and the journal until it is typed or runs out.
    as_user "$BIN" pair "$pair_name" --no-wait
  fi
fi

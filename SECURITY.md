# Security

Wysebee Agent for Linux runs programs on the machine it is installed on, so we take reports
about it seriously.

## Reporting a vulnerability

**Please do not open a public issue.** Report it privately through GitHub: on this
repository's **Security** tab, choose **Report a vulnerability**. Include what you found,
how to reproduce it, the version (`wysebee-agent --version`) and the distribution.

We will acknowledge the report, keep you posted while we fix it, and credit you in the
release notes unless you would rather we did not.

## What counts

The agent's promises (README.md) are what we defend. Most worth reporting:

- anything run that the person did not allow on this machine (`wysebee-agent allow`), or a
  call that changes what an allowed program or skill script runs;
- a folder tool acting outside a folder the person added, or writing to a read-only one;
- the pairing secret leaving its file (`~/.local/state/wysebee-agent/secret`) other than to
  the cloud the link was made with, or reaching a log;
- another local account reaching the control socket or the state folder;
- the installer or a release package being tampered with undetected.

**Not a vulnerability by itself**: a way past the **Run any program** safeguard (`guard.rs`).
It is a list of refused programs, documented as "a safeguard, not a sandbox", and is off
by default; turning it on lets the AI run programs as this account. We still welcome such
reports as ordinary issues, and fix them.

## Supported versions

Only the latest release. Update by running the one-line installer again.

//! Wysebee Agent for Linux, bar the command line.
//!
//! A service with a command line (no window) that pairs with Wysebee Cloud and runs the "Run
//! local program" tools one agent's settings hold, and the scripts its skills declare, after the
//! person allowed each on this machine — and the folder tools, over the folders the person
//! picked here.
//! It runs no model. `docs/protocol.md` is the contract.
//!
//! - [`programs`]: reading a program, filling its blanks, finding and running it.
//! - [`skills`]: reading a skill's `SKILL.md` and file list, and putting a skill in place.
//! - [`folders`]: the person's local folders, and the four folder tools over them.
//! - [`approvals`]: the state file and what the person allowed.
//! - [`secrets`]: the link's secret, in a file of its own.
//! - [`cloud`]: which Wysebee Cloud, and the rule its address must pass.
//! - [`link`]: the protocol's HTTP calls.
//! - [`agent`]: the [`Agent`] the service drives: pairing, the poll loop, the calls.
//! - [`control`]: the socket the command line asks the running agent through.
//! - [`guard`], [`dev`], [`android`]: Run any program's safeguard, Developer access, adb.
//!
//! Nothing here knows of the command line. **Every folder, the secret store, the cloud address and
//! the `PATH` searched are handed in** ([`agent::Options`]), so the tests use temporary
//! folders, a secret store in memory and a fake cloud on 127.0.0.1.

pub mod agent;
pub mod android;
pub mod approvals;
pub mod cloud;
pub mod control;
pub mod dev;
pub mod folders;
pub mod guard;
pub mod link;
pub mod programs;
pub mod screen;
pub mod secrets;
pub mod skills;

pub use agent::{Agent, Options, Status, Step, Timing};

/// The app's version: the workspace `Cargo.toml`'s.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

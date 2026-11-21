//! Where the link's secret is kept: a file of its own, readable by this account only, never
//! the state file.
//!
//! The secret is 32 random bytes (base64url) this app makes when pairing starts; Wysebee
//! Cloud keeps only its SHA-256. It is sent as `Authorization: Bearer` to the cloud the
//! link was made with and to nothing else, never shown and never logged.
//!
//! **Why a file, on Linux**: the Mac keeps it in the Keychain and Windows in the Credential
//! Manager, but a Linux server (an Azure or AWS VM, a headless Ubuntu) has no keyring a
//! service can reach: the Secret Service needs a desktop session's D-Bus and an unlocked
//! keyring. So it is [`FileSecrets`]: `secret` in the state folder, mode 0600 in a folder of
//! mode 0700, written beside and renamed over, as ssh keeps its keys.
//!
//! [`SecretStore`] is the seam: the app installs [`FileSecrets`]; the agent's tests use
//! [`MemorySecrets`], and [`FileSecrets`]'s own tests a temporary folder.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The one secret this app holds.
pub trait SecretStore: Send + Sync {
    /// The secret; None when there is none. An error is the store failing (a file that
    /// cannot be read), which is not the same as "none".
    fn get(&self) -> Result<Option<String>, String>;
    fn set(&self, secret: &str) -> Result<(), String>;
    /// Removes it; removing what is not there is fine.
    fn delete(&self) -> Result<(), String>;
}

/// A secret kept in memory: the tests' store.
#[derive(Default)]
pub struct MemorySecrets {
    secret: Mutex<Option<String>>,
    /// When set, every call fails with it (a store that cannot be used).
    failing: Mutex<Option<String>>,
}

impl MemorySecrets {
    pub fn new() -> MemorySecrets {
        MemorySecrets::default()
    }

    /// Make every later call fail with `error`, or work again with None.
    pub fn fail_with(&self, error: Option<&str>) {
        *self.failing.lock().unwrap() = error.map(str::to_string);
    }

    /// What is stored, whatever `fail_with` says.
    pub fn peek(&self) -> Option<String> {
        self.secret.lock().unwrap().clone()
    }

    fn usable(&self) -> Result<(), String> {
        match self.failing.lock().unwrap().clone() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

impl SecretStore for MemorySecrets {
    fn get(&self) -> Result<Option<String>, String> {
        self.usable()?;
        Ok(self.secret.lock().unwrap().clone())
    }

    fn set(&self, secret: &str) -> Result<(), String> {
        self.usable()?;
        *self.secret.lock().unwrap() = Some(secret.to_string());
        Ok(())
    }

    fn delete(&self) -> Result<(), String> {
        self.usable()?;
        *self.secret.lock().unwrap() = None;
        Ok(())
    }
}

/// The secret's file name in the state folder.
pub const SECRET_FILE: &str = "secret";

/// The secret in a file of its own: mode 0600, in a folder made mode 0700.
pub struct FileSecrets {
    pub path: PathBuf,
}

impl FileSecrets {
    /// The secret of the state folder `state_dir`: one per folder, so a copy run with another
    /// state folder (`WYSEBEE_AGENT_STATE_DIR`) pairs without touching the installed one's.
    pub fn in_folder(state_dir: &Path) -> FileSecrets {
        FileSecrets { path: state_dir.join(SECRET_FILE) }
    }
}

/// Make `dir` (and what is missing above it), and keep it this account's only.
pub fn private_folder(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    std::fs::set_permissions(dir, std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
    Ok(())
}

impl SecretStore for FileSecrets {
    fn get(&self) -> Result<Option<String>, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) if text.trim().is_empty() => Ok(None),
            Ok(text) => Ok(Some(text.trim().to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("The pairing key could not be read ({}): {e}", self.path.display())),
        }
    }

    fn set(&self, secret: &str) -> Result<(), String> {
        let failed = |e: std::io::Error| format!("The pairing key could not be saved ({}): {e}", self.path.display());
        let dir = self.path.parent().unwrap_or(Path::new("."));
        private_folder(dir).map_err(failed)?;
        // Written beside, then renamed over: never half a key, never readable by others.
        let beside = self.path.with_extension("new");
        let _ = std::fs::remove_file(&beside);
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&beside).map_err(failed)?;
        file.write_all(secret.as_bytes()).and_then(|_| file.sync_all()).map_err(failed)?;
        std::fs::rename(&beside, &self.path).map_err(failed)
    }

    fn delete(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("The pairing key could not be removed ({}): {e}", self.path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_memory_store_keeps_one_secret_and_can_be_made_to_fail() {
        let s = MemorySecrets::new();
        assert_eq!(s.get().unwrap(), None);
        s.delete().unwrap();
        s.set("one").unwrap();
        s.set("two").unwrap();
        assert_eq!(s.get().unwrap().as_deref(), Some("two"));
        s.fail_with(Some("locked"));
        assert_eq!(s.get().unwrap_err(), "locked");
        assert_eq!(s.set("x").unwrap_err(), "locked");
        assert_eq!(s.delete().unwrap_err(), "locked");
        assert_eq!(s.peek().as_deref(), Some("two"));
        s.fail_with(None);
        s.delete().unwrap();
        assert_eq!(s.peek(), None);
    }

    #[test]
    fn the_file_store_keeps_one_secret_readable_by_this_account_only() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("state");
        let s = FileSecrets::in_folder(&dir);
        assert_eq!(s.get().unwrap(), None, "no folder yet");
        s.delete().unwrap();
        s.set("one").unwrap();
        s.set("two").unwrap();
        assert_eq!(s.get().unwrap().as_deref(), Some("two"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&s.path).unwrap().permissions().mode() & 0o777, 0o600);
            assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
        }
        assert!(!dir.join("secret.new").exists());
        // An empty file is no secret.
        std::fs::write(&s.path, "\n").unwrap();
        assert_eq!(s.get().unwrap(), None);
        s.delete().unwrap();
        assert!(!s.path.exists());
    }

    #[test]
    fn a_file_store_that_cannot_be_used_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        // A folder where the file should be: it cannot be read, replaced or removed.
        let s = FileSecrets { path: tmp.path().join("secret") };
        std::fs::create_dir(&s.path).unwrap();
        std::fs::write(s.path.join("x"), "").unwrap();
        assert!(s.get().unwrap_err().starts_with("The pairing key could not be read"));
        assert!(s.set("x").unwrap_err().starts_with("The pairing key could not be saved"));
        assert!(s.delete().unwrap_err().starts_with("The pairing key could not be removed"));
        // A file where its folder should be.
        let blocked = FileSecrets { path: tmp.path().join("file").join("secret") };
        std::fs::write(tmp.path().join("file"), "").unwrap();
        assert!(blocked.set("x").unwrap_err().starts_with("The pairing key could not be saved"));
    }
}

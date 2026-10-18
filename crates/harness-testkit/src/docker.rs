//! A stand-in for the `docker` client for tests that need no daemon: see `fixtures/fake-docker.pl`.

use std::path::{Path, PathBuf};

use tempfile::TempDir;

/// A fake `docker` executable and its state directory, removed when this is dropped.
pub struct FakeDocker {
    dir: TempDir,
    executable: PathBuf,
}

impl FakeDocker {
    /// Writes the executable and an empty state next to it.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("docker");
        std::fs::write(&executable, include_str!("../fixtures/fake-docker.pl")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::create_dir_all(dir.path().join("state/flags")).unwrap();
        Self { dir, executable }
    }

    /// The path of the fake `docker` executable.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn state_dir(&self) -> PathBuf {
        self.dir.path().join("state")
    }

    /// Makes `image` exist locally.
    pub fn add_image(&self, image: &str) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.state_dir().join("images"))
            .unwrap();
        writeln!(file, "{image}").unwrap();
    }

    /// Turns a failure switch on: `daemon_down`, `slow_create`, `mount_denied`, `start_fails`,
    /// `oom`, `client_stops`, `rm_fails`.
    pub fn set_flag(&self, flag: &str) {
        std::fs::write(self.state_dir().join("flags").join(flag), "").unwrap();
    }

    pub fn clear_flag(&self, flag: &str) {
        let _ = std::fs::remove_file(self.state_dir().join("flags").join(flag));
    }

    /// Every call made so far, each as its arguments.
    pub fn calls(&self) -> Vec<Vec<String>> {
        std::fs::read_to_string(self.state_dir().join("log"))
            .unwrap_or_default()
            .lines()
            .map(|line| line.split('\u{1f}').map(str::to_string).collect())
            .collect()
    }

    /// The calls whose first argument is `subcommand`.
    pub fn calls_of(&self, subcommand: &str) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|call| call.first().map(String::as_str) == Some(subcommand))
            .collect()
    }

    /// The containers that still exist.
    pub fn containers(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.state_dir().join("c"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Adds a container that a sidecar might have left, with or without our label, owned by the
    /// process `owner` (a pid; a dead one makes it a leftover of a crash).
    pub fn add_leftover(&self, name: &str, labelled: bool, owner: u32) {
        let dir = self.state_dir().join("c").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("state"), "running 0 0").unwrap();
        std::fs::write(
            dir.join("labels"),
            if labelled {
                format!("banana-harness.command={name}\nbanana-harness.pid={owner}")
            } else {
                "other=1".to_string()
            },
        )
        .unwrap();
    }
}

impl Default for FakeDocker {
    fn default() -> Self {
        Self::new()
    }
}

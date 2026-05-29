//! Setup-state detection for the wizard.
//!
//! The wizard shows two prerequisites — the daemon running and the GNOME
//! extension enabled — and reverts to a pending item whenever one regresses.
//! Detection shells out to `systemctl` and `gnome-extensions`; the work is kept
//! behind a [`CommandRunner`] trait so it can be driven with canned output in
//! unit tests, with no real services or GNOME session needed.

use std::process::Command;

/// The UUID of the bundled GNOME Shell extension (see `gnome-extension/`).
const EXTENSION_UUID: &str = "mackey@mackey.app";

/// Runs a command and returns its trimmed stdout. A command that fails to spawn
/// yields an empty string, which reads as "prerequisite not satisfied".
pub trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> String;
}

/// The real runner: spawns the process and captures stdout.
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> String {
        Command::new(program)
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    }
}

/// Which prerequisites are currently satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupState {
    pub service_active: bool,
    pub extension_enabled: bool,
}

impl SetupState {
    /// Both prerequisites met — the wizard hands off to the status view.
    pub fn is_complete(self) -> bool {
        self.service_active && self.extension_enabled
    }
}

/// Detect the current setup state. `systemctl is-active` prints `active` only
/// when the unit is running; `gnome-extensions list --enabled` prints one UUID
/// per line, and we look for ours.
pub fn detect(runner: &dyn CommandRunner) -> SetupState {
    let service = runner.run("systemctl", &["is-active", "mackey.service"]);
    let extensions = runner.run("gnome-extensions", &["list", "--enabled"]);
    SetupState {
        service_active: service.trim() == "active",
        extension_enabled: extensions.lines().any(|line| line.trim() == EXTENSION_UUID),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A runner that returns canned stdout per program name.
    struct MockRunner {
        systemctl: String,
        gnome_extensions: String,
    }

    impl CommandRunner for MockRunner {
        fn run(&self, program: &str, _args: &[&str]) -> String {
            match program {
                "systemctl" => self.systemctl.clone(),
                "gnome-extensions" => self.gnome_extensions.clone(),
                other => panic!("unexpected command: {other}"),
            }
        }
    }

    fn state(systemctl: &str, gnome_extensions: &str) -> SetupState {
        detect(&MockRunner {
            systemctl: systemctl.to_string(),
            gnome_extensions: gnome_extensions.to_string(),
        })
    }

    #[test]
    fn both_satisfied_is_complete() {
        let s = state("active", "other@x.app\nmackey@mackey.app\n");
        assert_eq!(
            s,
            SetupState {
                service_active: true,
                extension_enabled: true
            }
        );
        assert!(s.is_complete());
    }

    #[test]
    fn inactive_service_is_pending() {
        let s = state("inactive", "mackey@mackey.app");
        assert!(!s.service_active);
        assert!(s.extension_enabled);
        assert!(!s.is_complete());
    }

    #[test]
    fn extension_absent_is_pending() {
        let s = state("active", "other@x.app\n");
        assert!(s.service_active);
        assert!(!s.extension_enabled);
        assert!(!s.is_complete());
    }

    #[test]
    fn empty_output_is_all_pending() {
        let s = state("", "");
        assert!(!s.service_active);
        assert!(!s.extension_enabled);
        assert!(!s.is_complete());
    }

    #[test]
    fn activating_is_not_yet_active() {
        // `systemctl is-active` reports `activating` during startup.
        let s = state("activating", "mackey@mackey.app");
        assert!(!s.service_active);
    }

    #[test]
    fn substring_uuid_does_not_match() {
        // A line that merely contains the UUID as a substring must not count.
        let s = state("active", "not-mackey@mackey.app.evil");
        assert!(!s.extension_enabled);
    }
}

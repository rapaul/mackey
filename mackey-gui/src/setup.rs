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

/// The polkit-authorized helper that pauses/resumes the daemon (see
/// `packaging/mackey-service-control`).
const SERVICE_CONTROL: &str = "/usr/libexec/mackey-service-control";

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

/// The daemon's runtime state, shown in the status view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Active,
    Paused,
    Failed,
}

/// Map `systemctl is-active` output to a [`ServiceState`]. Anything that isn't
/// `active` or `failed` (e.g. `inactive`, `deactivating`) reads as paused.
pub fn service_state_from(is_active_output: &str) -> ServiceState {
    match is_active_output.trim() {
        "active" => ServiceState::Active,
        "failed" => ServiceState::Failed,
        _ => ServiceState::Paused,
    }
}

/// Detect the daemon's current runtime state.
pub fn detect_service_state(runner: &dyn CommandRunner) -> ServiceState {
    service_state_from(&runner.run("systemctl", &["is-active", "mackey.service"]))
}

/// Pause (`running = false`) or resume (`running = true`) the daemon through the
/// polkit-authorized helper, via pkexec — no prompt for the active seat user.
pub fn set_service_running(runner: &dyn CommandRunner, running: bool) {
    let verb = if running { "start" } else { "stop" };
    runner.run("pkexec", &[SERVICE_CONTROL, verb]);
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

    #[test]
    fn service_state_maps_systemctl_output() {
        assert_eq!(service_state_from("active"), ServiceState::Active);
        assert_eq!(service_state_from("failed"), ServiceState::Failed);
        assert_eq!(service_state_from("inactive"), ServiceState::Paused);
        assert_eq!(service_state_from("deactivating"), ServiceState::Paused);
        assert_eq!(service_state_from(""), ServiceState::Paused);
    }

    /// A runner that records every command it was asked to run.
    #[derive(Default)]
    struct RecordingRunner {
        calls: std::cell::RefCell<Vec<Vec<String>>>,
    }

    impl CommandRunner for RecordingRunner {
        fn run(&self, program: &str, args: &[&str]) -> String {
            let mut call = vec![program.to_string()];
            call.extend(args.iter().map(|a| a.to_string()));
            self.calls.borrow_mut().push(call);
            String::new()
        }
    }

    #[test]
    fn pause_stops_and_resume_starts_via_pkexec_helper() {
        let runner = RecordingRunner::default();
        set_service_running(&runner, false); // pause
        set_service_running(&runner, true); // resume
        let calls = runner.calls.borrow();
        assert_eq!(
            calls[0],
            ["pkexec", SERVICE_CONTROL, "stop"],
            "pause runs the helper with stop"
        );
        assert_eq!(
            calls[1],
            ["pkexec", SERVICE_CONTROL, "start"],
            "resume runs the helper with start"
        );
    }
}

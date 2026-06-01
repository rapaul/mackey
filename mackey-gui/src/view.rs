//! The wizard's view model: a pure mapping from [`SetupState`] to what the
//! window should show. Kept free of GTK so the screen/step logic is unit-tested
//! without a display.

use crate::setup::SetupState;

/// Which screen the window displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    /// One or more prerequisites are unmet — show the setup steps.
    Wizard,
    /// Everything is satisfied — show the (placeholder) status screen.
    Ready,
}

/// One prerequisite the wizard walks the user through. The wizard only ever
/// *shows* `commands`; it never runs them.
pub struct WizardStep {
    pub title: &'static str,
    pub explanation: &'static str,
    pub commands: &'static [&'static str],
    pub done: bool,
}

/// The screen to show for a given state.
pub fn screen_for(state: SetupState) -> Screen {
    if state.is_complete() {
        Screen::Ready
    } else {
        Screen::Wizard
    }
}

/// The setup steps, each flagged done/pending from the current state. The
/// package starts the service and installs the extension system-wide, so the
/// only action left for the user is enabling the extension; the service step
/// stays as a recovery hint for the rare case where the daemon isn't running.
pub fn wizard_steps(state: SetupState) -> Vec<WizardStep> {
    vec![
        WizardStep {
            title: "Start the mackey service",
            explanation: "Runs the background daemon that remaps your keys. mackey \
                          starts this automatically when it's installed; run this \
                          only if the service isn't running.",
            commands: &["sudo systemctl enable --now mackey.service"],
            done: state.service_active,
        },
        WizardStep {
            title: "Enable the mackey extension",
            explanation: "Turns on the extension that lets mackey see which \
                          application is focused so it can apply per-app shortcuts. \
                          It's already installed system-wide; if this says the \
                          extension doesn't exist, log out and back in once (GNOME \
                          on Wayland only picks it up after a re-login), then retry.",
            commands: &["gnome-extensions enable mackey@mackey.app"],
            done: state.extension_enabled,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(service: bool, enabled: bool) -> SetupState {
        SetupState {
            service_active: service,
            extension_enabled: enabled,
        }
    }

    #[test]
    fn incomplete_state_shows_wizard() {
        assert_eq!(screen_for(st(false, false)), Screen::Wizard);
        assert_eq!(screen_for(st(false, true)), Screen::Wizard);
        // Service up but extension not yet enabled is incomplete.
        assert_eq!(screen_for(st(true, false)), Screen::Wizard);
    }

    #[test]
    fn complete_state_shows_ready() {
        assert_eq!(screen_for(st(true, true)), Screen::Ready);
    }

    #[test]
    fn step_done_flags_track_state() {
        let steps = wizard_steps(st(true, false));
        assert!(steps[0].done, "service step done when service active");
        assert!(!steps[1].done, "enable step pending when not enabled");

        let steps = wizard_steps(st(false, true));
        assert!(!steps[0].done, "service step pending when service down");
        assert!(steps[1].done, "enable step done when enabled");
    }

    #[test]
    fn wizard_has_two_single_command_steps() {
        let steps = wizard_steps(st(false, false));
        assert_eq!(steps.len(), 2);
        assert!(steps.iter().all(|s| s.commands.len() == 1));
        assert_eq!(
            steps[1].commands[0],
            "gnome-extensions enable mackey@mackey.app"
        );
    }
}

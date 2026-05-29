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

/// The two setup steps, each flagged done/pending from the current state.
pub fn wizard_steps(state: SetupState) -> Vec<WizardStep> {
    vec![
        WizardStep {
            title: "Start the mackey service",
            explanation: "Runs the background daemon that remaps your keys. It is \
                          installed but not started automatically.",
            commands: &["sudo systemctl enable --now mackey.service"],
            done: state.service_active,
        },
        WizardStep {
            title: "Enable the GNOME extension",
            explanation: "Lets mackey see which application is focused so it can \
                          apply per-app shortcuts.",
            commands: &[
                "gnome-extensions install /usr/share/mackey/gnome-extension/mackey@mackey.app.shell-extension.zip",
                "gnome-extensions enable mackey@mackey.app",
            ],
            done: state.extension_enabled,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(service: bool, extension: bool) -> SetupState {
        SetupState {
            service_active: service,
            extension_enabled: extension,
        }
    }

    #[test]
    fn incomplete_state_shows_wizard() {
        assert_eq!(screen_for(st(false, false)), Screen::Wizard);
        assert_eq!(screen_for(st(true, false)), Screen::Wizard);
        assert_eq!(screen_for(st(false, true)), Screen::Wizard);
    }

    #[test]
    fn complete_state_shows_ready() {
        assert_eq!(screen_for(st(true, true)), Screen::Ready);
    }

    #[test]
    fn step_done_flags_track_state() {
        let steps = wizard_steps(st(true, false));
        assert!(steps[0].done, "service step done when service active");
        assert!(!steps[1].done, "extension step pending when extension off");

        let steps = wizard_steps(st(false, true));
        assert!(!steps[0].done);
        assert!(steps[1].done);
    }

    #[test]
    fn extension_step_shows_both_commands() {
        let steps = wizard_steps(st(false, false));
        assert_eq!(steps[1].commands.len(), 2);
    }
}

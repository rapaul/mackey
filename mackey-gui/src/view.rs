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

/// The three setup steps, each flagged done/pending from the current state.
/// Install and enable are separate steps because GNOME on Wayland only loads a
/// newly installed extension after a re-login, so it cannot be enabled in the
/// same session it was installed.
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
            title: "Install the GNOME extension",
            explanation: "Adds the extension that lets mackey see which application \
                          is focused so it can apply per-app shortcuts.",
            commands: &[
                "gnome-extensions install /usr/share/mackey/gnome-extension/mackey@mackey.app.shell-extension.zip",
            ],
            done: state.extension_installed,
        },
        WizardStep {
            title: "Log out, log back in, then enable the extension",
            explanation: "GNOME on Wayland only loads a newly installed extension \
                          after you log out and back in. Once you're back, run this \
                          to turn it on.",
            commands: &["gnome-extensions enable mackey@mackey.app"],
            done: state.extension_enabled,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(service: bool, installed: bool, enabled: bool) -> SetupState {
        SetupState {
            service_active: service,
            extension_installed: installed,
            extension_enabled: enabled,
        }
    }

    #[test]
    fn incomplete_state_shows_wizard() {
        assert_eq!(screen_for(st(false, false, false)), Screen::Wizard);
        assert_eq!(screen_for(st(true, false, false)), Screen::Wizard);
        // Installed but not yet enabled (post-install, pre-relogin) is incomplete.
        assert_eq!(screen_for(st(true, true, false)), Screen::Wizard);
    }

    #[test]
    fn complete_state_shows_ready() {
        assert_eq!(screen_for(st(true, true, true)), Screen::Ready);
    }

    #[test]
    fn step_done_flags_track_state() {
        let steps = wizard_steps(st(true, false, false));
        assert!(steps[0].done, "service step done when service active");
        assert!(!steps[1].done, "install step pending when not installed");
        assert!(!steps[2].done, "enable step pending when not enabled");

        let steps = wizard_steps(st(false, true, false));
        assert!(!steps[0].done);
        assert!(steps[1].done, "install step done when installed");
        assert!(!steps[2].done, "enable step still pending until enabled");

        let steps = wizard_steps(st(false, true, true));
        assert!(steps[2].done, "enable step done when enabled");
    }

    #[test]
    fn wizard_has_three_single_command_steps() {
        let steps = wizard_steps(st(false, false, false));
        assert_eq!(steps.len(), 3);
        assert!(steps.iter().all(|s| s.commands.len() == 1));
        assert!(steps[1].commands[0].starts_with("gnome-extensions install"));
        assert_eq!(
            steps[2].commands[0],
            "gnome-extensions enable mackey@mackey.app"
        );
    }
}

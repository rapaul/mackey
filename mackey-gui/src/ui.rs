//! The GTK4 wizard + status window.
//!
//! [`WizardUi`] owns a [`gtk::Stack`] with two pages — the setup wizard and the
//! status view — and the policy of which to show lives in `main`'s poll. The
//! wizard only ever *displays* commands (with a Copy button); the status view's
//! Pause/Resume toggle runs the polkit-authorized helper via pkexec (no prompt
//! for the active seat user). It is built independently of the
//! [`adw::Application`] so its rendering can be exercised in a unit test.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{glib, Align, Orientation};

use crate::setup::{
    detect_service_state, set_service_running, ServiceState, SetupState, SystemRunner,
};
use crate::view::{wizard_steps, WizardStep};

const WIZARD_PAGE: &str = "wizard";
const STATUS_PAGE: &str = "status";

#[derive(Clone)]
pub struct WizardUi {
    stack: gtk::Stack,
    /// Container the wizard steps are rebuilt into on each update.
    steps: gtk::Box,
    /// Status view widgets and the daemon state they last rendered.
    status_label: gtk::Label,
    toggle: gtk::Button,
    service: Rc<Cell<ServiceState>>,
    /// Set once the status view is shown; `main` then stops reverting to the
    /// wizard in-session (regressions are caught on the next launch).
    settled: Rc<Cell<bool>>,
}

impl WizardUi {
    pub fn new() -> Self {
        let stack = gtk::Stack::new();

        // Wizard page: heading + steps container, in a scroll view.
        let steps = gtk::Box::new(Orientation::Vertical, 18);
        steps.set_margin_top(24);
        steps.set_margin_bottom(24);
        steps.set_margin_start(24);
        steps.set_margin_end(24);

        let heading = gtk::Label::new(Some("Finish setting up mackey"));
        heading.add_css_class("title-2");
        heading.set_halign(Align::Start);
        steps.prepend(&heading);

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&steps)
            .vexpand(true)
            .build();
        stack.add_named(&scroller, Some(WIZARD_PAGE));

        // Status page: daemon state + a single Pause/Resume toggle.
        let status_box = gtk::Box::new(Orientation::Vertical, 18);
        status_box.set_valign(Align::Center);
        status_box.set_halign(Align::Center);
        status_box.set_vexpand(true);
        let status_label = gtk::Label::new(None);
        status_label.add_css_class("title-2");
        let toggle = gtk::Button::with_label("Pause");
        toggle.add_css_class("pill");
        toggle.set_halign(Align::Center);
        status_box.append(&status_label);
        status_box.append(&toggle);
        stack.add_named(&status_box, Some(STATUS_PAGE));

        let service = Rc::new(Cell::new(ServiceState::Paused));

        // The toggle pauses when active and resumes otherwise, then re-renders
        // from the freshly-detected state.
        toggle.connect_clicked(glib::clone!(
            #[weak]
            status_label,
            #[strong]
            service,
            move |toggle| {
                let running = service.get() != ServiceState::Active;
                set_service_running(&SystemRunner, running);
                render_status(
                    &status_label,
                    toggle,
                    &service,
                    detect_service_state(&SystemRunner),
                );
            }
        ));

        Self {
            stack,
            steps,
            status_label,
            toggle,
            service,
            settled: Rc::new(Cell::new(false)),
        }
    }

    /// The root widget to place in a window.
    pub fn widget(&self) -> &gtk::Stack {
        &self.stack
    }

    /// Whether the status view has been shown (so `main` stops reverting).
    pub fn is_settled(&self) -> bool {
        self.settled.get()
    }

    /// The name of the currently-visible page (`"wizard"` or `"status"`).
    #[cfg(test)]
    pub fn visible_page(&self) -> Option<String> {
        self.stack.visible_child_name().map(|s| s.to_string())
    }

    /// Show the setup wizard, rebuilding the step rows for `state`.
    pub fn show_wizard(&self, state: SetupState) {
        // Clear previously-rendered step rows, keeping the heading.
        let mut child = self.steps.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if widget.has_css_class("card") {
                self.steps.remove(&widget);
            }
        }
        for step in wizard_steps(state) {
            self.steps.append(&build_step(&step));
        }
        self.stack.set_visible_child_name(WIZARD_PAGE);
    }

    /// Show the status view for the daemon's current state.
    pub fn show_status(&self, state: ServiceState) {
        render_status(&self.status_label, &self.toggle, &self.service, state);
        self.stack.set_visible_child_name(STATUS_PAGE);
        self.settled.set(true);
    }
}

/// Render the status label and toggle for a daemon state.
fn render_status(
    label: &gtk::Label,
    toggle: &gtk::Button,
    service: &Rc<Cell<ServiceState>>,
    state: ServiceState,
) {
    service.set(state);
    let (text, action) = match state {
        ServiceState::Active => ("mackey is active", "Pause"),
        ServiceState::Paused => ("mackey is paused", "Resume"),
        ServiceState::Failed => ("mackey failed to start", "Resume"),
    };
    label.set_text(text);
    toggle.set_label(action);
}

/// Build the card for one wizard step.
fn build_step(step: &WizardStep) -> gtk::Widget {
    let card = gtk::Box::new(Orientation::Vertical, 8);
    card.add_css_class("card");

    let header = gtk::Box::new(Orientation::Horizontal, 8);
    let marker = gtk::Label::new(Some(if step.done { "✓" } else { "○" }));
    if step.done {
        marker.add_css_class("success");
    }
    let title = gtk::Label::new(Some(step.title));
    title.add_css_class("heading");
    title.set_halign(Align::Start);
    header.append(&marker);
    header.append(&title);
    card.append(&header);

    let explanation = gtk::Label::new(Some(step.explanation));
    explanation.add_css_class("dim-label");
    explanation.set_halign(Align::Start);
    explanation.set_wrap(true);
    explanation.set_xalign(0.0);
    card.append(&explanation);

    if step.done {
        let done = gtk::Label::new(Some("Done"));
        done.add_css_class("dim-label");
        done.set_halign(Align::Start);
        card.append(&done);
    } else {
        for cmd in step.commands {
            card.append(&build_command_row(cmd));
        }
    }

    card.upcast()
}

/// A monospace command with a Copy button. The button copies to the clipboard;
/// it never executes the command.
fn build_command_row(command: &str) -> gtk::Widget {
    let row = gtk::Box::new(Orientation::Horizontal, 8);

    let label = gtk::Label::new(Some(command));
    label.add_css_class("monospace");
    label.set_halign(Align::Start);
    label.set_hexpand(true);
    label.set_selectable(true);
    label.set_wrap(true);
    label.set_xalign(0.0);
    row.append(&label);

    let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy.set_tooltip_text(Some("Copy command"));
    copy.set_valign(Align::Center);
    let command = command.to_string();
    copy.connect_clicked(glib::clone!(
        #[strong]
        command,
        move |button| {
            button.clipboard().set_text(&command);
        }
    ));
    row.append(&copy);

    row.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Building widgets needs a display; skip cleanly where there is none (e.g.
    /// a headless CI runner) so the gate stays reliable.
    fn gtk_ready() -> bool {
        if gtk::init().is_err() {
            eprintln!("skipping UI test: no display");
            return false;
        }
        let _ = adw::init();
        true
    }

    #[test]
    fn wizard_and_status_swap_the_visible_page() {
        if !gtk_ready() {
            return;
        }
        let ui = WizardUi::new();

        ui.show_wizard(SetupState {
            service_active: false,
            extension_enabled: false,
        });
        assert_eq!(ui.visible_page().as_deref(), Some("wizard"));
        assert!(!ui.is_settled());

        ui.show_status(ServiceState::Active);
        assert_eq!(ui.visible_page().as_deref(), Some("status"));
        assert_eq!(
            ui.toggle.label().map(|s| s.to_string()).as_deref(),
            Some("Pause")
        );
        assert!(ui.is_settled());

        // Paused state flips the toggle to Resume.
        ui.show_status(ServiceState::Paused);
        assert_eq!(
            ui.toggle.label().map(|s| s.to_string()).as_deref(),
            Some("Resume")
        );
    }
}

//! The GTK4 wizard window.
//!
//! [`WizardUi`] owns a [`gtk::Stack`] with two pages — the setup wizard and the
//! placeholder "ready" screen — and swaps between them from a [`SetupState`].
//! It is built independently of the [`adw::Application`] so its rendering can be
//! exercised in a unit test. The wizard only ever *displays* commands (with a
//! Copy button); it never runs anything on the user's behalf.

use gtk::prelude::*;
use gtk::{glib, Align, Orientation};

use crate::setup::SetupState;
use crate::view::{screen_for, wizard_steps, Screen, WizardStep};

const WIZARD_PAGE: &str = "wizard";
const READY_PAGE: &str = "ready";

/// Handle to the wizard widgets. GTK widgets are reference-counted GObjects, so
/// cloning shares the same underlying widgets — cheap, and lets the 5s poll
/// closure own a handle alongside the window.
#[derive(Clone)]
pub struct WizardUi {
    stack: gtk::Stack,
    /// Container the wizard steps are rebuilt into on each update.
    steps: gtk::Box,
}

impl WizardUi {
    pub fn new() -> Self {
        let stack = gtk::Stack::new();

        // Wizard page: heading + the steps container, in a scroll view.
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

        // Ready page: a placeholder until the status view lands in M10.
        let ready = adw::StatusPage::builder()
            .icon_name("emblem-ok-symbolic")
            .title("mackey is set up")
            .description("The daemon is running and the GNOME extension is enabled.")
            .build();
        stack.add_named(&ready, Some(READY_PAGE));

        Self { stack, steps }
    }

    /// The root widget to place in a window.
    pub fn widget(&self) -> &gtk::Stack {
        &self.stack
    }

    /// The name of the currently-visible page (`"wizard"` or `"ready"`).
    #[cfg(test)]
    pub fn visible_page(&self) -> Option<String> {
        self.stack.visible_child_name().map(|s| s.to_string())
    }

    /// Re-render for the given state: rebuild the step rows and select the page.
    pub fn update(&self, state: SetupState) {
        // Clear previously-rendered step rows, keeping the heading (first child).
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

        let page = match screen_for(state) {
            Screen::Wizard => WIZARD_PAGE,
            Screen::Ready => READY_PAGE,
        };
        self.stack.set_visible_child_name(page);
    }
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
    fn flipping_state_swaps_the_visible_page() {
        if !gtk_ready() {
            return;
        }
        let ui = WizardUi::new();

        ui.update(SetupState {
            service_active: false,
            extension_enabled: false,
        });
        assert_eq!(ui.visible_page().as_deref(), Some("wizard"));

        ui.update(SetupState {
            service_active: true,
            extension_enabled: true,
        });
        assert_eq!(ui.visible_page().as_deref(), Some("ready"));

        // And back to the wizard if a prerequisite regresses.
        ui.update(SetupState {
            service_active: true,
            extension_enabled: false,
        });
        assert_eq!(ui.visible_page().as_deref(), Some("wizard"));
    }
}

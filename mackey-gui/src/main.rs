//! The mackey GUI: a GTK4 wizard + status window.
//!
//! M10: a single libadwaita window with no tray and no background process. On
//! launch it shows the setup wizard until the daemon and GNOME extension are
//! both present, then the status view with a Pause/Resume toggle. The wizard
//! only displays commands (never runs them); the toggle runs the
//! polkit-authorized helper via pkexec. It re-checks every 5 seconds, but once
//! the status view is shown it does not revert to the wizard in-session —
//! regressions are caught on the next launch.

mod setup;
mod ui;
mod view;

use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use setup::{detect, detect_service_state, SystemRunner};
use ui::WizardUi;
use view::{screen_for, Screen};

const APP_ID: &str = "app.mackey.Setup";
const POLL: Duration = Duration::from_secs(5);

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let wizard = WizardUi::new();
    refresh(&wizard);

    // Re-poll every 5s so the view tracks the live state.
    let poll_ui = wizard.clone();
    glib::timeout_add_local(POLL, move || {
        refresh(&poll_ui);
        glib::ControlFlow::Continue
    });

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&adw::HeaderBar::new());
    content.append(wizard.widget());

    adw::ApplicationWindow::builder()
        .application(app)
        .title("mackey")
        .default_width(560)
        .default_height(480)
        .content(&content)
        .build()
        .present();
}

/// Re-render from the current state. Once the status view is shown we only
/// refresh it; we never drop back to the wizard within a session.
fn refresh(ui: &WizardUi) {
    if ui.is_settled() {
        ui.show_status(detect_service_state(&SystemRunner));
        return;
    }
    match screen_for(detect(&SystemRunner)) {
        Screen::Ready => ui.show_status(detect_service_state(&SystemRunner)),
        Screen::Wizard => ui.show_wizard(detect(&SystemRunner)),
    }
}

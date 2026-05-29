//! The mackey GUI: a GTK4 wizard window.
//!
//! M9: a single libadwaita window with no tray and no background process. On
//! launch it detects whether the daemon is running and the GNOME extension is
//! enabled; if either is missing it shows the setup wizard (copyable commands,
//! never run on the user's behalf), otherwise a placeholder ready screen. It
//! re-checks every 5 seconds so the view follows the real state.

mod setup;
mod ui;
mod view;

use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use setup::{detect, SystemRunner};
use ui::WizardUi;

const APP_ID: &str = "app.mackey.Setup";
const POLL: Duration = Duration::from_secs(5);

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let wizard = WizardUi::new();
    wizard.update(detect(&SystemRunner));

    // Re-poll every 5s so the wizard tracks the live setup state.
    let poll_ui = wizard.clone();
    glib::timeout_add_local(POLL, move || {
        poll_ui.update(detect(&SystemRunner));
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

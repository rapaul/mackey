//! The focus tracker D-Bus service.
//!
//! Owns `app.mackey.FocusTracker` on the **system** bus and exposes a single
//! method, `UpdateFocus(app_id, window_title)`. The D-Bus policy lets any local
//! user call it, so the daemon enforces, on every call, that the caller's uid is
//! the active local-seat user's uid (per logind). Mismatches are logged and
//! dropped. At M7 an accepted call only records the app id; the keymap doesn't
//! consult it until M8.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use mackey_core::{FocusState, FOCUS_TRACKER_NAME, FOCUS_TRACKER_PATH};
use zbus::message::Header;
use zbus::names::{BusName, UniqueName};
use zbus::zvariant::OwnedObjectPath;
use zbus::{connection, fdo, interface, Connection, Proxy};

/// The focus state shared with the reader threads, which read it to pick the
/// active keymap per key event.
pub type SharedFocus = Arc<Mutex<FocusState>>;

/// Resolves the uid of the active local-seat session for a given connection.
/// Boxed so the unit test can substitute a fixed value for the live logind
/// lookup; production always uses [`logind_seat_resolver`].
type SeatUidResolver = Box<
    dyn for<'a> Fn(&'a Connection) -> Pin<Box<dyn Future<Output = Option<u32>> + Send + 'a>>
        + Send
        + Sync,
>;

/// The production resolver: query logind live on every call.
fn logind_seat_resolver() -> SeatUidResolver {
    Box::new(|conn| Box::pin(active_seat_uid(conn)))
}

struct FocusTracker {
    focus: SharedFocus,
    /// Monotonic origin for the focus timestamps the stale-fallback uses.
    start: Instant,
    active_seat_uid: SeatUidResolver,
}

#[interface(name = "app.mackey.FocusTracker")]
impl FocusTracker {
    async fn update_focus(
        &self,
        app_id: String,
        _window_title: String,
        #[zbus(header)] hdr: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) {
        let Some(sender) = hdr.sender() else {
            eprintln!("rejected UpdateFocus: message has no sender");
            return;
        };
        let Some(sender_uid) = connection_uid(conn, sender).await else {
            eprintln!("rejected UpdateFocus: cannot resolve sender uid");
            return;
        };
        if (self.active_seat_uid)(conn).await != Some(sender_uid) {
            eprintln!("rejected UpdateFocus from uid={sender_uid}");
            return;
        }
        let now_ms = self.start.elapsed().as_millis() as u64;
        let mut state = self.focus.lock().unwrap();
        let prev = state.active_keymap(now_ms).id;
        let focus_changed = state.app_id() != Some(app_id.as_str());
        state.update(app_id.clone(), now_ms);
        let active = state.active_keymap(now_ms).id;
        drop(state);
        // Heartbeats re-send the same app id every couple of seconds; only log
        // when the focused app actually changes, not on each heartbeat.
        if focus_changed {
            eprintln!("accepted UpdateFocus app_id={app_id}");
        }
        if active != prev {
            eprintln!("keymap → {active}");
        }
    }
}

/// The uid behind a D-Bus connection, via the bus daemon.
async fn connection_uid(conn: &Connection, sender: &UniqueName<'_>) -> Option<u32> {
    let dbus = fdo::DBusProxy::new(conn).await.ok()?;
    dbus.get_connection_unix_user(BusName::Unique(sender.to_owned()))
        .await
        .ok()
}

/// The uid of the active session on seat0, via logind.
async fn active_seat_uid(conn: &Connection) -> Option<u32> {
    let seat = Proxy::new(
        conn,
        "org.freedesktop.login1",
        "/org/freedesktop/login1/seat/seat0",
        "org.freedesktop.login1.Seat",
    )
    .await
    .ok()?;
    let (_id, session_path): (String, OwnedObjectPath) =
        seat.get_property("ActiveSession").await.ok()?;

    let session = Proxy::new(
        conn,
        "org.freedesktop.login1",
        session_path,
        "org.freedesktop.login1.Session",
    )
    .await
    .ok()?;
    let (uid, _path): (u32, OwnedObjectPath) = session.get_property("User").await.ok()?;
    Some(uid)
}

/// Run the focus tracker. Blocks forever; call from a dedicated thread.
pub fn serve(focus: SharedFocus, start: Instant) {
    if let Err(e) = zbus::block_on(run(focus, start)) {
        eprintln!("focus tracker D-Bus error: {e}");
    }
}

async fn run(focus: SharedFocus, start: Instant) -> zbus::Result<()> {
    let tracker = FocusTracker {
        focus,
        start,
        active_seat_uid: logind_seat_resolver(),
    };
    let _conn = connection::Builder::system()?
        .name(FOCUS_TRACKER_NAME)?
        .serve_at(FOCUS_TRACKER_PATH, tracker)?
        .build()
        .await?;
    eprintln!("owning {FOCUS_TRACKER_NAME} on the system bus");
    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};

    /// A throwaway dbus-daemon, isolated from the user's real session bus. The
    /// daemon implements `GetConnectionUnixUser`, which the focus tracker needs
    /// to resolve a caller's uid — a peer-to-peer connection wouldn't have it.
    struct PrivateBus {
        child: Child,
        address: String,
    }

    impl PrivateBus {
        fn start() -> Option<Self> {
            let mut child = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;
            let stdout = child.stdout.take()?;
            let mut address = String::new();
            BufReader::new(stdout).read_line(&mut address).ok()?;
            let address = address.trim().to_string();
            if address.is_empty() {
                let _ = child.kill();
                return None;
            }
            Some(PrivateBus { child, address })
        }
    }

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    async fn connect(addr: &str) -> Connection {
        connection::Builder::address(addr)
            .unwrap()
            .build()
            .await
            .unwrap()
    }

    /// Serve a tracker whose seat resolver reports `seat_uid`, then call
    /// `UpdateFocus(app_id, ..)` from a client on the same bus. Returns what the
    /// tracker recorded — `Some(app_id)` if it accepted, `None` if it rejected.
    async fn serve_and_call(addr: &str, seat_uid: u32, app_id: &str) -> Option<String> {
        let focus: SharedFocus = Arc::new(Mutex::new(FocusState::new()));
        let resolver: SeatUidResolver = Box::new(move |_| Box::pin(async move { Some(seat_uid) }));
        let tracker = FocusTracker {
            focus: focus.clone(),
            start: Instant::now(),
            active_seat_uid: resolver,
        };
        // Serve on its own unique name (no well-known name) so successive calls
        // on one bus don't contend for ownership.
        let server = connection::Builder::address(addr)
            .unwrap()
            .serve_at(FOCUS_TRACKER_PATH, tracker)
            .unwrap()
            .build()
            .await
            .unwrap();
        let server_name = server.unique_name().unwrap().to_owned();

        let client = connect(addr).await;
        let proxy = Proxy::new(
            &client,
            server_name.as_str(),
            FOCUS_TRACKER_PATH,
            "app.mackey.FocusTracker",
        )
        .await
        .unwrap();
        let _: () = proxy.call("UpdateFocus", &(app_id, "title")).await.unwrap();

        let got = focus.lock().unwrap().app_id().map(str::to_owned);
        got
    }

    /// The uid the bus daemon attributes to our own connections — what the
    /// tracker will see as the caller's `sender_uid`.
    async fn own_uid(addr: &str) -> u32 {
        let conn = connect(addr).await;
        let dbus = fdo::DBusProxy::new(&conn).await.unwrap();
        let name = conn.unique_name().unwrap().to_owned();
        dbus.get_connection_unix_user(BusName::Unique(name.into()))
            .await
            .unwrap()
    }

    #[test]
    fn accepts_only_when_seat_uid_matches_the_caller() {
        let Some(bus) = PrivateBus::start() else {
            eprintln!("skipping: dbus-daemon unavailable");
            return;
        };
        zbus::block_on(async {
            let uid = own_uid(&bus.address).await;

            // Seat uid matches the caller → accepted, app id recorded.
            let accepted = serve_and_call(&bus.address, uid, "firefox.desktop").await;
            assert_eq!(accepted, Some("firefox.desktop".to_string()));

            // Seat uid differs from the caller → rejected, nothing recorded.
            let rejected = serve_and_call(&bus.address, uid.wrapping_add(1), "evil.desktop").await;
            assert_eq!(rejected, None);
        });
    }
}

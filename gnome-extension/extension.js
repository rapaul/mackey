// mackey focus tracker — GNOME Shell extension.
//
// GNOME Wayland exposes no public window-focus API to unprivileged processes,
// so the daemon relies on this extension (running in the seat user's session)
// to tell it which app is focused. On every focus change it calls
// `app.mackey.FocusTracker.UpdateFocus(app_id, title)` on the system bus. The
// daemon authenticates the caller's uid against the active seat, so a hostile
// caller cannot drive another user's keymap. There is no UI and no state.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const DEST = 'app.mackey.FocusTracker';
const OBJECT_PATH = '/app/mackey/FocusTracker';
const IFACE = 'app.mackey.FocusTracker';

// The daemon reverts to the global keymap if no UpdateFocus arrives for >5s
// (its stale-fallback for a lost focus signal). Focus-change events alone are
// too sparse to keep it fresh — staying in one window for >5s would trip it —
// so re-send the current focus on this interval. Must stay well under the
// daemon's 5s stale window.
const HEARTBEAT_SECONDS = 2;

export default class MackeyFocusTracker extends Extension {
    enable() {
        this._tracker = Shell.WindowTracker.get_default();
        this._focusHandler = global.display.connect(
            'notify::focus-window',
            () => this._onFocusChanged());
        // Push the currently-focused app once on enable.
        this._onFocusChanged();
        // Re-send the current focus periodically so the daemon's stale-fallback
        // only fires when this extension is genuinely gone, not during normal
        // dwell in a single window.
        this._heartbeat = GLib.timeout_add_seconds(
            GLib.PRIORITY_DEFAULT, HEARTBEAT_SECONDS,
            () => {
                this._onFocusChanged();
                return GLib.SOURCE_CONTINUE;
            });
    }

    disable() {
        if (this._focusHandler) {
            global.display.disconnect(this._focusHandler);
            this._focusHandler = null;
        }
        if (this._heartbeat) {
            GLib.Source.remove(this._heartbeat);
            this._heartbeat = null;
        }
        this._tracker = null;
    }

    _onFocusChanged() {
        const win = global.display.focus_window;
        if (!win)
            return;

        const app = this._tracker.get_window_app(win);
        const appId = app ? app.get_id() : '';
        if (!appId)
            return;

        this._updateFocus(appId, win.get_title() ?? '');
    }

    _updateFocus(appId, title) {
        Gio.DBus.system.call(
            DEST, OBJECT_PATH, IFACE, 'UpdateFocus',
            new GLib.Variant('(ss)', [appId, title]),
            null, Gio.DBusCallFlags.NONE, -1, null,
            (connection, result) => {
                try {
                    connection.call_finish(result);
                } catch (e) {
                    // Best-effort: the daemon may be paused/stopped, or may
                    // reject a non-seat caller. Don't spam the journal.
                    console.debug(`mackey: UpdateFocus failed: ${e.message}`);
                }
            });
    }
}

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

export default class MackeyFocusTracker extends Extension {
    enable() {
        this._tracker = Shell.WindowTracker.get_default();
        this._focusHandler = global.display.connect(
            'notify::focus-window',
            () => this._onFocusChanged());
        // Push the currently-focused app once on enable.
        this._onFocusChanged();
    }

    disable() {
        if (this._focusHandler) {
            global.display.disconnect(this._focusHandler);
            this._focusHandler = null;
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

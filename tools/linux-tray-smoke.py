"""Run with dbus-run-session -- xvfb-run -a /usr/bin/python3 tools/linux-tray-smoke.py BINARY.

Requires python3-dbus and python3-gi. Start without a watcher, then bring one
online and click Quit through the menu exported by the ordinary executable.
"""
import os
import subprocess
import sys
import tempfile
import time

import dbus
import dbus.service
from dbus.mainloop.glib import DBusGMainLoop
from gi.repository import GLib


WATCHER = "org.kde.StatusNotifierWatcher"


class Watcher(dbus.service.Object):
    service = None

    @dbus.service.method(WATCHER, in_signature="s", out_signature="")
    def RegisterStatusNotifierItem(self, service):
        self.service = service


def wait_until(test, app):
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        GLib.MainContext.default().iteration(False)
        assert app.poll() is None, "Wraex exited during startup"
        if result := test():
            return result
        time.sleep(0.02)
    raise TimeoutError("Timed out waiting for the live tray")


def main():
    DBusGMainLoop(set_as_default=True)
    bus = dbus.SessionBus()
    assert not bus.name_has_owner(WATCHER), "Run in a private dbus-run-session"
    with tempfile.TemporaryDirectory() as config:
        app = subprocess.Popen([sys.argv[1]], env=dict(os.environ, XDG_CONFIG_HOME=config))
        try:
            wait_until(lambda: any("StatusNotifierItem-" in n for n in bus.list_names()), app)
            watcher = Watcher(bus, "/StatusNotifierWatcher")
            owner = dbus.service.BusName(WATCHER, bus)
            wait_until(lambda: watcher.service, app)
            item = bus.get_object(watcher.service, "/StatusNotifierItem")
            path = item.Get("org.kde.StatusNotifierItem", "Menu", dbus_interface=dbus.PROPERTIES_IFACE)
            menu = dbus.Interface(bus.get_object(watcher.service, path), "com.canonical.dbusmenu")
            _, layout = menu.GetLayout(0, 1, ["label"])
            actions = {props["label"]: idx for idx, props, _ in layout[2] if "label" in props}
            assert set(actions) == {"Settings/League", "Open config folder", "Quit"}
            menu.Event(actions["Quit"], "clicked", dbus.Int32(0), 0)
            assert app.wait(timeout=15) == 0
            del owner
        finally:
            if app.poll() is None:
                app.kill()
            app.wait()


if __name__ == "__main__":
    main()

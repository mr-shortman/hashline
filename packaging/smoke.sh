#!/bin/sh
# Run only in a disposable Ubuntu 26.04 environment, e.g. packaging/Dockerfile.
set -eu
package=$(readlink -f "$1")
package_version=$(dpkg-deb -f "$package" Version)
upstream_version=${package_version%-*}
schema=de.kalendium.Hashline
desktop=/usr/share/applications/$schema.desktop

test "$(id -u)" = 0
test ! -e /usr/bin/hashline
test ! -e "$desktop"
test ! -e /usr/share/glib-2.0/schemas/$schema.gschema.xml
# An existing user association must survive installation and removal.
mkdir -p "$XDG_CONFIG_HOME"
mkdir -p "$XDG_DATA_HOME/applications"
cat > "$XDG_DATA_HOME/applications/existing-reader.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Existing Reader
Exec=true %f
MimeType=text/markdown;
EOF
printf '[Default Applications]\ntext/markdown=existing-reader.desktop;\n' > "$XDG_CONFIG_HOME/mimeapps.list"
cp "$XDG_CONFIG_HOME/mimeapps.list" /tmp/mimeapps.before

check_installed() {
    test "$(dpkg-query -W -f='${Status}' hashline)" = 'install ok installed'
    test -z "$(dpkg -V hashline)"
    desktop-file-validate "$desktop"
    test "$(env -u DISPLAY -u WAYLAND_DISPLAY hashline --version)" = "Hashline $upstream_version"
    env -u DISPLAY -u WAYLAND_DISPLAY hashline --help >/dev/null
    # Use the system schema cache; there is no build tree in this container.
    gsettings list-keys "$schema" | sort
    test "$(gsettings get "$schema" theme)" = "'system'"
    gsettings set "$schema" theme dark
    test "$(gsettings get "$schema" theme)" = "'dark'"
    gsettings reset "$schema" theme
    for suffix in md markdown mdown mkd mkdn mdwn; do
        printf '# Markdown\n\nInhalt.\n' > "/tmp/Hashline Grüße.$suffix"
        gio info -a standard::content-type "/tmp/Hashline Grüße.$suffix" | grep -q 'text/markdown'
    done
    /usr/bin/python3 - <<'PY'
import gi
gi.require_version('Gtk', '4.0')
from gi.repository import Gtk, Gdk, Gio
Gtk.init()
assert Gtk.IconTheme.get_for_display(Gdk.Display.get_default()).has_icon('de.kalendium.Hashline')
assert Gio.AppInfo.get_default_for_type('text/markdown', False).get_id() == 'existing-reader.desktop'
PY
    /usr/bin/python3 /tests/native_reader.py /usr/bin/hashline --desktop
    cmp /tmp/mimeapps.before "$XDG_CONFIG_HOME/mimeapps.list"
}

apt-get install -y --no-install-recommends "$package"
check_installed
# Repeat unpack/configure to cover upgrades and cache triggers.
dpkg -i "$package"
check_installed
apt-get remove -y hashline
test ! -e /usr/bin/hashline
test ! -e "$desktop"
test ! -e /usr/share/icons/hicolor/scalable/apps/$schema.svg
test ! -e /usr/share/mime/packages/$schema.xml
test ! -e /usr/share/glib-2.0/schemas/$schema.gschema.xml
if gsettings list-schemas | grep -Fx "$schema"; then
    echo 'Schema remained in the shared cache after removal' >&2
    exit 1
fi
cmp /tmp/mimeapps.before "$XDG_CONFIG_HOME/mimeapps.list"
apt-get install -y --no-install-recommends "$package"
check_installed
apt-get purge -y hashline
cmp /tmp/mimeapps.before "$XDG_CONFIG_HOME/mimeapps.list"
echo 'PASS: clean install, desktop/MIME open, settings, icon, instance handoff, reinstall, removal and purge'

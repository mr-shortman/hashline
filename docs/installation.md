# Linux-Installation

Das erste Paket richtet sich an **Ubuntu 26.04 LTS, amd64**, die Distribution
der bisherigen Messumgebung. Es enthält den nativen GTK4-Release. Andere
Distributionen und Architekturen sind damit nicht freigegeben. Die vollständige
v1-Abnahme steht wegen der [bekannten Einschränkungen](limitations.md) aus.

## Paket bauen

Rust 1.98.1 über rustup installieren; die Version steht in
`rust-toolchain.toml`. Auf Ubuntu 26.04:

```sh
sudo apt install libgtk-4-dev build-essential pkg-config python3 dpkg-dev \
    desktop-file-utils libglib2.0-bin shared-mime-info
python3 packaging/build_deb.py
```

Der Befehl baut mit `Cargo.lock` einen Release, validiert Desktop-Eintrag,
MIME-Daten und GSettings-Schema und schreibt
`target/packages/hashline_0.1.0-1_amd64.deb` samt SHA-256-Datei. Er installiert
nichts und braucht keine Rootrechte. Die Versionsnummer kommt aus dem
Workspace; `--revision` setzt die Debian-Revision, `--maintainer` den
Paketbetreuer. Ohne Angabe heißt dieser „Hashline contributors“.

`dpkg-shlibdeps` ermittelt die Laufzeitabhängigkeiten aus dem tatsächlichen
Binary. Deshalb auf der Zieldistribution bauen: ein auf einem neueren System
gebautes Paket ist nicht automatisch mit älteren GTK-/GLib-Versionen kompatibel.
`SOURCE_DATE_EPOCH` kommt standardmäßig aus dem letzten Git-Commit; beim Bau
aus einem Quellarchiv muss die Variable gesetzt sein. Gleicher Quellstand,
Toolchain, Pfad und Zeitstempel ergeben wiederholbare Paketarchive.

## Installieren und öffnen

```sh
sudo apt install ./target/packages/hashline_0.1.0-1_amd64.deb
hashline --version
hashline "Dokumente/erste Datei.md"
```

Hashline erscheint im Anwendungsmenü und im „Öffnen mit“-Dialog für Markdown.
Der Desktop-Eintrag heißt `de.kalendium.Hashline.desktop`, verwendet
`Name=Hashline`, `GenericName=Markdown Viewer`, `Exec=hashline %f` und dieselbe
ID wie Fenster und Icon. Weitere Aufrufe öffnen im bestehenden Fenster; relative
Pfade gehören zum Arbeitsverzeichnis des jeweiligen Aufrufers. Bei mehreren
Dateien öffnet die erste, begleitet von einem Hinweis. `hashline -- -notizen.md`
öffnet einen Dateinamen, der mit einem Minuszeichen beginnt.

Das Paket registriert Markdown-Unterstützung, verändert aber keine bestehende
Standardzuordnung. Optional lässt sie sich im Dateimanager oder ausdrücklich
mit `xdg-mime default de.kalendium.Hashline.desktop text/markdown` setzen.

Schema, MIME-Daten und Icons liegen unter `/usr/share`; die Trigger der
Systempakete aktualisieren die gemeinsamen Caches bei Installation, Upgrade
und Entfernung. Kompilierte Caches gehören nicht zum Hashline-Paket. Der
GSettings-Backend speichert Präferenzen; das installierte Programm benötigt
keinen Zugriff auf den Buildbaum und kein `GSETTINGS_SCHEMA_DIR`.

```sh
sudo apt remove hashline
```

Benutzereinstellungen bleiben dabei erhalten. Sie lassen sich bei Bedarf mit
`gsettings reset-recursively de.kalendium.Hashline` vor der Entfernung löschen.

## Isolierte Installationsprüfung

Nach dem Paketbau, mit verfügbarem Docker:

```sh
docker build -f packaging/Dockerfile -t hashline-package-test .
docker run --rm --network=bridge hashline-package-test
```

Im Container wird das Paket tatsächlich über APT installiert, erneut
konfiguriert, entfernt, wieder installiert und vollständig entfernt. Unter
Xvfb und eigenem D-Bus prüfen GIO und AT-SPI Desktop-/MIME-Start,
Instanzübergabe, Dateinamen mit Sonderzeichen, das installierte Schema und
Icon. Eine vorhandene Standardzuordnung muss unverändert bleiben. Der
Container bekommt nur Paket und Tests, keinen Quell- oder Buildbaum.
Das ersetzt keine physische Wayland-, Dateimanager- oder visuelle Abnahme.

Ergebnisse und verbleibende Freigabepunkte: [M3-Abnahmebericht](acceptance/M3.md).

# Hashline

A fast, quiet Markdown viewer for Linux. Lokaler, schreibgeschützter Reader mit
Tauri 2, React und TypeScript. Ein Fenster, eine Datei, keine Konten oder Telemetrie.

Dateien lassen sich per Dialog, Drag-and-drop, Dateimanager oder `hashline datei.md`
öffnen. Hashline bietet GFM-Darstellung, Inhaltsverzeichnis, Textsuche, Code-Kopieren,
lokale Bilder, relative Markdown-Links, automatische Aktualisierung, Lesepositionen,
Themes und Textzoom.

**Status:** Implementierung und Linux-Release-Paket vorhanden. Die vollständige
v1-Abnahme nach `SPEC.md` ist noch nicht erteilt. Messwerte und offene Abnahmepunkte
stehen in [benchmarks/REPORT.md](benchmarks/REPORT.md).

## Entwickeln

Referenzumgebung: Ubuntu 26.04 LTS, WebKitGTK 2.52.6. Node.js 22.22.1 und Rust
1.98.1 sind in `.nvmrc` und `rust-toolchain.toml` festgelegt. npm- und Cargo-Lockfiles
gehören zum Projekt. Der fertige Reader benötigt keinen Node-Server.

```sh
sudo apt-get install build-essential pkg-config libwebkit2gtk-4.1-dev \
  libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf desktop-file-utils
npm ci
npm run desktop
```

Rust mit [rustup](https://rustup.rs/) installieren; die Toolchain-Datei wählt die
Projektversion. `npm run dev` startet eine Browser-Vorschau. Dort funktionieren
Dateidialog und Rendering, während Dateibeobachtung, lokale Bildpfade und
Systemintegration die Desktop-App voraussetzen.

Für den direkten Desktop-Test mit Beispieldokument:

```sh
npm run dev:demo
# Oder eine eigene Datei:
npm run desktop -- --file ./README.md
```

Der Befehl startet Vite und das native Fenster gemeinsam. CSS und React nutzen
Live-Updates; Rust-Änderungen bauen die App neu und starten sie erneut. Änderungen
am geöffneten Markdown lädt der Dateiwatcher automatisch. `npm run dev:doctor`
prüft die Build-Werkzeuge. Weitere Details zu Reloads, Inspector und lokalen
Werkzeugpfaden stehen in [docs/development.md](docs/development.md).

## Prüfen und paketieren

```sh
npm run check
npm run test:ui               # installiertes Google Chrome erforderlich
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run bundle
```

Das Debian-Paket liegt anschließend unter
`src-tauri/target/release/bundle/deb/Hashline_0.1.0_amd64.deb`.

```sh
sudo apt install ./src-tauri/target/release/bundle/deb/Hashline_0.1.0_amd64.deb
hashline ./README.md
```

Installation registriert Desktop-Eintrag, Icon und Markdown-MIME-Typ. Die bevorzugte
Standardanwendung bleibt eine Benutzereinstellung des Dateimanagers. Native
Integrationstests und Messabläufe beschreibt [docs/testing.md](docs/testing.md).

## Bedienung

Die Desktop-App vereint Werkzeuge und Fenstersteuerung in einer selbst gestalteten
Titelleiste: flache Knöpfe rechts unter Windows, Ampelknöpfe links unter macOS und
runde Knöpfe rechts unter Linux. Dateiname und freie Flächen lassen sich zum
Verschieben ziehen; ein Doppelklick maximiert das Fenster oder stellt es wieder her.
Der grüne macOS-Knopf schaltet Vollbild um. Die Leiste folgt dem gewählten Theme.
In der Browser-Vorschau werden keine Fensterknöpfe eingeblendet.

| Aktion                                       | Tastatur                           |
| -------------------------------------------- | ---------------------------------- |
| Öffnen                                       | Strg+O                             |
| Suche                                        | Strg+F                             |
| Nächster / vorheriger Treffer                | Enter / Umschalt+Enter im Suchfeld |
| Inhaltsverzeichnis                           | Strg+Umschalt+O                    |
| Text vergrößern / verkleinern / zurücksetzen | Strg++ / Strg+- / Strg+0           |
| Nachladen                                    | Strg+R                             |
| Dokument auswählen / kopieren                | Strg+A / Strg+C                    |
| Suche bzw. oberstes Overlay schließen        | Escape                             |

Das Menü enthält Theme, Textgröße, Nachladen und „Dateipfad kopieren“. Der Dateiname
zeigt den vollständigen Pfad als Tooltip. Start ohne Dateiparameter bleibt leer;
Lesepositionen werden erst beim erneuten Öffnen einer Datei angewendet.

## Inhaltsgrenzen

- Markdown: UTF-8 einschließlich BOM, höchstens 20 MiB. Dateien werden nie verändert.
- Lokale PNG-, JPEG-, GIF- und WebP-Bilder: höchstens 16 MiB und 24 Megapixel je Bild.
  Automatischer Zugriff bleibt im Dokumentverzeichnis einschließlich Unterordnern.
- Remote-Bilder und SVG werden derzeit als Platzhalter dargestellt. Es gibt keine
  Remote-Freigabeaktion. Diese Einschränkung ist in der Ressourcenentscheidung dokumentiert.
- Links zu Markdown öffnen im selben Fenster. HTTP(S) und Mail öffnen nach einem
  Klick in der Systemanwendung. Andere Schemata und Dateitypen werden abgewiesen.
- Passive HTML-Auswahl; keine Dokument-Styles, Skripte, Formulare oder Frames.
  Aufgabenlisten bleiben deaktiviert. Frontmatter steuert keine App-Funktionen.

[Architektur und Rendering-Vertrag](docs/architecture.md) ·
[Entscheidungen](docs/decisions/001-resources.md) ·
[Benchmarkbericht](benchmarks/REPORT.md)

# Entwicklung

Hashline ist ein nativer GTK4-Prozess in Rust. Es gibt keine Node-Toolchain,
keinen Frontend-Build (SPEC.md, Abschnitt 4). Die native `.deb`-Paketierung
steht in [installation.md](installation.md).

## Voraussetzungen

```sh
sudo apt install libgtk-4-dev build-essential pkg-config
```

Alles Weitere, was `gtk4-rs` braucht — GLib, Pango, Cairo, gdk-pixbuf,
Graphene —, kommt als Abhängigkeit dieser Pakete. Mindestversion ist GTK 4.14:
`GtkAccessible` gibt es ab 4.10, die `GtkAccessibleText`-Schnittstelle, über die
das Dokumentwidget seinen Text meldet, ab 4.14.

Rust ist in `rust-toolchain.toml` auf 1.98.1 festgelegt; `rustup` wählt es
selbst. Für die Desktop-Prüfungen zusätzlich `python3-gi` und
`gir1.2-atspi-2.0`, für die Frametime-Messung `sysprof`.

## Bauen und starten

```sh
cargo run -p hashline -- SPEC.md
cargo build --release --workspace
```

`build.rs` übersetzt das GSettings-Schema bei jedem Bau mit, und die Anwendung
findet es dort, wenn keines installiert ist. Ein Entwicklungsbaum braucht
deshalb kein `GSETTINGS_SCHEMA_DIR` — und kann auch nicht mehr gegen ein
veraltetes kompiliertes Schema laufen.

Ein Schema, das einen Schlüssel *nicht* enthält, wäre für GIO ein
Programmierfehler und würde den Prozess beenden. `preferences::Preferences`
prüft deshalb jeden Zugriff über `has_key` und arbeitet ohne Speicher weiter,
statt abzubrechen (SPEC.md, Abschnitt 7).

### `HASHLINE_MONITOR`

Ein Entwicklungshilfsmittel für Mehrschirmarbeit: die Variable wählt den
Monitor, auf dem das Fenster erscheint, verglichen gegen Connector, Modell,
Hersteller oder Beschreibung.

```sh
HASHLINE_MONITOR=DP-3 cargo run -p hashline -- SPEC.md
```

Trifft nichts zu, listet das Programm die vorhandenen Monitore und platziert
normal. Wayland erlaubt einem Client nicht, seine Fenster selbst zu setzen; die
einzige Ausnahme ist `fullscreen_on_monitor`, weil eine Fullscreen-Surface ihren
Output benennen muss. Kurz hinein und direkt wieder heraus lässt das Fenster in
Normalgröße auf dem gewählten Monitor stehen. Ohne die Variable passiert nichts
und der Compositor platziert.

## Aufbau

```text
crates/markdown/    Parser und Op-Buffer, ohne Toolkit- und Plattformbezug
crates/hashline/    Die Anwendung
  app/              GApplication, Fenster, HeaderBar, Aktionen, Laden
  document/         Dateibeobachtung, Leseanker, Digest
  layout/           Blockplan, Höhen, Prefix-Summen, Op-Buffer → Pango
  view/             Dokumentwidget, Auswahl, Bilder, Barrierefreiheit
  search/           Suche auf dem Textblob
  outline/          Überschriften und ihre Blöcke
  highlight/        Syntaxfarben über syntect
  preferences/      GSettings, Lesepositionen
  theme/            Design-Tokens, Hell/Dunkel
data/               Desktop-Eintrag, MIME, Icons, GSettings-Schema
docs/design/        Gestaltungsreferenz (CSS des alten Stands)
tests/fixtures/     Markdown-, Bild- und Fehlerfälle
benchmarks/         Generatoren, Messwerkzeuge, Rohdaten
```

`crates/markdown` bleibt frei von GTK und Dateisystem. Markdown-Regeln stehen an
genau einer Stelle.

## Prüfungen

Siehe [Prüfungen und reproduzierbare Desktop-Tests](testing.md). Kurzfassung:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Was noch fehlt

Die M3-Auslieferungsfunktionen sind umgesetzt. Die vollständige Freigabe bleibt
wegen der [bekannten Einschränkungen](limitations.md) offen; Paketprüfungen
und Abnahmegrenzen stehen im [M3-Bericht](acceptance/M3.md).

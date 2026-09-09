# Prüfungen und reproduzierbare Desktop-Tests

Die Anwendung ist ein einziger nativer Prozess. Es gibt keine Node-Toolchain,
keinen WebDriver und keine WebView mehr; alles unten läuft mit Rust, GTK4 und
`python3-gi`.

## Werkstatt

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
GSETTINGS_BACKEND=memory cargo test --workspace
cargo build --release --workspace
```

`build.rs` übersetzt `data/de.kalendium.Hashline.gschema.xml` bei jedem Bau in
das Ausgabeverzeichnis, und die Anwendung findet es dort, wenn kein Schema
installiert ist. `GSETTINGS_SCHEMA_DIR` ist damit nur nötig, wenn man
absichtlich ein anderes Schema prüfen will. `GSETTINGS_BACKEND=memory`
isoliert die Unit-Tests von persönlichen Einstellungen und benötigt keinen
schreibbaren dconf-Dienst; der Pakettest prüft zusätzlich echte Persistenz.

## Aussehen ohne Fenster

`examples/render` setzt ein Dokument über denselben Layout-Code wie das Widget
in ein PNG. Das ist der einzige Weg, das Ergebnis auf Maschinen zu beurteilen,
auf denen Bildschirmaufnahmen nicht erlaubt sind, und es taugt für den
visuellen Vergleich zweier Stände:

```sh
cargo run --release -p hashline --example render -- SPEC.md /tmp/spec.png 900
cargo run --release -p hashline --example render -- SPEC.md /tmp/spec-dark.png 900 dark
```

## Performance

Die Fixtures liegen deterministisch unter `benchmarks/generated/` und bleiben
unverändert, damit Reihen über den Stackwechsel hinweg vergleichbar sind
(SPEC.md, Abschnitt 9). `benchmarks/generate.mjs` hat sie erzeugt und ist
historisch: es braucht die entfernte Node-Toolchain und wird nicht mehr
ausgeführt — neu zu generieren würde die Messreihe entwerten.

Der eigene Anteil an der Öffnungszeit — Parsen, Blockplan, erstes Setzen —
getrennt instrumentiert, ohne Fenster:

```sh
cargo build --release -p hashline --example measure
./target/release/examples/measure benchmarks/generated/{small,medium,large}.md
```

Mit `--geometry` setzt derselbe Aufruf statt des ersten Schirms **jeden** Block
einmal und meldet, wie weit die geschätzte Gesamthöhe von der gemessenen
abweicht — die Zahl, an der Bildlaufleiste und jeder Sprung in einen noch nicht
gesetzten Block hängen. Einer Zeitmessung entgeht sie vollständig: ein
Codeblock, der als eine einzige Zeile geschätzt wurde, war um den Faktor 28.000
zu kurz und kostete im Schätzen nichts.

```sh
./target/release/examples/measure --geometry benchmarks/generated/*.md
```

Speicher der laufenden Anwendung als PSS der Prozessgruppe, ein eigener Prozess
je Fixture:

```sh
python3 benchmarks/memory.py target/release/hashline \
    --fixture - --fixture benchmarks/generated/small.md \
    --fixture benchmarks/generated/large.md \
    --renderer default --renderer gl --renderer cairo --repeat 3 \
    --output benchmarks/results/<lauf>/memory.json
```

Zwei Dinge machen eine Speicherzahl auf einem Desktop unbrauchbar, und
`memory.py` umgeht beide. Die laufende installierte Instanz hält den Busnamen,
also übergäbe ein erneuter Start die Datei nur an jenes Fenster;
`dbus-run-session` löst das, wird aber zum Elternprozess des gesamten
Portal-Stacks und misst dessen rund 60 MiB mit. Das Werkzeug startet den
privaten Bus deshalb **neben** der Anwendung, hält Portale, gvfs und dconf über
die Umgebung ganz von ihm fern und schreibt die Prozessgruppe des Busses in
jede Zeile — die Isolierung ist damit prüfbar statt geglaubt. `--fixture -`
misst das leere Fenster, also den Boden des jeweiligen Renderers. Der erste
Durchgang wärmt Treiber und Shadercache und gehört nicht in eine Auswertung;
dafür ist `--repeat` da.

Für einen einzelnen laufenden Prozess reicht weiterhin der einfache Sampler:

```sh
./target/release/hashline benchmarks/generated/large.md &
python3 benchmarks/process-sample.py <pid> 30 > /tmp/hashline-memory.json
```

Der Sampler erfasst die rekursive Prozessgruppe über `/proc`, summiert PSS und
CPU-Ticks und kennzeichnet nicht lesbare PSS-Werte. Nach 50 Dateiwechseln
dieselbe Messung wiederholen und mit der aufgewärmten Ausgangslage vergleichen.
Unter Sandbox-/Ptrace-Beschränkungen kann `/proc/<pid>/smaps_rollup` nicht
lesbar sein; fehlende Werte dürfen nicht als Nullverbrauch gelten.

### Scroll-Frametimes

> **Achtung:** `compositor.py` stellt zur Messung vorübergehend die
> Bildwiederholrate des primären Monitors um und setzt sie danach zurück. Bei
> einem Wechsel auf 120 Hz kann der Bildschirm mehrfach kurz schwarz werden.
> Nicht während anderer Arbeit ausführen.

Der Reiz ist echte Zeigereingabe über `org.gnome.Mutter.RemoteDesktop`; in der
Anwendung ist nichts instrumentiert. `scroll-native.py` sucht das Fenster,
indem es an Kandidatenpunkten scrollt und die CPU-Zeit der Anwendung prüft —
ohne diesen Nachweis meldet der Lauf einen Fehler statt einer leeren, sauberen
Messung. Die Richtung kehrt regelmäßig um, damit ein Dokument nicht am Ende
liegt und Ruhe statt Scrollen gemessen wird.

```sh
python3 benchmarks/compositor.py ./target/release/hashline \
    benchmarks/generated/large.md --refresh-hz 60 --seconds 12 \
    --output-prefix benchmarks/results/<lauf>/scroll-60
sysprof-cat --no-callgraph --no-counters benchmarks/results/<lauf>/scroll-60.syscap \
    > benchmarks/results/<lauf>/scroll-60.dump
python3 benchmarks/analyze-compositor.py benchmarks/results/<lauf>/scroll-60.dump \
    --scroll   benchmarks/results/<lauf>/scroll-60-scroll.json \
    --display  benchmarks/results/<lauf>/scroll-60-display.json \
    --capture  benchmarks/results/<lauf>/scroll-60.syscap \
    --output   benchmarks/results/<lauf>/frametimes-60.json
```

Die Auswertung liest Mutters Präsentationsmarken für den Monitor, **nicht**
Frames der Anwendung: der Wert schließt alle Clients ein und ist damit keine
Abnahme der Anwendungsdarstellung, sondern eine Untergrenze für ihre Qualität.
Das Werkzeug schreibt das selbst in jede Ausgabe.

## Manuelle Freigabe

Vor einer v1-Freigabe bleiben Installation auf sauberer Zielumgebung,
tatsächlicher Dateimanager-Aufruf und Drag-and-drop, System-Clipboard und
Linköffnen sowie die vollständigen SPEC-Performancebudgets zu bestätigen. Die
Anwendungs-ID `de.kalendium.Hashline` ist aus der Spezifikation übernommen;
ihre Herausgeberbestätigung wird nicht aus einem erfolgreichen Build abgeleitet.

Der [Abnahmebericht](acceptance/REPORT.md) und das
[Performance-Protokoll](../benchmarks/REFERENCE.md) beschreiben Prüfungen der
WebView-Fassung. Sie bleiben als Historie und als Vergleichsbasis erhalten; die
Befehle darin beziehen sich auf einen Stand, der nicht mehr im Baum ist.

## Native Migration

Die Rust-Prüfungen benötigen keine Node-Toolchain:

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p hashline
mkdir -p /tmp/hashline-test-schemas
glib-compile-schemas --strict --targetdir=/tmp/hashline-test-schemas data
GSETTINGS_SCHEMA_DIR=/tmp/hashline-test-schemas GSETTINGS_BACKEND=memory cargo test -p hashline preferences::
```

Der explizite GTK-Integrationstest öffnet Testfenster. Er prüft Fensterwiederverwendung, Dateiwechsel, Menüaktionen, Themenzustand, Escape-Reihenfolge, Auswahl nach Loslassen und die Textschnittstelle. Zum isolierten Betrieb kann `gtk4-broadwayd :9` in einem separaten Terminal laufen:

```sh
dbus-run-session -- env GDK_BACKEND=broadway BROADWAY_DISPLAY=:9 GSETTINGS_SCHEMA_DIR=/tmp/hashline-test-schemas GSETTINGS_BACKEND=memory cargo test -p hashline native_ui -- --ignored --test-threads=1
```

Die tatsächliche AT-SPI-Anbindung benötigt X11 oder Wayland; Broadway unterstützt diesen GTK-Backendpfad nicht. Der folgende Test verwendet temporäre Dokumente, prüft Dokumentrolle, Unicode-Textoffsets, den zweiten Prozessaufruf mit demselben Fenster und den sichtbaren Mehrdatei-Hinweis und beendet seine Anwendung anschließend. Benötigt werden `python3-gi` und `gir1.2-atspi-2.0`:

```sh
dbus-run-session -- env GDK_BACKEND=x11 GTK_A11Y=atspi GSETTINGS_SCHEMA_DIR=/tmp/hashline-test-schemas GSETTINGS_BACKEND=memory /usr/bin/python3 tests/desktop/native_reader.py target/debug/hashline
```

Diese Tests sind Entwicklungsprüfungen. Vollständige Orca-Bedienung, visueller
Referenzvergleich sowie die Performance-Abnahme bleiben gesonderte Prüfungen.

## Linux-Paket

```sh
python3 packaging/build_deb.py
docker build -f packaging/Dockerfile -t hashline-package-test .
docker run --rm hashline-package-test
```

Die CI baut auf Ubuntu 26.04 und prüft das `.deb` in einem frischen Container.
Geprüft werden APT-Installation, Reinstallation, Entfernung, Cache-Trigger,
GSettings-Persistenz, Icon, MIME-Registrierung und Start über den installierten
Desktop-Eintrag samt Instanzübergabe. Standardzuordnungen müssen erhalten
bleiben. Details: [Installation](installation.md), Ergebnisse und verbleibende
Freigabepunkte: [M3](acceptance/M3.md).

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

## Integrationstests mit Fenster

Die Prüfungen oben brauchen kein Fenster. Die folgenden brauchen eines und
laufen deshalb nicht in `cargo test --workspace` mit. Beide setzen ein
kompiliertes Schema an einem eigenen Ort voraus:

```sh
cargo build -p hashline
mkdir -p /tmp/hashline-test-schemas
glib-compile-schemas --strict --targetdir=/tmp/hashline-test-schemas data
```

Der explizite GTK-Integrationstest öffnet Testfenster. Er prüft
Fensterwiederverwendung, Tabs samt Ctrl+W und Ctrl+Tab, Menüaktionen,
Themenzustand, Escape-Reihenfolge, Auswahl nach Loslassen, die Textschnittstelle,
den Leseanker über einen Neuladevorgang, die Leseposition über Zoom, schmalere
Spalte und Tabwechsel, Links in andere Dateien samt `#fragment`, Fußnoten,
GitHub-Anker und dass zwei Schreibweisen desselben Pfads einen Tab ergeben. Zum
isolierten Betrieb kann `gtk4-broadwayd :9` in einem separaten Terminal laufen:

```sh
dbus-run-session -- env GDK_BACKEND=broadway BROADWAY_DISPLAY=:9 GSETTINGS_SCHEMA_DIR=/tmp/hashline-test-schemas GSETTINGS_BACKEND=memory cargo test -p hashline -- --ignored --exact app::tests::native_ui
```

Broadway zeichnet ohne verbundenen Browser kaum Bilder, und alles, was erst ein
gezeichnetes Bild setzt — ein gesetzter Block, eine gemessene Höhe —, bleibt
dann aus; der Test scheitert daran, nicht am Leser. Verlässlich ist eine
kopflose GNOME Shell mit virtuellem Monitor, ebenfalls ohne Fenster auf dem
eigenen Bildschirm. Das Skript wartet auf den Socket und startet den Test
dagegen:

```sh
dbus-run-session -- sh -c '
  gnome-shell --headless --wayland --no-x11 --virtual-monitor 1280x900 \
      --wayland-display=hashline-test >/dev/null 2>&1 & shell=$!
  until [ -S "$XDG_RUNTIME_DIR/hashline-test" ]; do sleep 0.1; done
  WAYLAND_DISPLAY=hashline-test GDK_BACKEND=wayland \
      GSETTINGS_SCHEMA_DIR=/tmp/hashline-test-schemas GSETTINGS_BACKEND=memory \
      cargo test -p hashline -- --ignored --exact app::tests::native_ui
  status=$?; kill $shell; exit $status'
```

Der zweite Fenstertest misst die **längste zusammenhängende Hauptthread-Aufgabe**
und ist damit der Nachweis für das 16-ms-Budget aus SPEC Abschnitt 9. Er öffnet
`large.md`, `wide-table.md`, `long-line.md`, `large-code.md`, `many-blocks.md`
und `deep-list.md`, fährt jede Datei zwanzig Schritte hinunter und zehn wieder
hinauf und liest ab, was die längste Aufgabe gekostet hat. Er braucht die
erzeugten Fixtures (`python3 benchmarks/fixtures.py`):

```sh
env GSETTINGS_BACKEND=memory GSK_RENDERER=cairo cargo test --release -p hashline -- --ignored --exact app::tests::main_thread_work_stays_inside_the_frame_budget --nocapture
```

**Jeder dieser beiden Tests braucht einen eigenen Prozess.** GTK lässt sich pro
Prozess nur einmal initialisieren, und `cargo test` gibt jedem Test einen
eigenen Thread; zwei Fenstertests in einem Lauf scheitern deshalb am zweiten.
Deswegen steht oben je ein `--exact`.

Dasselbe Maß meldet das laufende Programm mit `HASHLINE_BENCH_MAIN_THREAD=1`
nach `stderr`, jede Zeile ein neues Maximum:

```text
HASHLINE_BENCH mainThreadMaxMs=11.02 task=measure-visible
```

Die tatsächliche AT-SPI-Anbindung benötigt X11 oder Wayland; Broadway unterstützt diesen GTK-Backendpfad nicht. Der folgende Test verwendet temporäre Dokumente, prüft Dokumentrolle, Unicode-Textoffsets, den zweiten Prozessaufruf mit demselben Fenster und den sichtbaren Mehrdatei-Hinweis und beendet seine Anwendung anschließend. Benötigt werden `python3-gi` und `gir1.2-atspi-2.0`:

```sh
dbus-run-session -- env GDK_BACKEND=x11 GTK_A11Y=atspi GSETTINGS_SCHEMA_DIR=/tmp/hashline-test-schemas GSETTINGS_BACKEND=memory /usr/bin/python3 tests/desktop/native_reader.py target/debug/hashline
```

Diese Tests sind Entwicklungsprüfungen. Vollständige Orca-Bedienung, visueller
Referenzvergleich sowie die Performance-Abnahme bleiben gesonderte Prüfungen.

## Live-Vorschau

Der Vertrag des Beobachters läuft ohne Fenster: jeder Test macht sich einen
eigenen Hauptkontext und treibt ihn selbst.

```sh
GSETTINGS_BACKEND=memory cargo test --release -p hashline --test reload
```

Geprüft werden Speichern über Rename, Löschen und Wiederanlegen, eine halb
geschriebene Datei, eine Folge von zwanzig Schreibvorgängen, eine Datei ohne
inhaltliche Änderung, das Lesen der offenen Datei — das ist kein Änderungsgrund,
sonst weckt sich der Prozess selbst — und die Zeit vom Schreiben bis zur
angeforderten Neuladung.

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

### Fixtures

Die Fixtures liegen unter `benchmarks/generated/` und stehen in `.gitignore`.
Eingecheckt ist stattdessen `benchmarks/fixtures/metadata.json`: Name, Größe und
SHA-256 jeder einzelnen Datei der bisherigen Reihen. `benchmarks/fixtures.py`
erzeugt die Dateien mit `benchmarks/generate.rs` — einer einzelnen Datei, die
`rustc` ohne Cargo, ohne Fremdkiste und ohne npm übersetzt — und vergleicht
anschließend jedes Byte mit dem Manifest. Erzeugt wird zuerst daneben; eine
Abweichung bricht ab, bevor eine vorhandene Fixture ersetzt wird. Damit bleiben
alte und neue Messreihen vergleichbar (SPEC.md, Abschnitt 9), und ein frischer
Klon kommt ohne die entfernte Node-Toolchain zu denselben Dokumenten. Der alte
Erzeuger `generate.mjs` ist entfernt; er brauchte `marked` und `jsdom` aus npm.

```sh
python3 benchmarks/fixtures.py           # erzeugen und prüfen
python3 benchmarks/fixtures.py --check   # nur prüfen
```

`generate.rs` bindet die PNG-Bilder der Bildfixtures über die Systembibliothek
`zlib` ein; ein frischer Klon braucht dafür `zlib1g-dev`. `run.py` erzeugt
fehlende Fixtures selbst, bevor es misst.

### Ein Lauf für alles: `bench` und `compare`

`benchmarks/run.py` fasst die Werkzeuge unten zu einer Messreihe zusammen.
`bench` misst nur Hashline, `compare` zusätzlich die Programme aus
`benchmarks/competitors.toml`. Beide teilen Messlogik, Budgets und Berichtsform;
sie unterscheiden sich allein in der Auswahl.

```sh
python3 benchmarks/run.py bench --quick
python3 benchmarks/run.py bench --only startup,memory --fixtures small,large
python3 benchmarks/run.py compare --provision --out benchmarks/results/<lauf>
```

- `--only` wählt Gruppen: `stages`, `startup`, `content`, `memory`, `idle`,
  `scroll`, `interaction`, `tabs`, `reload`, `stability`. `startup` meldet den ersten vom
  Compositor ausgegebenen Frame aus den Wayland-Marken und braucht keine
  Bildschirmaufnahme; `content` meldet den ersten Frame **mit Dokumenttext**.
  Nur `content` trägt das Ziel „Start bis lesbarer Text" aus
  [014](decisions/014-competitive-targets.md).
- `--fixtures`, `--viewers` und `--renderers` wählen Dokumente, Programme und
  GSK-Renderer. `--quick` bedeutet n=5 auf `small` und ist ausdrücklich **nicht**
  abnahmefähig.
- `memory` misst den Speicher in einem kurzen Fenster (`--sample-seconds`,
  voreingestellt 5 s); der PSS-Median steht nach einer Sekunde fest. `idle` misst
  die Leerlauf-CPU in dem 30-Sekunden-Fenster, das der Zielwert nennt
  (`--idle-seconds`), und läuft dafür nur fünfmal (`--idle-repetitions`). Beide
  Fenster dreißigmal zu wiederholen wäre über anderthalb Stunden reines Warten
  für einen einzigen Zielwert.
- `--repetitions` nimmt eine Zahl für alle Gruppen oder ein Budget je Gruppe:
  `--repetitions content=30,memory=8`. Eine nackte Zahl verschiebt dabei die
  Voreinstellung (`--repetitions 20,content=30`). Damit lässt sich eine feste
  Zeit dorthin lenken, wo die Streuung ist: ein p95 braucht eine lange Reihe,
  ein eingeschwungener PSS liegt beim achten Durchgang so wie beim dreißigsten.
  Unterhalb von n=30 meldet die betroffene Zelle `sufficient: false`, und der
  Lauf ist keine Abnahme — er bleibt aber eine gültige Messung.
- `--plan` schreibt die geplante Matrix, ohne ein Fenster zu öffnen.
- `--provision` beschafft Konkurrenzprogramme und Messwerkzeuge auf ihren
  festgeschriebenen Ständen nach `benchmarks/.provision/`; ins System wird nichts
  installiert.
- `--refresh-hz` gilt für den **gesamten** Lauf und prüft nur die bereits
  eingestellte Rate. 60 gegen 120 Hz sind zwei Läufe, keine Umschaltung im Lauf.

#### Eine Stunde, mit Konkurrenz

Der vollständige Vergleich dauert rund sechzehn Stunden. Die Ziele aus
[014](decisions/014-competitive-targets.md), die den Wettbewerb tragen, hängen
aber an zwei Gruppen: `content` für „Start bis lesbarer Text" und `memory` für
den PSS über die Dokumentgröße. Beides ist von außen messbar und für jedes
Programm gleich. Der folgende Lauf misst genau das, in ungefähr einer Stunde:

```sh
python3 benchmarks/run.py compare \
    --only content,memory --fixtures small,medium,large \
    --viewers hashline,viewmd,mdview,md-viewer --renderers cairo \
    --repetitions content=30,memory=8 --sample-seconds 3 \
    --out benchmarks/results/<lauf>
```

480 Messungen: `content` mit n=30 auf drei Größen gegen drei Konkurrenten,
`memory` mit n=8. Okular bleibt draußen, weil es als Referenz ausgewiesen ist
und kein Peer; Vulkan bleibt draußen, weil [015](decisions/015-renderer-choice.md)
den Renderer bereits entschieden hat.

Zwei Stellschrauben sind **keine**: `--hold` und `--quick`. ViewMD braucht bei
1 MiB 1,7 s bis zum ersten lesbaren Frame; ein kürzeres `--hold` macht daraus
„kein Ergebnis" und schmeichelt Hashline. `--quick` senkt die Fixtures auf
`small` und wirft damit genau die Achse weg, um die es geht.

Die Reihenfolge der Matrix läuft durchgangsweise, nicht programmweise. Nach
jedem vollen Durchgang haben alle Programme und Fixtures dieselbe Anzahl
Messungen, und `report.json` wird nach jeder einzelnen Messung geschrieben. Ein
Lauf, der nach einer Stunde mit Strg+C endet, ist deshalb kein verlorener Lauf,
sondern ein kürzerer.

Die Programme werden abwechselnd gemessen, ein Aufwärmdurchgang zählt nicht mit.
Jeder Lauf schreibt fortlaufend `report.json` — Fixture-Hashes, Programmversion
und -prüfsumme, angeforderter und beobachteter Renderer, Backend, Rohartefakte,
Budgetdefinitionen — und daneben `report.md`. Läufe unter 30 Wiederholungen sind
als nicht abnahmefähig ausgewiesen; fehlende, nicht unterstützte und
diagnostische Ergebnisse bestehen nie ein Budget. Die Grenze von 30
Wiederholungen gilt für Zeitreihen; `idle` belegt seinen Zielwert durch die
Länge des Fensters und braucht fünf. Gruppen ohne Zielwert — `stages` und
`startup` — müssen vollständig sein, tragen aber keine Abnahme; ein Lauf, der
nur aus ihnen besteht, ist nie eine. Okular steht als eigene
Referenz in einem eigenen Abschnitt des Berichts, nicht unter den Konkurrenten.

### Inhaltsnachweis

Ein Fenster erscheint, bevor es Text zeigt; eine Startzeit ohne Inhaltsnachweis
misst deshalb möglicherweise ein leeres Fenster. `benchmarks/content.py` nimmt
den Monitor über `org.gnome.Mutter.ScreenCast` und PipeWire auf und sucht in den
Einzelbildern per OCR nach Textstellen, die nur im geöffneten Dokument
vorkommen. `readableUpperMs` ist die Empfangszeit des ersten solchen Bildes:
eine konservative Obergrenze einschließlich Aufnahme- und Erkennungsweg, kein
Scanout-Zeitstempel. Vor dem Start wartet das Werkzeug, bis der Text **nicht**
mehr auf dem Bildschirm steht — das Fenster der vorigen Messung kann noch
gezeichnet sein —, und bricht ab, wenn er nicht verschwindet. Enthält danach ein
Bild vor dem Reiz den gesuchten Text, ist der Lauf ungültig statt schnell. Die
Erkennung läuft vor dem Start und nach dem Beenden der Anwendung, nie
währenddessen, und kann die Messung deshalb nicht ausbremsen.

Der Bildstrom ist schadensgesteuert: ein Monitor, auf dem sich nichts bewegt,
liefert überhaupt keine Bilder — gemessen null Bilder in vier ruhigen Sekunden,
während die Bühne weiter sechzigmal je Sekunde zeichnete. Ein Nachweis, dessen
Reiz auf ein stillstehendes Fenster trifft, könnte deshalb nicht unterscheiden,
ob der Text noch fehlte oder ob nur nichts ankam, und jedes Intervall wäre so
breit wie die Stille davor. Während einer Aufnahme läuft darum ein Herzschlag:
ein 16 × 16 Pixel großes Feld außerhalb des Zielfensters, das jedes Bild neu
gezeichnet wird. Es hält den Strom auf voller Aufnahmerate, wird vom
Fensterausschnitt weggeschnitten und aus dem gespeicherten Bild entfernt, damit
gleiche Schreibtische gleich bleiben. Ohne `Shell.Eval` — also außerhalb der
eigenen Sitzung — läuft er nicht; die Aufnahme hält das im Nachweis fest, statt
das Fehlen zu verschweigen.

Die Untergrenze des Intervalls ist nie älter als der Reiz. Der Text steht auf
dem Schirm, **weil** die Datei sich geändert hat oder der Betrachter gestartet
wurde; früher als sein Auslöser kann er nicht sichtbar sein. Ein älteres
Grundbild begrenzt darum nichts weiter.

Die Einzelbilder werden außerhalb des Aufnahmefadens verarbeitet. Ein
eingeblendeter PipeWire-Puffer ist einer, den der Compositor nicht nachfüllen
kann; wer darin komprimiert, hungert den Erzeuger aus und verliert etwa zwei von
fünf Bildern. Der Rückruf kopiert und gibt frei, das Packen läuft nebenläufig,
und gleiche Bilder werden über ihre Prüfsumme nur einmal abgelegt.

Die OCR ist auf einen genauen Paketstand festgelegt und wird nicht ins System
installiert:

```sh
python3 benchmarks/provision.py --tools
```

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

`compositor.py` liest die bestehende Bildwiederholrate ausschließlich aus und
ändert keine Monitorkonfiguration. `--refresh-hz` ist optional und prüft nur,
ob die angegebene Rate bereits aktiv ist; eine Abweichung bricht den Lauf ab.
Automatische 60/120-Hz-Wechsel sind auf Wunsch des Nutzers ausgeschlossen.

Der Reiz ist echte Zeigereingabe über `org.gnome.Mutter.RemoteDesktop`; in der
Anwendung ist nichts instrumentiert. `scroll-native.py` sucht das Fenster,
indem es an Kandidatenpunkten scrollt und die CPU-Zeit der Anwendung prüft —
ohne diesen Nachweis meldet der Lauf einen Fehler statt einer leeren, sauberen
Messung. Die Richtung kehrt regelmäßig um, damit ein Dokument nicht am Ende
liegt und Ruhe statt Scrollen gemessen wird.

```sh
python3 benchmarks/compositor.py ./target/release/hashline \
    benchmarks/generated/large.md --seconds 12 \
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

## Manuelle Freigabe

Vor einer v1-Freigabe bleiben Installation auf sauberer Zielumgebung,
tatsächlicher Dateimanager-Aufruf und Drag-and-drop, System-Clipboard und
Linköffnen sowie die vollständigen SPEC-Performancebudgets zu bestätigen. Die
Anwendungs-ID `de.kalendium.Hashline` ist aus der Spezifikation übernommen;
ihre Herausgeberbestätigung wird nicht aus einem erfolgreichen Build abgeleitet.

Die Abnahmeberichte und Messprotokolle der WebView-Fassung sind aus dem Baum
entfernt; ihre Befehle bezogen sich auf `tauri-driver`, `desktop.py` und
`npm run fixtures`, die es nicht mehr gibt. Sie liegen im Tag `webview-final`
und sind gegen den nativen Stand nicht vergleichbar.


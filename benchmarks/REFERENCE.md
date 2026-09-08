# Reproduzierbare Performance-Abnahme

Die automatischen Werte sind WebKitGTK-Release-Hilfsmessungen. Eine Abnahme der
sichtbaren Darstellung oder präsentierter Scrollframes lässt sich daraus nicht
ableiten. `REPORT.md` verknüpft tatsächlich ausgeführte Reihen mit ihren Rohdaten.

## Vorbereitung auf der Referenzmaschine

1. Installierten Release verwenden; Pfad, Paketversion und SHA-256 speichern.
   Während der Reihe keine Builds, anderen Benchmarks oder weiteren Hashline-
   Instanzen ausführen. Stromprofil, Lastzustand und thermischen Zustand notieren.
2. Integrierte Grafik, SSD, Distribution, CPU, RAM, WebKitGTK, Sitzungstyp,
   physische Displayfrequenz und Skalierung nachweisen. Der Runner speichert
   `lscpu`, `lspci`, `lsblk`, Paketinformationen und Mutter-Displayinformationen;
   fehlende Abfragen gelten als unbekannt. Eine diskrete GPU oder ein unbekannter
   Displaymodus darf nicht als Referenz ausgewiesen werden.
3. `npm run fixtures` erzeugt deterministische Markdown- und PNG-Fixtures.
   `benchmarks/generated/metadata.json` enthält Bytezahlen, SHA-256, Parser-DOM-
   Knoten sowie Bildgrößen, Pixelmaße und geschätzten RGBA-Dekodierspeicher.
4. Den WebDriver mit eigenen Test-Einstellungen starten. Keine persönliche
   laufende Instanz verwenden. Bei parallelen Desktop-Arbeiten kann
   `dbus-run-session` Instanzkonflikte verhindern; diesen Unterschied dokumentieren.

```sh
tauri-driver --port 4545 --native-port 4546 --native-driver /usr/bin/WebKitWebDriver
export HASHLINE_WEBDRIVER_URL=http://127.0.0.1:4545
python3 benchmarks/desktop.py /usr/bin/hashline --mode open --repetitions 30 --output benchmarks/results/reference-open.json
python3 benchmarks/desktop.py /usr/bin/hashline --mode startup --repetitions 30 --output benchmarks/results/reference-startup.json
python3 benchmarks/desktop.py /usr/bin/hashline --mode interaction --repetitions 30 --output benchmarks/results/reference-interaction.json
python3 benchmarks/desktop.py /usr/bin/hashline --mode scroll --repetitions 3 --refresh-hz 60 --output benchmarks/results/reference-scroll-60.json
python3 benchmarks/desktop.py /usr/bin/hashline --mode images --repetitions 3 --output benchmarks/results/reference-images.json
python3 benchmarks/desktop.py /usr/bin/hashline --mode open --fixtures long-line deep-list wide-table many-blocks large-code --repetitions 3 --output benchmarks/results/reference-special.json
python3 benchmarks/stability.py /usr/bin/hashline --output benchmarks/results/reference-stability.json
```

Ausgabedateien werden nicht überschrieben. Abgebrochene Reihen speichern ihre
Teilwerte und `completed: false`; nach jedem abgeschlossenen Lauf wird ein
atomarer Zwischenstand gespeichert, auch ein harter Prozessabbruch verliert
dadurch nicht die gesamte Reihe; weniger als 30 Läufe erfüllen keine reguläre
Zeitmessreihe. Alle Öffnungen wechseln zwischen zwei Pfaden, warten auf vollständige
Einfügung und unterscheiden erste Lesbarkeit vom vollständigen Hintergrundaufbau.
Startläufe erstellen und schließen jeweils eine eigene Instanz. Der Dateicache ist
unkontrolliert. Echte kalte Dateicache-Reihen benötigen ein separates Protokoll;
der Runner leert keine systemweiten Caches.

## Externe Darstellung und Compositor

Für **jede** reguläre Start-/Öffnungsreihe mindestens 30 Beobachtungen des
sichtbaren Dokumenttexts ergänzen, z. B. mit einer externen Kamera oder einem
geeigneten Plattform-Profiler. Aufnahmefrequenz, Zeitauflösung und Unsicherheit
angeben. Startmarke und erste tatsächlich lesbare Textdarstellung müssen in
derselben Zeitbasis liegen. Ein schwarzes/leeres Fenster, Screenshot-Abruf,
React-Commit oder ein doppelter Animation-Frame ist kein Präsentationsnachweis.
`driverToObservedFirstFrameMs` enthält Treiber-, CLI- bzw. Sessionkosten und darf
nicht als native Prozessstartzeit umbenannt werden. `hashline.native-main-to-frontend`
trennt den Eintritt in Rust-`main` von der initialisierten Frontend-Datei;
`hashline.native-main-to-first-frame` verwendet den gleichen nativen Zeitstempel
mit dem JS-Hilfsframe. Die Zuordnung nutzt Unix-Epochenzeit plus monotone Browserzeit
und setzt eine unveränderte Systemuhr voraus. Loaderkosten vor `main` und echte
Bildschirmpräsentation sind nicht enthalten. Diese beiden Marken sind nur in
**Startläufen** als Startmetrik sinnvoll, nicht bei späteren Dateiöffnungen.
Die native Startphase muss
zusätzlich vom Prozessstart bis zur WebView-Bereitschaft mit einem Plattformtrace
aufgezeichnet werden.

Scrollen über mindestens zehn Sekunden nach initialem Layout aufzeichnen. Aus
Compositor-/Profiler-Daten tatsächlich präsentierte Frame-Zeitpunkte der App
extrahieren, Rohtrace und Extraktionsverfahren speichern. Mindestens 99 % der
60-Hz-Intervalle müssen ≤16,7 ms sein; kein Stillstand >50 ms. Der Runner speichert
rAF-Intervalle nur als Diagnose. Bei verfügbarer 120-Hz-Hardware Displaymodus
umstellen, nachweisen und separat ausführen (`--refresh-hz 120`, Budget 8,37 ms).
Falls nicht verfügbar, Hardware-Nichtverfügbarkeit dokumentieren; keine simulierte
120-Hz-Reihe als Hardwareabnahme ausgeben.

Für Hauptthread-Aufgaben Browser-/Plattform-Profiling verwenden. Die
`hashline.max-render-task`-Marke umfasst Bereinigung, Einfügung und Indexierung
innerhalb der geplanten Aufgaben, aber keine späteren Browser-Layout-/Painttasks.
`maxEventLoopDelayMs` ist eine zusätzliche Timer-Verzögerung und kein Long-Task-
oder Compositor-Nachweis. Besonders große Einzelabsätze, Tabellen, Codeblöcke und
rohe HTML-Container müssen im Profiler einzeln untersucht werden.

## Speicher und Idle

Der Stabilitätsrunner wartet bei **jedem** Wechsel auf vollständige Darstellung,
anschließend auf Beruhigung. Er misst zunächst die kleine Datei, nach zehn
Aufwärmwechseln und nach weiteren 50 vollständigen Wechseln jeweils mindestens
30 Sekunden. Prozessgruppenwechsel oder unlesbare PSS-Werte bleiben ungültig.
Anfangsbudget ≤200 MiB PSS, nach 50 Wechseln höchstens +20 % gegenüber aufgewärmter
Baseline, CPU im Mittel <1 % eines Kerns. Eine einzelne 50-Wechsel-Reihe ersetzt
keinen Nachweis gegen dauerhaftes Wachstum bei langen Sitzungen.

Bildlast bleibt von Textlast getrennt: 100 verschiedene 256×256-PNGs sowie vier
verschiedene 2048×2048-PNGs. Der Runner scrollt jedes Bild in Sichtweite und
protokolliert tatsächliche `naturalWidth`/`naturalHeight` sowie Fehler. Der Runner erfasst dabei zusätzlich PSS der App-Prozessgruppe alle 250 ms und
weist das beobachtete Maximum aus. Kürzere Dekodierspitzen können zwischen den
Samples liegen und benötigen ergänzendes Allokationsprofiling; nur erfolgreiche
Dekodierung genügt keiner Speicherabnahme.

## GNOME-Compositor bei 60 und 120 Hz

`compositor.py` verbindet sich mit `org.gnome.Sysprof3.Profiler` der echten
GNOME-Shell-Sitzung und zeichnet deren Sysprof-Stream auf. Für die App wird die
separate Test-Bus-Adresse verwendet. Das Skript kann ausschließlich den Modus des
primären Monitors bei unveränderter Auflösung vorübergehend umstellen. Im
`finally`-Block wird die ursprüngliche Frequenz wiederhergestellt; Vorher-,
Während- und Nachher-Konfiguration sowie `restored` werden gespeichert.

```sh
python3 benchmarks/compositor.py /usr/bin/hashline --refresh-hz 60 --test-bus-file /tmp/hashline-perf-bus --output-prefix benchmarks/results/compositor-60
python3 benchmarks/compositor.py /usr/bin/hashline --refresh-hz 120 --test-bus-file /tmp/hashline-perf-bus --output-prefix benchmarks/results/compositor-120
sysprof-cat --no-callgraph --no-counters benchmarks/results/compositor-60.syscap > /tmp/compositor-60.txt
```

Pro Größe werden drei 12-Sekunden-Scrollläufe mit monotonen Zeitgrenzen erfasst.
Aus dem Inneren jedes Laufs ist ein 10-Sekunden-Fenster auszuwerten. Frame-
Präsentationsmarken müssen dem richtigen Display und den App-Aktualisierungen
zugeordnet werden; reine Paint-/Dispatch-Marken beweisen keine Präsentation.
Falls der Compositor diese Zuordnung nicht liefert, ist der Trace ein
Profilingnachweis und die Präsentationsabnahme bleibt offen.

## Trace-Auswertung

Komprimierte Rohtraces zunächst nach `/tmp` entpacken. Der Extraktor benötigt
`sysprof-cat`-Text, Scrollzeitfenster und Displayzustände aus derselben Aufzeichnung:

```sh
gzip -dc benchmarks/results/compositor-final-60.syscap.gz > /tmp/hashline-60.syscap
sysprof-cat --no-callgraph --no-counters /tmp/hashline-60.syscap > /tmp/hashline-60.txt
python3 benchmarks/analyze-compositor.py /tmp/hashline-60.txt --capture /tmp/hashline-60.syscap --scroll benchmarks/results/compositor-final-60-scroll.json --display benchmarks/results/compositor-final-60-display.json --output /tmp/hashline-60-analysis.json
```

Die Ausgabe enthält nachvollziehbare Präsentationszeitgrenzen, alle extrahierten
Marken und je ein zehnsekündiges Innenfenster. Sie setzt
`applicationFramesVerified: false`, da Mutter in diesen Marken keine App-ID
liefert. Eine gute Monitorquote allein beweist keine guten App-Frames.
Vor Beginn prüfen, dass das Benchmarkfenster auf dem richtigen Monitor sichtbar
ist und nicht durch parallele Tests verdeckt wird. Die 120-Hz-Einstellung des
primären Monitors reicht allein nicht, wenn das App-Fenster auf einem anderen
Monitor liegt oder seine Aktualisierung dort nicht nachgewiesen ist.

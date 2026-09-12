# Lokale Benchmark-Läufe

`run.py` schreibt standardmäßig nach `benchmarks/.local/runs/<Zeitstempel>/`.
Das gesamte `.local/` ist ignoriert. Rohdaten bleiben dort; `results/` enthält
veröffentlichte, verkleinerte Berichte.

```sh
# Matrix und geschätzte Gesamtdauer, ohne Programme zu starten
python3 benchmarks/run.py compare --only content,memory --fixtures small,medium,large --plan

# Nebenbei, mit eigenem Compositor und maximal einem Abend pro Abschnitt
python3 benchmarks/run.py compare --only content,memory --fixtures small,medium,large \
  --session nested-headless --time-budget 60m
python3 benchmarks/run.py list
python3 benchmarks/run.py --resume <Name> --time-budget 60m

# Bewusst veröffentlichen; bestehende Veröffentlichungen werden nie überschrieben
python3 benchmarks/run.py promote <Name> --as <Veröffentlichungsname>
python3 benchmarks/run.py check-results

# Zunächst nur Größe und Auswahl anzeigen; --delete entfernt die benannten Läufe
python3 benchmarks/run.py prune <Name>
python3 benchmarks/run.py prune <Name> --delete
```

Jede fertige Messzeile enthält `startedAt`, `durationSeconds`, `sessionKind`
und gegebenenfalls `attempts`. `report.json` wird atomar ersetzt und enthält
auch während einer Messung sekündlich ein `progress`-Objekt mit `done`, `total`,
`elapsedSeconds` und `estimatedRemainingSeconds`. Dieselben Zahlen erscheinen
auf stdout; an einem Terminal wird die Zeile ersetzt. Warmups zählen zum
Arbeitsfortschritt, aber nicht zur statistischen Auswertung.

Die Schätzung verwendet ab zwei Beobachtungen den Mittelwert je
(Gruppe, Programm, Fixture) aus `.local/durations.json`. Zuvor gelten die
Startwerte aus `timing.py`. Für dort nicht gemessene Gruppen
stehen ausdrücklich abgeleitete Startwerte in `timing.py`. Es sind Schätzungen
inklusive Messapparat und OCR, keine zugesicherten Laufzeiten.

Ein Zeitbudget startet nur einen weiteren vollständigen Durchgang, wenn dessen
Schätzung noch hineinpasst. Ein bereits angefangener Durchgang wird beendet;
daher kann eine unterschätzte Dauer das Budget überschreiten. `stoppedBy` hält
`time-budget`, `interrupted` oder `error` fest. Unterschiedliche ausdrücklich
gewählte Wiederholungszahlen je Gruppe bleiben erhalten. Ein Resume vervollständigt
zuerst einen unterbrochenen Durchgang, überspringt sämtliche vorhandenen Zeilen
(auch fehlgeschlagene) und prüft Matrix, Fixture- und Binärdatei-Prüfsummen.
Ein Lock verhindert gleichzeitiges Fortsetzen, Löschen oder Veröffentlichen.
Historische Läufe ohne Matrix können aus ihren vorhandenen Zellen rekonstruiert
werden; ihre fehlenden Zeit- und Bildinformationen werden nicht nachträglich erfunden.

Der Wachhund beendet einen Messprozess samt Nachkommen nach dem Sechsfachen
seiner geschätzten Dauer, mindestens nach 30 Sekunden. `--watchdog-factor`
ändert den Faktor. Alle Wiederholungsversuche teilen diese Obergrenze. Frühe Setup-Fehler und
Timeouts werden nicht als normale Zellkosten in die Schätzung eingelernt.

## Bildnachweise

Aufnahme und Erkennung sind getrennt. Während der Interaktionsfolge werden
Snapshots gesammelt; OCR beginnt erst nach dem Ende des getesteten Programms.
Die Erkennung läuft mit `nice -n 10`, einem Tesseract-Thread und `--psm 6`;
erst bei einem Fehlschlag folgt `--psm 11`. `ocrPsm` nennt den erfolgreichen Modus.
Identische Fensterbilder teilen ihr OCR-Ergebnis.

Die Aufnahme behält sämtliche vom PipeWire-Stream gelieferten Bilder. Erst
anschließend wird auf das verifizierte Programmfenster zugeschnitten und nur
außerhalb von ±500 ms um den Stimulus entdoppelt. Vollbildänderungen außerhalb
des Fensters zählen dabei nicht. Eine Aufnahmequelle, die selbst keine Bilder
liefert, lässt sich dadurch nicht in eine höhere zeitliche Auflösung verwandeln:
es werden keine Frames oder Zeitstempel interpoliert.

`readableLowerMs` bezeichnet den Empfang des letzten Bildes ohne Text,
`readableUpperMs` den Empfang des ersten Bildes mit Text; `proofGapMs` ist ihr
Abstand. Das sind Empfangsgrenzen mit Aufnahmelatenz, keine Monitor-Präsentationszeiten.
Ein Intervall über 40 ms ist `diagnostic`, und `proofResolutionValid: false`
verhindert jede Budgetabnahme dieser Bildmessung. Die Grenze sind zwei
60-Hz-Intervalle (33,33 ms) plus ein weiteres als Aufnahme-Spielraum: der
Aufnahmeweg liefert bestenfalls ein Bild je Bildwiederholung und überspringt
unter Last jede zweite; der gemessene Abstand aufeinanderfolgender Bilder
reichte über 54 Intervalle bis 36,57 ms. Ohne diesen Spielraum wäre rund ein
Drittel einwandfreier Nachweise allein am Messapparat gescheitert.
Die komprimierten Fensterbilder und Zeitstempel liegen in `capture.json` plus
`frames/*.ppm.z`. Erneute OCR ohne erneute Messung:

```sh
python3 benchmarks/content.py <Pfad-zur-capture.json> --out /tmp/neue-auswertung
```

`proofFailure.kind` unterscheidet `no-frames`, `window-never-mapped`,
`program-exited`, `text-not-recognized`, `window-geometry-unavailable`,
`no-baseline`, `preexisting-text` und `capture-error`. `unrenderable` bedeutet: innerhalb des
Beobachtungsfensters weder ein Anwendungs-Frame-Callback noch eine Präsentation;
das ist keine Behauptung über beliebig lange Wartezeiten. Es gibt dafür keine
Wiederholung. Nur `missing` mit `text-not-recognized` und vorhandenen Bildern
wird höchstens zweimal neu gemessen. Jeder Versuch hat eigene Rohdaten.

## Sitzung und Messumfang

`stages`, `memory`, `idle`, `stability` und `mainthread` brauchen keine
Monitorbilder. Programme mit einer Oberfläche brauchen weiterhin einen
Wayland-Compositor. `mainthread` liest die vom Programm protokollierten Aufgaben
bei Start und Dateiübergabe; Tastatur- und Menüaufgaben erfasst zusätzlich
`interaction`.

`content`, `interaction`, `tabs`, `reload` und `scroll` brauchen Bildnachweise.
Auf dem physischen Desktop prüft ein Eingabe-Ruhefenster vorab
`desktopUnattendedVerified`; ein Fehlschlag liefert begründete fehlende Messungen.
Der Ruhecheck garantiert keine spätere Abwesenheit von Eingaben oder Hintergrundlast.

`--session nested-headless` startet GNOME Shell auf einem privaten D-Bus mit
festem virtuellem Monitor (`--resolution 1920x1080`). WAYLAND_DISPLAY wird in
die bestehende Anwendungsisolation übernommen. Die private Shell erlaubt das
Auslesen der tatsächlichen Fensterrechtecke; die Host-Shell wird nicht umkonfiguriert.
Voraussetzungen: GNOME Shell mit `--headless`, `--virtual-monitor` und
`--unsafe-mode`, D-Bus, PipeWire/GStreamer, PyGObject und Tesseract. Die lokale
Implementierung wurde mit GNOME Shell 50.1 geprüft. Die virtuelle Tastatur
wird vor dem ersten Stimulus angelegt, damit dessen Eingaben nicht verloren gehen. Hintergrundlast auf CPU/GPU
bleibt auch hier eine Messbedingung.

`scroll` und der präsentationsbasierte `startup` bleiben in dieser Sitzung
`unsupported`: Headless-Präsentationsmarken weisen kein Monitorbudget nach.
Jede Zeile trägt deshalb `bare-metal` oder `nested-headless`. Auf einem physischen
Desktop nutzt der Fensterzuschnitt die vorhandene Shell-Auskunft oder AT-SPI;
ist keine verlässliche Geometrie verfügbar, wird kein Vollbild als Fenster ausgegeben.

## Veröffentlichungsgrenze

`promote` erzeugt `report.json`, `report.md`, höchstens ein PNG je Zelle und
`MANIFEST.json` mit Dateigrößen und SHA-256-Prüfsummen des lokalen Vollbestands.
Frames-Listen, Rohmessungen und lokale Beweisbildpfade werden entfernt. Die
Schranken und Fehler der einzelnen Interaktionsnachweise bleiben erhalten. Zellen
ohne verfügbares Beweisbild behalten einen leeren Bildverweis.

Leere, nur geplante oder reine Warmup-Läufe, unvollständige Läufe ohne `stoppedBy` sowie mehr
als **20 MiB pro Veröffentlichung** werden verweigert. `--force` übergeht diese
Verweigerung ausdrücklich; die Größe wird immer ausgegeben. Die CI-Grenze bleibt
auch bei `--force` wirksam: irgendein `raw/` oder eine Veröffentlichung über
20 MiB unter `results/` lässt `check-results` fehlschlagen. `prune` löscht nur
explizit benannte direkte Unterverzeichnisse der lokalen Laufablage.

Die Beispiele und Messverträge werden durch `test_suite.py` und
`test_lifecycle.py` geprüft. Die Integrationstests mit GNOME Shell lieferten
Fensterbilder für Content, Suche öffnen, Menü öffnen, Dateiübergabe, Tabwechsel
und Reload sowie Speicher- und Hauptthreadwerte. Die Suchziel-OCR blieb in der
kleinen Testfixture `missing`; Wiederholungen und Beweisdaten wurden protokolliert.
Große Bildintervalle bleiben ausdrücklich diagnostisch, auch wenn die
Zeitobergrenze selbst ein Ziel unterschreitet. Der getestete ViewMD-Start mit
10 MiB erhielt `unrenderable` ohne Wiederholung.

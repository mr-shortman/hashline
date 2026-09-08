# Hashline — Renderer- und Performancebericht

**Arbeit auf Nutzerwunsch angehalten.** Aktueller Implementierungs-, Build- und
Messstatus einschließlich der noch ausstehenden nativen Prüfung des neuesten
Builds: [Übergabedokument](../docs/performance-progress.md).

Stand: 8. September 2026. **Keine M0-/v1-Freigabe.** Der abbrechbare
Abschnittsrenderer ist implementiert und funktional geprüft. Die erste Messserie
zeigt deutlich frühere erste Lesbarkeit und schnelle Suche. Vollständiger Aufbau,
Start, einzelne Hauptthread-Unterbrechungen und Speicher erfüllen weiterhin nicht
alle Budgets. Die integrierte-Grafik-Referenz und der externe Nachweis tatsächlicher
Textpräsentation fehlen. Ziele wurden nicht geändert.

Die [Rendererentscheidung](../docs/decisions/004-rendering-gate.md) beschreibt
Semantik, Abbruch und bekannte Grenzen. Der [vorherige Releasebericht](BASELINE.md)
mit zusammenhängender Bereinigung bleibt als Ausgangsmessung erhalten.

## Umgebung und Vergleichbarkeit

Alle ausgeführten Reihen verwenden die Entwicklungsmaschine: Ubuntu 26.04,
Ryzen 9 9900X, rund 30 GiB RAM, SSD, **diskrete RTX 3060**, Wayland und
WebKitGTK 2.52.6. Build-, Hardware- und Fixture-Provenienz steht in jeder Rohdatei;
[Fixture-Metadaten](results/section-fixtures.json) enthalten auch deterministische
Bildlasten. Getestet wurde ein eingefrorenes Release-Binary, kein Vite-Server.
Die App läuft auf einem eigenen D-Bus mit isolierten Testeinstellungen. Das Paket
ist auf dieser Maschine nicht installiert; der Betriebssystem-Dateicache bleibt
unkontrolliert. Diese Reihen sind keine Referenzabnahme.

Die erste vollständige Abschnittsserie (`sectioned-*`) gehört ausschließlich zu
Binary-SHA-256 `ca423fcc1a5ecfb5ccf39e8b47e2a6a84dbb27486d9d76b245db99b4fc0e918f`.
Sie umfasst je 30 Öffnungen aller drei Dateigrößen, 30 eigene Prozessstarts,
je 30 Interaktionsläufe für kleine/mittlere Dateien, je drei Scroll- und Bildläufe,
Sonderfälle sowie eine Speicher-/Idle-Reihe. Spätere Varianten werden separat
aufgeführt; ihre Werte werden nicht mit dieser Reihe vermischt.

## Zweite Optimierung: Öffnungszeit und vollständiger Aufbau

Die Folgearbeit adressiert gezielt große Öffnungen und den vollständigen Aufbau.
Der Renderer verwendet unveränderte, bildfreie Abschnitte samt Suchindizes weiter,
ohne ihre verbundenen DOM-Knoten umzuhängen. Für geänderten Inhalt reduziert ein
18-ms-Aufgabenbudget mit Window-Nachrichten die Wartezeit zwischen Einfügungen;
auch die Entfernung alter Abschnitte ist gebündelt und abbrechbar. Selektive
Attributnachprüfung spart einen zweiten vollständigen Elementdurchlauf nach
DOMPurify. Die Sicherheitsbereinigung selbst bleibt für jedes neue Fragment aktiv.

Im Parser werden eine Rendererinstanz je Dokument, zusätzliche sichere
Präfixprüfungen und höchstens acht wiederverwendete Listen-Bullet-Regulärausdrücke
verwendet. Der Worker bleibt bis zu 30 Sekunden nach dem letzten Ergebnis bereit;
eine Sekunde hatte ihn schon während des DOM-Aufbaus wieder beendet. Die native
Übertragung liefert UTF-8-Markdown als Binärdaten mit kurzem JSON-Metadatenkopf.
Abbruch, Ressourcenberechtigungen und Fehlererholung bleiben erhalten.
Ein zusätzlicher, streng begrenzter Cache vermeidet erneute Block-/Inline-Analyse
wiederkehrender Listen ohne HTML-, Referenz- oder Taskkontext. Er gilt nur innerhalb
einer Lexerinstanz; einzigartige Inhalte profitieren davon nicht. Die vollständige
Markdown-Grammatik bleibt als Rückfallpfad aktiv.

Die neuen Reihen unterscheiden ausdrücklich **geänderten Inhalt**
(`--distinct-content`, alle Abschnittsüberschriften wechseln) von identischem
Inhalt unter wechselnden Dateipfaden. Hashes beider Fixture-Varianten sowie Zahl
wiederverwendeter Abschnitte stehen in jeder Rohdatei. Nur identische Dateien zu
messen würde den Aufwand eines tatsächlichen Neuaufbaus verdecken.

Diagnostische Zwischenstände bleiben als `optimization-1-*` bis
`optimization-8-*` erhalten und sind keine Abnahme. Insbesondere wurde der
Inline-Token-Cache aus Variante 6 mangels hinreichenden Vorteils wieder entfernt.
Die Variante-7-Datei ohne `retry` enthält wegen eines beendeten Testtreibers keine
Beobachtungen. Variante 8 wurde nach drei Beobachtungen zugunsten einer Korrektur
der UTF-8-BOM-Behandlung beendet. Ihre unvollständige Reihe wird nicht als n=30
ausgewiesen. Die abschließenden Ergebnisse werden separat geführt.
Auch `optimized-distinct-open.json` ist ein Zwischenstand: je 30 kleine/mittlere
Öffnungen, aber nur 17 große. Der längere Lauf zeigte weitere Überschreitungen
von zwei Sekunden und wurde für die Listenoptimierung beendet. Er ist keine
bestandene Öffnungsabnahme. Die Variante `optimized-v2` enthält außerdem den
DOM-basierten Ausschluss bildhaltiger Abschnitte einschließlich HTMLs historischer
`<image>`-Schreibweise von der Wiederverwendung.

## Vorherige Variante und Messstatus

Die abschließend vermessene Renderer-Variante ist als
SHA-256 `e09a02b9d346a5334354c560f006daafb2083e45dd8cca41450903ebaee98b19`
eingefroren. Gegenüber der ersten Abschnittsserie entfernt sie alte Abschnitte
vom Ende her in abbrechbaren Aufgaben, lädt nur benötigte Highlight-Grammatiken
und lässt nachweislich geschlossene HTML-Tokens an Abschnittsgrenzen zu.

| Finale Messung                         | Ergebnis und Rohdaten                                                                                                                                                |
| -------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Start, n=30                            | Rust-main → Text-Hilfsframe: Median 1.219,1 ms, p95 1.270,0 ms; Frontend bereit: 731,6 / 776,3 ms. [Start](results/final-startup.json)                               |
| Suche, 1 MiB, n=30                     | Ergebnisse einschließlich Bündelung: 64,5 / 69 ms; Suchleiste öffnen: 29 / 33 ms. [Interaktion](results/final-interaction.json)                                      |
| Menü, 1 MiB, n=30                      | 32 / 36 ms bis Hilfsframe. Kleine Datei: 16 / 33 ms. [Interaktion](results/final-interaction.json)                                                                   |
| Bilder, je n=3                         | PSS-Spitzen viele Bilder: 264,3 / 314,5 / 364,0 MiB; große Bilder: 576,3 / 607,5 / 766,5 MiB. Alle Pixelmaße geprüft. [Bildlast](results/final-images-complete.json) |
| Nach Bildlast zurück zur kleinen Datei | Letztes PSS-Sample nach 30 s: 451,2 MiB; CPU 0,167 % eines Kerns. [Bildlast](results/final-images-complete.json)                                                     |
| Speicher/Idle                          | Initial 237,74 MiB; aufgewärmt 240,21 MiB; nach 50 Wechseln 377,22 MiB (**+57,04 %**). CPU 0,400 / 0,233 / 0,233 %. [Stabilität](results/final-stability.json)       |
| Native Verträge                        | Alle sechs Abschnittsverträge und sieben Smoke-Schritte bestanden. [Abschnittsverträge](results/final-contracts-settled.json)                                        |

Nach dem Einfrieren dieser Messreihen wurde zusätzlich verhindert, dass Links
aus gerade entfernten alten Abschnitten gegen die neue Datei aufgelöst werden.
Die 16 Browser-Bedienungstests einschließlich eines gezielten Übergangsklicks
und der Auswahl aller 3.000 Abschnittsüberschriften bestehen. Ein eigener
Release-Build mit SHA-256
`5fc27e95a7d6af39d2f2d52cba7ada1c8841cf3d0f808ba90ca1e868d1c00e11`
besteht ebenfalls alle sechs [nativen Abschnittsverträge](results/verified-contracts.json)
und sieben [Desktop-Smoke-Schritte](results/verified-smoke.json).
Das [Quellmanifest](results/verified-source.json) blieb während des Builds unverändert; die obigen
Zeitreihen behalten ausdrücklich ihren ursprünglichen Binary-Hash.

Die finale Speicherreihe überschreitet sowohl das absolute Budget als auch die
relative 20-%-Grenze. Sie ist schlechter als die frühere Abschnittsreihe und darf
nicht durch deren günstigere Werte ersetzt werden. Ursache und Reproduzierbarkeit
auf einer ungestörten Maschine sind noch zu untersuchen. Die Bildlast-Rückkehr
zeigt ebenfalls hohen zurückbehaltenen Speicher.

**Störbedingungen:** Während dieser Nachmessungen liefen weitere Hashline-
Desktop-Abnahmetests in derselben grafischen Sitzung. Eine Compositor-Aufnahme
zeigte, dass ein fremdes Testfenster das Benchmarkfenster überdeckte; Fokus wechselte.
Der eigene D-Bus verhindert Single-Instance-Verwechslungen, isoliert jedoch weder
GPU/Compositor noch CPU-Last. Die Zeiten sind deshalb lokale Vergleichsdiagnosen
mit vollständiger Wiederholungszahl, keine kontrollierte Abnahme.
[Statusprotokoll](results/final-run-status.json) dokumentiert außerdem
Unterbrechungen und den anfänglich fehlgeschlagenen Testversuch.

Der erste finale native Auswahlversuch schlug fehl. Diagnose und erneuter Lauf
zeigten vollständige Auswahl; der Test wartete vorher zwischen dem Schließen der
Suchleiste und dem folgenden Tastaturauftrag nicht auf den React-Zustandswechsel.
Der korrigierte Ablauf wartet auf die geschlossene Suchleiste; alle sechs Schritte
bestehen. Die Produkt-Auswahllogik wurde dafür nicht geändert.

### Finale Öffnungsreihe

[90 vollständige Öffnungen](results/final-open.json), n=30 je Größe,
Median / p95 in ms, gleicher eingefrorener Build wie oben:

| Messgröße                                  |       100 KiB |         1 MiB |            10 MiB |
| ------------------------------------------ | ------------: | ------------: | ----------------: |
| Auftrag bis erster Hilfsframe              |       42 / 58 |     268 / 300 |   2.478,5 / 2.550 |
| CLI bis Treiberbeobachtung des Hilfsframes | 106,5 / 124,5 | 331,8 / 365,8 | 2.556,9 / 2.627,3 |
| Auftrag bis vollständige DOM-Einfügung     |   102,5 / 119 | 1.198 / 1.372 | 26.485,5 / 32.559 |
| Worker-Parsing                             |       13 / 16 |   163,5 / 174 |     1.518 / 1.565 |
| Synchrone Vorbereitung                     |       10 / 13 |       13 / 17 |           23 / 27 |
| Gesamtdauer Entfernung alter Abschnitte    |        6 / 19 |       58 / 73 |         726 / 778 |
| Größte instrumentierte Entfernungsaufgabe  |         4 / 5 |       6,5 / 7 |             7 / 7 |
| Größte instrumentierte Renderaufgabe       |       10 / 11 |       16 / 25 |           48 / 62 |
| Größte Timer-Verzögerung je Öffnung        |       11 / 28 |       20 / 31 |       118 / 1.172 |

Gegenüber dem zusammenhängenden Ausgangsrenderer sinkt die 1-MiB-CLI-Hilfszeit
von 1.486,5 auf 365,8 ms p95 (rund 75 %). Der 10-MiB-Ausgangswert von 12.736,8 ms
war ein Einzelwert; die finale Serie erreicht 2.627,3 ms p95, überschreitet aber
das 2-s-Ziel. Der maximale große Hilfswert beträgt 2.631,2 ms mit Treiberkosten.

Gegenüber der **ersten Abschnittsvariante** ist der wiederholte große Wechsel
etwas langsamer. Dafür fällt die synchrone Vorbereitung von 538 auf 27 ms p95;
das Entfernen wird auf Aufgaben von maximal 8 ms verteilt. Diese Verbesserung
begrenzt nicht automatisch Layout, Garbage Collection oder andere Tasks:
Renderaufgaben erreichen bei 10 MiB noch 67 ms, Timer-Verzögerungen 1.486 ms
als Maxima. Im allerersten kleinen Lauf beträgt die Vorbereitung 210 ms, was
in p95 der 30 Öffnungen nicht sichtbar wird. Auch deshalb bleibt die Startreihe
separat und der Hauptthread-Vertrag offen.

### Vergleich der Entfernung alter DOM-Abschnitte

Die [sequenzielle Stichprobe](results/retirement-sequential-probe.json), je zwei
Öffnungen von 1 und 10 MiB, reduziert beim wiederholten großen Dokument die
synchrone Vorbereitung auf 23 ms. Das Entfernen benötigt insgesamt 619 ms,
keine instrumentierte Einzelaufgabe über 7 ms. Trotzdem beträgt der Hilfsframe
in diesem Lauf 7.702 ms: 7.044 ms vergehen bereits bis zur Parserübergabe bei
nur 1.425 ms Worker-Parsing. Die größte Timer-Verzögerung beträgt 3.705 ms.
Daraus lässt sich keine erfolgreiche Ende-zu-Ende-Optimierung ableiten.

Drei verworfene Varianten versteckten beziehungsweise verschoben den alten
Renderbaum vor der schrittweisen Entfernung. Sie erzeugten beim zweiten großen
Dokument 1.991 bis 2.470 ms synchrone Vorbereitung:
[display:none](results/retirement-probe.json),
[content-visibility:hidden](results/retirement-skipped-probe.json),
[opacity](results/retirement-opacity-probe.json). Diese Stichproben begründen die
Wahl der abbrechbaren Entfernung, ersetzen aber keine reguläre Öffnungsreihe.

### Compositor-Traces bei 60 und 120 Hz

Je drei 12-s-Scrollläufe für kleine und mittlere Dateien wurden in GNOME Sysprof
aufgezeichnet. Der primäre G27FC-Monitor an DP-3 lief nachweislich mit 1920×1080
bei 60,000 beziehungsweise 119,982 Hz, Skalierung 1. Beide Aufzeichnungen speichern
den Ausgangsmodus, den Messmodus und die erfolgreiche Wiederherstellung.

- 60 Hz: [Displayzustände](results/compositor-final-60-display.json),
  [Scrollzeitfenster](results/compositor-final-60-scroll.json),
  [Rohtrace, gzip](results/compositor-final-60.syscap.gz),
  [Extraktion](results/compositor-final-60-analysis.json).
- 120 Hz: [Displayzustände](results/compositor-final-120-display.json),
  [Scrollzeitfenster](results/compositor-final-120-scroll.json),
  [Rohtrace, gzip](results/compositor-final-120.syscap.gz),
  [Extraktion](results/compositor-final-120-analysis.json).

`analyze-compositor.py` extrahiert Monitor-Präsentationsintervalle aus den
`Clutter::FrameClock::presented()`-Marken. Die gemeldete Präsentationsverzögerung
wird innerhalb einer Trace-Spanne erfasst; die Auswertung behält deshalb untere
und obere Zeitgrenzen bei. Herleitung:
[Mutter-Implementierung](https://github.com/GNOME/mutter/blob/50.4/clutter/clutter/clutter-frame-clock.c).

**Keine Scrollabnahme:** Die Marken enthalten keine App-Surface-/Frame-ID,
sondern sämtliche Clients des Monitors. Das Testfenster war während der
120-Hz-Reihe nicht fokussiert; seine rAF-Takte lagen überwiegend nahe 60 Hz.
Wegen überdeckender paralleler Fenster ist die Zuordnung zur sichtbaren App
nicht gesichert. Lange Pausen in den DP-3-Marken dürfen daher nicht als
Hashline-Scrollstillstände interpretiert werden. Die Rohtraces sind ein
Profilingnachweis; die 99-%-Anforderung bleibt offen.

## Erste vollständige Abschnittsserie

Zeiten in ms, Median / p95, jeweils 30 Wiederholungen. p95 verwendet den
Nearest-Rank-Wert (29. sortierter Wert bei n=30). Der erste Lauf bleibt enthalten.
„Hilfsframe“ bedeutet zwei `requestAnimationFrame`-Callbacks nach dem Einfügen;
das ist kein Beweis einer tatsächlichen Bildschirmpräsentation.

| Messgröße                                                |       100 KiB |         1 MiB |            10 MiB |
| -------------------------------------------------------- | ------------: | ------------: | ----------------: |
| Auftrag bis erster Hilfsframe                            |       37 / 52 |   257,5 / 276 |     2.202 / 2.302 |
| CLI bis Treiberbeobachtung des Hilfsframes               | 105,9 / 116,9 | 319,8 / 342,7 | 2.273,7 / 2.386,3 |
| Auftrag bis vollständige DOM-Einfügung                   |     101 / 116 | 1.147 / 1.214 |   29.707 / 38.558 |
| Synchrone Vorbereitung einschließlich erzwungenem Layout |       16 / 19 |       60 / 65 |         488 / 538 |
| Größte instrumentierte Renderaufgabe                     |       10 / 12 |       17 / 24 |         54,5 / 69 |
| Größte Timer-Verzögerung pro Öffnung                     |       12 / 18 |     57,5 / 66 |     515,5 / 2.020 |

[Öffnungsrohdaten](results/sectioned-open.json). Die CLI-Hilfszeit für 1 MiB
sinkt gegenüber dem bisherigen p95 von 1.486,5 ms auf 342,7 ms (rund 77 %).
Bei 10 MiB stehen einem früheren **Einzelwert** von 12.736,8 ms nun 30 Werte mit
p95 2.386,3 ms gegenüber; das ist kein gleichwertiger Perzentilvergleich.
Der erste 10-MiB-Lauf nach der mittleren Datei erreicht den Hilfsframe in 1.696 ms;
wiederholte große Wechsel sind langsamer. Das 2-s-Ziel ist nicht bestanden.

Die gesamten Bereinigungs-/Einfügezeiten der mittleren Datei betragen im Median
349,5 / 217 ms, bei 10 MiB 3.625,5 / 2.177,5 ms. Diese Arbeit ist verteilt,
aber der vollständige Hintergrundaufbau ist bei 10 MiB weiterhin sehr langsam.
Vollständige Suchergebnisse werden erst nach diesem Aufbau freigegeben.
Timer-Verzögerungen sind Diagnosen, keine direkten Long-Task-Messungen; auch
Layout, Paint, Garbage Collection und Treibereinfluss können darin auftreten.

## Start und Interaktion

[Startrohdaten](results/sectioned-startup.json), n=30 eigene Prozesse, kleine Datei:

| Messgröße                                            |  Median / p95, ms |
| ---------------------------------------------------- | ----------------: |
| Eintritt in Rust-main bis Frontend-Modul bereit      |       730 / 821,6 |
| Rust-main bis erster Text-Hilfsframe                 | 1.214,5 / 1.304,6 |
| WebDriver-Sessionauftrag bis beobachteter Hilfsframe |   1.262,1 / 1.347 |

Damit ist bereits der Hilfswert deutlich über dem 500-ms-Startbudget. Die native
Marke berücksichtigt keine Loaderkosten vor `main`; die Zuordnung verwendet
Unix-Epochenzeit und setzt voraus, dass die Systemuhr während des Laufs nicht springt.
Die Frontend-Marke ist kein nativer WebView-Bereitschaftstrace.

[Interaktionsrohdaten](results/sectioned-interaction.json), n=30 pro Größe,
Zeit ab DOM-Ereignis einschließlich Suchbündelung bis Hilfsframe:

| Aktion         | 100 KiB, Median / p95, ms | 1 MiB, Median / p95, ms |
| -------------- | ------------------------: | ----------------------: |
| Suche öffnen   |            siehe Rohdaten |                 28 / 38 |
| Menü öffnen    |                   16 / 29 |                 34 / 49 |
| Suchergebnisse |                 48,5 / 62 |               60,5 / 73 |

Suche und Menü liegen in dieser lokalen Hilfsmessung innerhalb ihrer jeweiligen
150-/50-ms-Grenzen. Eine zwischenzeitliche Variante zeichnete alle Such-Ranges
und erzwang damit Layout im gesamten Dokument; die aktuelle Variante zeichnet
sichtbare und aktive Treffer. Die abgebrochenen Dateien `sections-interaction*`
sind ausschließlich Entwicklungsdiagnosen, keine bestandenen Messreihen.

## Scrollen und Bildlast

[Scrollrohdaten](results/sectioned-scroll.json): je drei Läufe über zehn Sekunden.
Bei 100 KiB liegen im Median 93,1 % der rAF-Intervalle innerhalb 16,7 ms,
bei 1 MiB 93,9 %. Größte beobachtete Lücke: 48 bzw. 264 ms. Der JS-Zeitstempel
ist in WebKit auf ungefähr 1 ms quantisiert; das beeinflusst die 16,7-ms-Grenze.
Diese Werte beweisen weder die 99-%-Anforderung noch präsentierte Frames.
Die Displayfrequenz war in dieser ersten Reihe nicht nachgewiesen.

[Bildrohdaten](results/sectioned-images.json): je drei vollständige Öffnungs- und
Scrollläufe. Alle 100 verschiedenen 256×256-PNGs und alle vier verschiedenen
2048×2048-PNGs wurden mit korrekten natürlichen Pixelmaßen geladen.
Die beobachteten PSS-Spitzen der App-Prozessgruppe liegen bei 260/306/355 MiB
für viele Bilder und 560/714/864 MiB für große Bilder. Der Sampler erfasst alle
250 ms; kürzere Spitzen können höher sein. Die Bilderreihen laufen in derselben
Instanz hintereinander und enthalten daher Cache-/Sitzungseffekte. Die erste
Reihe enthält noch keine Rückkehr-zur-kleinen-Datei-Abnahme.

## Atomare Sonderfälle

[Sonderfallrohdaten](results/sectioned-special.json), je drei Läufe, **keine reguläre
p95-Serie**. Ein einzelner 1-MiB-Absatz braucht im Median 8.948 ms bis zum ersten
Hilfsframe; davon 4.486 ms synchrone Vorbereitung. Eine 100×100-Tabelle erreicht
372 ms, mit einer größten instrumentierten Renderaufgabe von 86 ms im Median.
Der große Codeblock erreicht 205 ms, aber 99 ms synchrone Vorbereitung.
Tief verschachtelte Listen und viele kleine Blöcke sind ebenfalls erfasst.

Das Gruppierungsziel von 16 KiB darf einen semantisch zusammenhängenden Block
nicht beliebig zerschneiden. Diese Fälle widerlegen eine allgemeine Zusage von
maximal 50 ms Hauptthread-Arbeit. Eine nachfolgende Architekturarbeit braucht
innerhalb großer Blöcke abgestimmten Aufbau/Layout sowie einen begrenzten
Speicherbedarf; allein weiteres Verkleinern der Gruppierung löst das nicht.

## Speicher und Idle

[Stabilitätsrohdaten](results/sectioned-stability.json): jeweils 30 Sekunden nach
vollständiger Darstellung und drei Sekunden Beruhigung, zuerst kleine Datei,
nach zehn Aufwärmwechseln und nach weiteren 50 abgeschlossenen Wechseln.

| Phase                | Median PSS, MiB | CPU, % eines Kerns |
| -------------------- | --------------: | -----------------: |
| Erste kleine Datei   |          207,94 |              1,033 |
| Aufgewärmte Baseline |          238,87 |              0,167 |
| Nach 50 Wechseln     |          279,62 |              0,033 |

Wachstum gegenüber aufgewärmter Baseline: **17,06 %**, unter der relativen
20-%-Grenze. Das absolute 200-MiB-Budget ist überschritten. Die initiale
30-s-CPU-Reihe liegt knapp über 1 %, die beiden späteren Reihen darunter.
Das ist keine vollständige Idle-Abnahme und kein Nachweis gegen langfristiges
Speicherwachstum. Der Sampler validiert PID-Startzeiten, rekursive Prozessgruppen
und lesbare PSS-Werte; fehlende oder wechselnde Prozesse ergeben keinen gültigen
Nullverbrauch. Drei Python-Vertragstests prüfen diese Auswertung.

## Funktionsnachweise und verbleibende Abnahme

- 1.342 Unit-/Vertragstests, darunter je 652 CommonMark-Vergleiche für Parser und
  bereinigte Abschnitte; globale Referenzen, IDs, HTML-Grenzen und Worker-Abbruch.
- 16 Browser-Bedienungstests bestanden, einschließlich Auswahl während Aufbau,
  Suchnavigation, frühe Ankersprünge und Ersetzen laufender Verarbeitung.
- Die erste Release-Serie besteht sechs native Abschnittsverträge
  ([Rohdaten](results/sectioned-contracts.json)) sowie die sieben Desktop-Smoke-Schritte.
- Reproduzierbare Runner für Öffnen, Start, Interaktion, Scrollen, Bildlast,
  Sonderfälle, PSS und Idle: [Referenzprotokoll](REFERENCE.md).

Die ausstehende Abnahme benötigt eine zugängliche Linux-Referenzmaschine mit
integrierter Grafik und SSD. Dort sind die Reihen mit installiertem Release,
verifiziertem 60-Hz-Modus und gegebenenfalls 120 Hz zu wiederholen. Tatsächlich
sichtbarer Text ab Prozessstart/Öffnungsauftrag muss zusätzlich mit externer
Aufnahme oder geeigneter Plattformmessung nachgewiesen werden. Compositor-
Präsentationsdaten müssen dem App-Fenster und richtigen Monitor zugeordnet werden.
Bis diese Nachweise und die verbleibenden Budgetüberschreitungen geklärt sind,
bleiben beide Performance-Abnahmepunkte offen.

## Phase 1 nach Entscheidung 007 (8. September 2026)

Arbeitsauftrag: [007-performance-path.md](../docs/decisions/007-performance-path.md).
Die Vorarbeiten wurden unverändert als eigener Ausgangscommit gesichert. Neue
Builds liegen getrennt unter `benchmarks/generated/phase1-*`; SHA-256 und
Quellmanifeste stehen in `results/phase1-*-source.json`. Die deterministischen
Fixtures wurden nicht neu erzeugt. Jede Öffnungsreihe verwendet 30 Öffnungen je
Größe mit `--distinct-content`; Tests und Builds laufen außerhalb der Messreihen.
Der WebDriver läuft auf einem eigenen D-Bus mit separaten XDG-Testverzeichnissen.
Dies sind lokale WebKitGTK-Hilfsmessungen auf der vorhandenen Entwicklungsmaschine,
keine neue Referenzabnahme und kein Nachweis präsentierter Pixel. SPEC-Budgets
bleiben unverändert.

| Stand | klein: komplett ms | mittel: komplett ms | groß: komplett ms | groß: Einfügen ms |
| --- | ---: | ---: | ---: | ---: |
| [Ausgang](results/phase1-baseline-open.json) | 84,5 | 968,5 | 14.092,5 | 2.142,5 |
| [P1.1](results/phase1-p11-open.json) | 69,5 | 783,5 | 11.587,0 | 263,0 |

P1.1: Die Indexierung entfällt beim Einfügen; die erste Suche erzeugt fehlende
Indizes weiterhin bei Bedarf. Das reduziert den vollständigen 10-MiB-Aufbau um
17,8 %. Die Sicherheitsbereinigung ist unverändert. TypeScript-Verträge,
Lint, Release-Build sowie native Abschnittsverträge und Desktop-Smoke bestehen.
Rohdaten: [Ausgangsverträge](results/phase1-baseline-contracts.json),
[P1.1-Verträge](results/phase1-p11-contracts.json).

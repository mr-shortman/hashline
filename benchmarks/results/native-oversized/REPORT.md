# Messreihe zu Entscheidung 013 — übergroße Blöcke und der Speicherboden

Stand: 9. September 2026. Auftrag und Begründung:
[013-oversized-blocks.md](../../../docs/decisions/013-oversized-blocks.md).
Ausgangsbefunde: [native-current/REPORT.md](../native-current/REPORT.md).

**Keine Referenzabnahme.** Entwicklungsmaschine, Ubuntu 26.04, Kernel 7.0.0-31,
Wayland/GNOME, GTK 4.22.4, **diskrete RTX 3060**, Bildwiederholrate unverändert.
Die Sitzung war nicht ruhiggestellt. Für Zeiten und Speicher ist das
nebensächlich; Frametimes wurden hier gar nicht gemessen.

Zwei Release-Builds derselben Quelle vor und nach der Änderung:

| Build | SHA-256 (Anfang) |
| --- | --- |
| vorher | `e2313fdaa9171281` |
| nachher | `ea16389e12ecec9b` |

Alle Speicherzahlen stammen aus `benchmarks/memory.py`: je ein eigener Prozess
je Fixture, privater Sitzungsbus **neben** statt um die Anwendung, Portale,
gvfs und dconf über die Umgebung ferngehalten. Alle 95 Messzeilen melden
`isolated: true` — die Prozessgruppe der Anwendung und die des Busses sind
disjunkt, der Busbaum enthält nur den Bus selbst. Damit ist erstmals nachweisbar
gemessen, was der Reader kostet, und nicht, was der Portal-Stack kostet.

## Erster Schirm, ohne Fenster (`examples/measure`, n=5)

Median / p95 in Millisekunden. „gesetzt" ist die Zahl der Blöcke für den ersten
Schirm, „Blöcke" die Länge des Plans.

| Fixture | vorher: erster Schirm | nachher | Blöcke vorher → nachher | gesetzt |
| --- | ---: | ---: | ---: | ---: |
| `small.md` | 0,3 / 17,8 | 0,3 / 18,0 | 1 446 → 1 446 | 14 |
| `medium.md` | 0,3 / 0,4 | 0,4 / 0,4 | 14 626 → 14 626 | 14 |
| `large.md` | 0,4 / 0,4 | 0,4 / 1,3 | 144 651 → 144 651 | 14 |
| `many-blocks.md` | 0,1 / 0,2 | 0,1 / 0,2 | 20 001 → 20 001 | 28 |
| `deep-list.md` | 1,5 / 1,7 | 1,5 / 1,6 | 2 → 2 | 2 |
| `wide-table.md` | 53,9 / 54,9 | **54,2 / 55,8** | 2 → 2 | 2 |
| `long-line.md` | 174,9 / 176,2 | **8,3 / 8,4** | 2 → **246** | 2 |
| `large-code.md` (n=1) | **67 139,6** | **1,5 / 1,6** | 2 → **275** | 2 |

Rohdaten: [`first-screen-before.txt`](first-screen-before.txt),
[`first-screen-after.txt`](first-screen-after.txt).

Die 67 Sekunden sind mit dem Vorher-Build genau reproduziert worden. Nach der
Änderung setzt der erste Schirm zwei Teilblöcke statt eines Blocks mit 70.004
Zeilen. Die breite Tabelle bleibt unverändert: eine Tabelle wird nicht
geschnitten, und das bleibt das letzte offene Budgetloch dieser Klasse.

Der Blockplan wird teurer, weil er die Zeilen jedes Codeblocks zählt: bei
`large.md` 0,49 → 1,45 ms für 10 MiB. Das ist der Preis für eine Schätzung, die
stimmt.

## Höhenschätzung (`examples/measure --geometry`)

Jeder Block einmal gesetzt, geschätzte gegen gemessene Gesamthöhe.

| Fixture | Blöcke | Fehler gesamt | Codeblöcke | Fehler Code |
| --- | ---: | ---: | ---: | ---: |
| `small.md` | 1 446 | −32,0 % | 289 | **−0,2 %** |
| `medium.md` | 14 626 | −32,0 % | 2 925 | **−0,2 %** |
| `many-blocks.md` | 20 001 | −2,0 % | 0 | – |
| `deep-list.md` | 2 | −94,4 % | 0 | – |
| `wide-table.md` | 2 | +358,2 % | 0 | – |
| `long-line.md` | 246 | −3,6 % | 0 | – |
| `large-code.md` | 275 | **−0,0 %** | 274 | **−0,0 %** |

Rohdaten: [`geometry-after.txt`](geometry-after.txt).

Dieselben Fixtures vor der Änderung, mit einem Wegwerf-Build der alten Quelle
gemessen — `--geometry` gibt es dort noch nicht, deshalb liegt dafür keine
Rohdatei bei: `small.md` −46,6 % gesamt und −62,6 % über die Codeblöcke,
`medium.md` ebenso, `large-code.md` −99,98 % gesamt (279 px geschätzt gegen
1 568 137 px gemessen) und −100,0 % über den einen Codeblock.

**Drei offene Schätzfehler**, alle ohne Bezug zu dieser Arbeit und alle nicht
angefasst: Fließtext −32 % (die mittlere Zeichenbreite rechnet mehr Zeichen in
eine Zeile, als eine Zeile mit ausgefranstem Rand fasst), tiefe Listen −94 %
(Einrückung und Zeilenabstand je Eintrag fehlen in der Schätzung), breite
Tabellen +358 % (geschätzt wird umbrechender Text, gesetzt wird ein Raster).

## Speicher, PSS der Prozessgruppe

Standardrenderer (`vulkan`), Median der Durchgänge nach dem Aufwärmen, MiB.
Der erste Durchgang wärmt Treiber und Shadercache und ist ausgeschlossen.

| Fixture | Byte | vorher | nachher | |
| --- | ---: | ---: | ---: | --- |
| (ohne Datei) | – | 87,9 | 88,1 | unverändert |
| `small.md` | 102 465 | 96,2 | 96,7 | unverändert |
| `many-blocks.md` | 180 018 | 91,9 | 92,7 | unverändert |
| `wide-table.md` | 91 722 | 106,2 | 106,4 | unverändert |
| `medium.md` | 1 048 581 | 117,2 | 117,6 | unverändert |
| `long-line.md` | 1 000 015 | 176,3 | **91,3** | **−85 MiB** |
| `large-code.md` | 1 050 033 | 203,3 | **92,5** | **−111 MiB** |
| `large.md` | 10 486 101 | 325,3 | 325,4 | unverändert |

Rohdaten: [`memory-before.json`](memory-before.json),
[`memory-before-large-code.json`](memory-before-large-code.json),
[`memory-after.json`](memory-after.json).

`large-code.md` brauchte vorher **90 Sekunden Beruhigungszeit**, bevor überhaupt
etwas zu messen war, und die beiden Werte streuen entsprechend (224,3 und
182,4 MiB); nachher genügen die üblichen fünf Sekunden und die drei Durchgänge
liegen zwischen 92,3 und 92,6 MiB.

`long-line.md` lag vorher **88 MiB über dem leeren Fenster** für eine 1-MiB-Datei
— mehr als das gleich große `medium.md` mit 29 MiB. Jetzt sind es 3 MiB. Das ist
genau das eine Pango-Layout über eine Million Zeichen, das nicht mehr existiert.

Alles andere ist unverändert, wie es sein soll: die Änderung schneidet nur
Blöcke, die kein gewöhnliches Dokument hat. `large.md` bleibt bei 325 MiB — das
ist das Inhaltsverzeichnis mit 28 931 `GtkLabel`, Befund 2 aus der vorherigen
Reihe, hier nicht angefasst.

Leerlauf-CPU nachher: Median 0,00 %, Maximum 2,18 % eines Kerns über alle 72
Zeilen.

## GSK-Renderer, leeres Fenster

Derselbe Build, dieselbe Sitzung, drei Durchgänge je Renderer.

| Renderer | leeres Fenster | `small.md` | `large.md` |
| --- | ---: | ---: | ---: |
| Standard (`vulkan`) | 88,0 | 96,6 | 324,2 |
| `gl` | **74,0** | 80,9 | 305,1 |
| `cairo` | **33,0** | 41,3 | 256,8 |

Kleinster Wert der Durchgänge nach dem Aufwärmen, weil die beiden
GPU-Renderer Ausreißer nach **oben** zeigen: `gl` mit `small.md` misst in drei
Durchgängen 80,9 / 134,8 / 137,4 MiB, der Standardrenderer mit `medium.md`
117,4 / 117,6 / 143,9. `cairo` streut über alle Fixtures um weniger als
0,5 MiB. Diese Streuung ist selbst ein Befund: der Speicherboden der
GPU-Renderer ist nicht nur hoch, er ist auch unruhig.

**55 MiB liegen zwischen dem Standardrenderer und `cairo`, 14 MiB zwischen ihm
und `gl`.** Gegen das 80-MiB-Budget für kleine Dateien, das mit 96,6 MiB
verfehlt wird, ist das der größte Einzelposten nach dem Inhaltsverzeichnis. Der
frühere Nebensatz aus `native-current/REPORT.md` — 27 statt 75 MiB — ist damit
der Richtung nach bestätigt und der Höhe nach genauer bekannt.

**Umgestellt wird nichts.** `cairo` rendert auf der CPU, und der Frameanteil
beim Scrollen ist selbst ein verfehltes Budget (92,9–98,3 % gegen 99 %). Eine
Umstellung braucht Frametimes aller drei Renderer auf einer ruhiggestellten
Sitzung; `benchmarks/compositor.py` kann sie aufzeichnen, verstellt dafür aber
die Bildwiederholrate des Monitors und übernimmt den Zeiger, weshalb der Lauf
hier bewusst unterblieben ist.

## Was diese Reihe nicht zeigt

- Keine Frametimes, weder für die Änderung noch für die Renderer.
- Keine Referenzhardware: gemessen wurde auf einer diskreten RTX 3060, gefordert
  ist integrierte Grafik.
- Keine Aussage über das Inhaltsverzeichnis, die Suche oder die Startzeit.
- Der GTK-Integrationstest `native_ui` schlägt auf dieser Maschine an der
  Overlay-Reihenfolge und am Escape-Verhalten fehl, **vor wie nach** der
  Änderung. Die davor laufenden Prüfungen der Textschnittstelle, der Auswahl
  und des Inhaltsverzeichnisses bestehen.

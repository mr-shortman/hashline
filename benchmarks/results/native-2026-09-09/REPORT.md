# Messreihe nach Speicher, Tabs und Live-Reload

Stand: 9. September 2026. Release-Build, `GSK_RENDERER=cairo`
([015](../../../docs/decisions/015-renderer-choice.md)), Ubuntu 26.04,
Kernel 7.0.0-31, Wayland/GNOME, GTK 4.22.4. Ein Prozess je Messung, jeder auf
einem eigenen privaten D-Bus, damit Portal, gvfs und dconf nicht in die
gemessene Prozessgruppe geraten (`benchmarks/memory.py`).

**Dies ist keine Abnahme.** Die Reihen haben n = 3 statt der geforderten
n = 30, und die Sitzung war nicht ruhiggestellt. Die Werte sind Belege für die
Richtung der Arbeit, nicht die Reihe, die SPEC Abschnitt 9 verlangt.

Verglichen wird gegen die Ziele aus
[014](../../../docs/decisions/014-competitive-targets.md), die überall
strenger sind als SPEC Abschnitt 9.

## Speicher

PSS der Prozessgruppe nach dem Beruhigen, drei verschränkte Durchgänge.

| Fixture | Bytes | PSS | Ziel | |
| --- | ---: | ---: | ---: | --- |
| (ohne Datei) | – | 32,9–33,3 MiB | – | Boden |
| small | 102 465 | 40,7–40,9 MiB | ≤ 40 MiB | knapp verfehlt |
| medium | 1 048 581 | 42,2–42,3 MiB | ≤ 70 MiB | erfüllt |
| large | 10 486 101 | 58,1–58,4 MiB | ≤ 150 MiB | erfüllt |
| **Zuwachs large über small** | | **17,43–17,52 MiB** | ≤ 20 MiB | **erfüllt** |

Zum Vergleich: dieselbe Zeile stand bei 402 MiB, als
[014](../../../docs/decisions/014-competitive-targets.md) geschrieben wurde,
und bei 258 MiB vor dieser Arbeit. Rohdaten: [memory-cairo.json](memory-cairo.json).

Die 100-KiB-Zeile ist um 0,7–0,9 MiB verfehlt. Davon sind 1,5 MiB die
CJK-Schrift, die `small.md` mit „Grüße 日本語“ in jedem Abschnitt anfordert:
ersetzt man dieses eine Wort durch lateinische Buchstaben, sinkt derselbe
Aufbau auf 38,9 MiB. Der Betrag ist damit nicht dem Betrachter, sondern dem
Inhalt der Fixture zuzurechnen — was ihn nicht aus dem Budget nimmt.

### Was den Unterschied gemacht hat

Gemessen an den Datenstrukturen, die für `large.md` gehalten werden:

| Posten | vorher | jetzt |
| --- | ---: | ---: |
| Inhaltsverzeichnis als Widgets | 153 MiB | 0 |
| Op-Buffer | 24,4 MiB | 3,1 MiB |
| Zeichenketten | 2,4 MiB | 0,9 MiB |
| Zweite Kopie des Texts für AT-SPI | 7,9 MiB | 0 (bei Bedarf) |
| Blocktabelle des Dokuments | 2,8 MiB | 0 |
| Blockplan | 5,7 MiB | 4,0 MiB |

## Tabs

Fixture `medium.md` (1 MiB), ein Prozess je Zeile, echte Kopien mit eigenen
Namen: eine bereits offene Datei wird nach vorn geholt statt erneut geöffnet.

| Offene Tabs | PSS | Ziel | |
| ---: | ---: | --- | --- |
| 1 | 42,8 MiB | – | |
| 2 | 44,7 MiB | Zusatz ≤ 3 MiB | erfüllt (1,9 MiB) |
| 10 | 59,9 MiB | ≤ 110 MiB | erfüllt |

Rohdaten: [tabs-1.json](tabs-1.json), [tabs-2.json](tabs-2.json),
[tabs-10.json](tabs-10.json).

## Hauptthread

Kein zusammenhängendes Stück Arbeit über 16 ms, gemessen vom Programm selbst
(`crates/hashline/src/view/mainthread.rs`) über den Fenstertest
`main_thread_work_stays_inside_the_frame_budget`: jede Fixture wird zwanzig
Schritte hinunter und zehn wieder hinauf gefahren.

| Fixture | längste Aufgabe | vorher |
| --- | ---: | ---: |
| large.md | 11,02 ms | – |
| wide-table.md | 0,53 ms | **53 ms** |
| long-line.md | 5,47 ms | 172 ms (vor 013) |
| large-code.md | 2,04 ms | 66 852 ms (vor 013) |
| many-blocks.md | 0,09 ms | – |
| deep-list.md | 0,81 ms | – |

Drei Änderungen tragen das: eine Zellenbreite wird je verschiedenem Zellentext
gemessen statt je Zelle, Spalten jenseits des Schirmrands werden gar nicht erst
gesetzt, und der Schirm Puffer über und unter dem Sichtbereich wird nicht mehr
im selben Frame gesetzt wie der Sichtbereich, sondern in Scheiben von 6 ms im
Leerlauf. Der Textteil eines übergroßen Absatzes ist von 4 KiB auf 2 KiB
halbiert, weil zwei solche Teile auf einem Schirm zusammen 16,3 ms kosteten und
die Kosten quadratisch mit der Länge wachsen.

## Eigener Anteil an der Öffnungszeit, n=30

Ohne Fenster gemessen (`examples/measure`); Median / p95 in Millisekunden.

| Fixture | Bytes | Parsen | Blockplan | Erster Schirm | Blöcke | gesetzt |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| small | 102 465 | 0,7 / 0,8 | 0,01 / 0,01 | 0,3 / 0,3 | 1 446 | 14 |
| medium | 1 048 581 | 6,0 / 6,7 | 0,12 / 0,13 | 0,3 / 0,4 | 14 626 | 14 |
| large | 10 486 101 | 76,8 / 77,9 | 1,26 / 1,29 | 0,4 / 0,5 | 144 651 | 14 |
| wide-table | 91 722 | 0,6 / 0,6 | 0,00 / 0,00 | 3,8 / 3,9 | 2 | 2 |
| long-line | 1 000 015 | 0,9 / 1,0 | 0,10 / 0,10 | 8,2 / 8,3 | 246 | 2 |
| large-code (n=3) | 1 050 033 | 1,5 | 0,21 | 1,5 / 6,7 | 275 | 2 |

Das Parsen läuft auf einem Arbeitsthread und blockiert nichts.

## Suche

Ein Durchlauf über die 7,9 MiB Text von `large.md` kostet 5–6 ms, unabhängig
davon, ob er 0, 28 930 oder 983 622 Treffer findet. Das Budget von 120 ms ab
letztem Tastendruck besteht damit fast ganz aus Warten: `GtkSearchEntry` meldet
eine Änderung mit 150 ms Verzögerung, und darauf lagen weitere 120 ms eigene
Bündelung. Die erste ist abgeschaltet, die zweite auf 60 ms gesenkt.

## Leerlauf und Dauerbetrieb

Drei Fenster von je 30 Sekunden.

| Fixture | Leerlauf-CPU | Ziel |
| --- | ---: | --- |
| small | 0,000–0,033 % | < 0,3 % |
| large | 0,000 % | < 0,3 % |

Rohdaten: [idle-cairo.json](idle-cairo.json). Fünfzig echte Dateiwechsel über
die Instanzübergabe: **+1,91 %** gegenüber der aufgewärmten Ausgangsmessung bei
einem Ziel von ≤ 20 %, keine fehlgeschlagene Übergabe
([stability.json](stability.json)). Mit Tabs ist die wiederholte Übergabe
derselben Datei ein Tabwechsel; der Ladeweg selbst ist über den `reload`-Teil
der Suite und `crates/hashline/tests/reload.rs` abgedeckt.

## Live-Reload

Die Zeit vom Schreiben bis zur angeforderten Neuladung liegt bei p95 74 ms
gegen ein Gesamtbudget von 250 ms bis zum sichtbaren Text
(`crates/hashline/tests/reload.rs`). Zwei Fehler hat der Vertrag aufgedeckt und
sind behoben: das Lesen der offenen Datei galt als Änderung an ihr, sodass sich
der Prozess nach jedem Speichern selbst weckte, und die Bündelung wartete eine
feste Zeit statt auf Ruhe, sodass zwanzig Schreibvorgänge vier Neuladungen
ergaben.

## Was diese Reihe nicht enthält

Alles, dessen Nachweis ein Einzelbild des Monitors ist: Start bis lesbarer
Text, Öffnen in laufender Instanz, Suche und Menü öffnen, Tabwechsel und der
Frameanteil beim Scrollen. Die Werkzeuge dafür sind vorhanden und werden von
`benchmarks/run.py bench --only content,interaction,tabs,reload,scroll`
aufgerufen; sie brauchen eine unbeaufsichtigte Sitzung, in der das Fenster des
Betrachters allein auf dem Schirm steht. Ein Versuch auf dieser Arbeitssitzung
las per Texterkennung das Terminal, vor dem das Fenster stand — kein Ergebnis
ist besser als ein solches.

Ebenfalls offen: die Reihe mit n = 30, 120 Hz, die Referenzmaschine mit
integrierter Grafik und der Vergleich gegen die vier Konkurrenten aus
[014](../../../docs/decisions/014-competitive-targets.md) Abschnitt 4.

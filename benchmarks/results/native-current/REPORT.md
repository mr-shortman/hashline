# Messreihe, nativer Stand (aktueller Build)

Commit-Stand: nach `bf2a6d5` (WebView entfernt). Release-Build,
GSK-Standardrenderer (`vulkan`). Ubuntu 26.04, Kernel 7.0.0-31, Wayland/GNOME,
GTK 4.22.4. Primärer Monitor DP-3 (GIGA-BYTE G27FC), 1920×1080 bei 60 Hz —
**die Bildwiederholrate wurde für diese Reihe nicht verstellt**, daher fehlen
120-Hz-Werte.

Wichtige Einschränkung: die Sitzung war **nicht ruhiggestellt**. Für die
Zeit- und Speicherwerte ist das nebensächlich, für die Frametimes nicht — sie
messen den Monitor samt aller Clients.

## Eigener Anteil an der Öffnungszeit, n=30

Median / p95 in Millisekunden, ohne Fenster gemessen (`examples/measure`).
„gesetzt" ist die Zahl der Blöcke, die für den ersten Schirm gesetzt wurden.

| Fixture | Bytes | Parsen | Blockplan | Erster Schirm | Blöcke | gesetzt |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| small | 102 465 | 0,7 / 0,9 | 0,00 / 0,01 | 0,3 / 0,4 | 1 446 | 14 |
| medium | 1 048 581 | 7,3 / 7,7 | 0,05 / 0,05 | 0,3 / 0,4 | 14 626 | 14 |
| large | 10 486 101 | 77,4 / 79,4 | 0,49 / 0,51 | 0,4 / 0,4 | 144 651 | 14 |
| many-blocks | 180 018 | 1,6 / 1,6 | 0,06 / 0,06 | 0,1 / 0,1 | 20 001 | 28 |
| deep-list | 11 004 | 0,0 / 0,0 | 0,00 / 0,00 | 1,5 / 1,5 | 2 | 2 |
| wide-table | 91 722 | 0,7 / 0,7 | 0,00 / 0,00 | **53,3 / 53,8** | 2 | 2 |
| long-line | 1 000 015 | 1,0 / 1,0 | 0,00 / 0,00 | **172,0 / 174,3** | 2 | 2 |
| large-code (n=1) | 1 050 033 | 1,6 | 0,00 | **66 451,5** | 2 | 2 |

Die Virtualisierung trägt: 10 MiB werden lesbar, indem **14 von 144 651
Blöcken** gesetzt werden, und der Blockplan dafür kostet eine halbe
Millisekunde.

## Speicher, PSS der Prozessgruppe

Je Fixture ein eigener Prozess, nach dem Beruhigen gemessen.

| Fixture | Bytes | PSS | Leerlauf-CPU über 30 s |
| --- | ---: | ---: | ---: |
| (ohne Datei) | – | 75 MiB | 0,27 % |
| small | 102 465 | 84 MiB | 0,17 % |
| many-blocks | 180 018 | 79 MiB | 0,17 % |
| wide-table | 91 722 | 96 MiB | 0,20 % |
| medium | 1 048 581 | 104 MiB | 0,20 % |
| long-line | 1 000 015 | 171 MiB | 0,27 % |
| large | 10 486 101 | **313 MiB** | 0,17 % |

## Fünfzig Dateiwechsel

Echte Wechsel über die Instanzübergabe, wie ein Dateimanager sie auslöst.
Zuwachs gegenüber der aufgewärmten Ausgangsmessung: **+2,76 %**, kein
steigender Trend innerhalb der Abschlussmessung. Rohdaten in
`../native-m0/stability.json`.

## Scroll-Frametimes, 60 Hz

Fixture `large.md`, drei Zehn-Sekunden-Fenster.

| Lauf | Frames | im 16,7-ms-Budget | größte Lücke |
| ---: | ---: | ---: | ---: |
| 1 | 600 | 98,33 % | 33,3 ms |
| 2 | 596 | 94,46 % | 33,3 ms |
| 3 | 595 | 92,94 % | 33,4 ms |

## Gegen die Budgets aus SPEC Abschnitt 9

| Budget | Soll | Ist | |
| --- | --- | --- | --- |
| Ruhender Speicher, kleine Datei | ≤ 80 MiB PSS | 84 MiB | knapp verfehlt |
| **Speicherzuwachs über Dokumentgröße** | ≤ PSS(100 KiB) + 4× Dateigröße = 126 MiB | **313 MiB** | **deutlich verfehlt** |
| Leerlauf nach Abschluss | < 0,5 % eines Kerns | 0,17–0,27 % | erfüllt |
| Kein Wachstum über 50 Wechsel | ≤ 20 % über Ausgangswert | +2,8 % | erfüllt |
| Scrollen, Frames im Budget | ≥ 99 % über 10 s | 92,9–98,3 % | verfehlt |
| Scrollen, kein Stillstand | ≤ 50 ms | 33,4 ms | erfüllt |
| Hauptthread während Interaktion | keine Aufgabe > 16 ms | 53 ms / 172 ms / 66 s | **verfehlt** |
| Installierte Größe | ≤ 20 MiB | 3,84 MiB Binary | erfüllt |

## Die drei Befunde, die zählen

### 1. Ein einzelner Codeblock friert das Fenster 67 Sekunden ein

`large-code.md` ist **ein** eingezäunter Codeblock mit 70 004 Zeilen. Ein
Codeblock ist genau ein Block des Plans, also hilft Virtualisierung hier
überhaupt nicht: um irgendetwas zu zeichnen, muss er ganz gesetzt werden. Am
laufenden Programm nachgemessen — Zustand `R`, 100 % CPU, 66,85 s Rechenzeit,
danach wieder ansprechbar.

Dasselbe in kleiner Form bei `long-line` (172 ms) und `wide-table` (53 ms). Die
tragende Annahme des Blockplans — ein Block ist etwa schirmgroß — hält für
Codeblöcke, sehr lange Zeilen und breite Tabellen nicht. Die Abhilfe ist
strukturell: solche Blöcke müssen in Teilblöcke zerfallen, damit die
Virtualisierung in sie hineinreicht.

### 2. Das Inhaltsverzeichnis kostet 153 MiB

`fill_outline` legt beim Laden **ein `GtkLabel` je Überschrift** an, unabhängig
davon, ob das Verzeichnis sichtbar ist. `large.md` hat 28 931 Überschriften.

Nachgewiesen, nicht vermutet: mit unterdrückten Zeilen sinkt PSS von 310 auf
**157 MiB**. Das ist auch die Erklärung für die Regression gegenüber M0, wo
`large.md` noch bei 121 MiB lag. `GtkListView` mit einem Modell ist genau für
diesen Fall gebaut.

Die restlichen 36 MiB Unterschied zu M0 sind ungeklärt und stecken in den
Ergänzungen aus M1/M2.

### 3. Der Frame-Anteil ist schlechter als in der früheren Reihe

Früher 97,2–98,3 %, jetzt 92,9–98,3 % bei denselben 60 Hz. Das ist **kein
Regressionsnachweis**: gemessen werden Mutters Präsentationsmarken für den
Monitor, alle Clients eingeschlossen, und diese Sitzung war nicht
ruhiggestellt. Der Wert ist eine Untergrenze für die Qualität der Anwendung.
Für eine belastbare Aussage müssten die Läufe auf einer ruhigen Sitzung
wiederholt werden — dann auch mit `GSK_RENDERER=cairo`, das im leeren Fenster
27 statt 75 MiB kostet und dessen Frametimes noch niemand kennt.

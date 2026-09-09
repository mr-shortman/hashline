# 011 — M0: technische Grundlage, erste Messungen, offene Risiken

Status: In Arbeit. Stand: 8. September 2026.
Vorgänger: [009-native-renderer.md](009-native-renderer.md),
[010-op-buffer-for-the-native-renderer.md](010-op-buffer-for-the-native-renderer.md).
Bezug: [SPEC.md](../../SPEC.md), Abschnitte 5, 9, 10 und 14 (M0).

Dieses Dokument hält fest, was die native Grundlage jetzt kann, welche
Entscheidungen dabei zu treffen waren und — vor allem — welche der vier Risiken
aus M0 **noch nicht** entkräftet sind. Es ist keine Abnahme.

## 1. Was existiert

`crates/hashline` ist eine Bibliothek mit dünnem Binary darauf, damit alles,
was kein Fenster braucht, ohne Fenster geprüft werden kann (SPEC Abschnitt 12).

| Modul | Inhalt |
| --- | --- |
| `theme` | Die Token-Tabelle aus SPEC Abschnitt 3, Hell und Dunkel aus einer Definition |
| `layout` | Blockplan, Höhenschätzung, Prefix-Summen, Op-Buffer → Pango |
| `view` | Dokumentwidget (`GtkScrollable`), Auswahlmodell |
| `app` | `GtkApplication`, Fenster, `GtkHeaderBar`, Aktionen mit Tastenbelegung |

45 Tests laufen unter Cargo; `cargo fmt --check` und
`cargo clippy --workspace --all-targets -- -D warnings` sind grün.

Dazu zwei Werkzeuge ohne Fenster: `examples/render` setzt ein Dokument über
**denselben** Layout-Code wie das Widget in ein PNG — brauchbar für den
visuellen Abgleich aus SPEC Abschnitt 12 und für Maschinen, auf denen
Bildschirmaufnahmen nicht erlaubt sind. `examples/measure` instrumentiert
Parsen, Blockplan und erstes Setzen getrennt, wie SPEC Abschnitt 9 es verlangt.

## 2. Entscheidungen, die dabei zu treffen waren

### GTK-Mindestversion 4.14

`GtkAccessible` gibt es ab 4.10, die `GtkAccessibleText`-Schnittstelle, über die
das Dokumentwidget seinen Text meldet (SPEC Abschnitt 3), ab 4.14. Damit ist
**4.14 die Untergrenze**. Sie ist von Anfang 2024 und in den Zielverteilungen
vorhanden; die Referenzmaschine läuft mit 4.22.

### Zeilenhöhe wird absolut gesetzt, nicht als Faktor

`pango::Layout::set_line_spacing(1.65)` sieht wie das CSS `line-height: 1.65`
der Gestaltungsreferenz aus, ist es aber nicht: der Faktor multipliziert die
**natürliche** Zeilenhöhe der Schrift, die den Durchschuss der Schnitt schon
enthält. 1,65 landet dadurch bei etwa 2,0, und der Satz wird sichtbar zu luftig
— im ersten Render sofort erkennbar.

Gesetzt wird deshalb `pango::AttrInt::new_line_height_absolute` mit
`Schriftgröße × Faktor`. Das ist die Semantik, in der die Referenz geschrieben
ist. Dies ist genau die Art von Abweichung, die SPEC Abschnitt 3 vorsieht:
„Wo Pango und CSS sich unvermeidlich unterscheiden […] gilt der optische
Eindruck der Referenz."

### Prefix-Summen über Blockhöhen als Fenwick-Baum

Der Plan braucht drei Dinge in Interaktionsgeschwindigkeit: Gesamthöhe,
Oberkante eines Blocks, und den Block an einer Scrollposition. Ein einfaches
Summenfeld beantwortet die letzten beiden in konstanter Zeit, kostet aber einen
vollständigen Neuaufbau, sobald **eine** Höhe sich ändert — und Höhen ändern
sich dauernd, weil jeder einscrollende Block seine Schätzung ersetzt. Ein
Fenwick-Baum macht alle drei logarithmisch. Bei 144 651 Blöcken ist das der
Unterschied zwischen einer Messung nebenbei und einem Durchlauf pro Frame.

### Der Layouttext eines Blocks ist seine Scheibe des Textblobs

Byte für Byte, mit einer Ausnahme: das abschließende `\n` eines eingezäunten
Codeblocks fällt weg, sonst stünde unter jedem Codeblock eine Leerzeile. Der
Text bleibt damit ein **Präfix** der Scheibe, und genau das ist die Zusage —
jeder Offset im Layout bedeutet weiter dasselbe Byte im Dokument. Auswahl,
Suchmarkierung und Treffertest brauchen deshalb keine eigene
Übersetzungstabelle. Der Preis: Listenzeichen und Aufzählungsnummern fehlen
noch, weil sie Text einfügen würden, den das Dokument nicht hat (siehe
Abschnitt 5).

## 3. Ein Fehler, der ohne die 10-MiB-Fixture nicht aufgefallen wäre

`Slugs::id` hat für jede Überschrift die Suffixe `-1`, `-2`, … von vorn
durchprobiert. Bei *N* gleichlautenden Überschriften ist das O(N²). Die
Fixture mit 60 000 identischen `## Kapitel` hat den Parser damit auf Minuten
festgenagelt — bei einer Datei, die SPEC Abschnitt 9 in 400 ms lesbar sehen
will. Dokumente wiederholen Überschriften berufsmäßig: Changelogs, generierte
Referenzen, API-Dokumentation mit „Parameters" und „Returns" je Eintrag.

Behoben durch ein gemerktes „nächstes Suffix" je Basis; die Menge der belegten
Slugs wird weiter befragt, weil ein Suffix mit der Überschrift „Kapitel 1"
kollidieren kann. Zwei Tests halten Reihenfolge und Kollisionsfall fest.

## 4. Erste Messungen

> **Überholt.** Die Zahlen in diesem Abschnitt sind der Stand vom
> 8. September 2026 und stehen hier als Datum, nicht als Auskunft. Sie werden
> durch [013](013-oversized-blocks.md) und den
> [nativen Messbericht](../../benchmarks/results/native-current/REPORT.md)
> ersetzt; die Speicherzusage unten gilt insbesondere **nicht** mehr. Was heute
> offen ist, steht in [Einschränkungen](../limitations.md).

Release-Build, Entwicklungsmaschine, **n=1**. Das ist eine Peilung, **keine
Abnahme**: SPEC Abschnitt 9 verlangt n=30 auf der festgelegten
Referenzmaschine, und die steht noch nicht fest.

Eigener Code, ohne Fenster (`examples/measure`):

| Fixture | Bytes | Parsen | Blockplan | Erster Schirm | Blöcke | davon gesetzt |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| small | 102 465 | 0,9 ms | 0,0 ms | 13,4 ms | 1 446 | **15** |
| medium | 1 048 581 | 7,9 ms | 0,1 ms | 12,9 ms | 14 626 | **15** |
| large | 10 486 101 | 84,6 ms | 1,1 ms | 13,6 ms | 144 651 | **15** |
| long-line | 1 000 015 | 1,5 ms | 0,0 ms | **190,3 ms** | 2 | 2 |

Die Spalte „davon gesetzt" ist der Kern der Sache: 10 MiB werden lesbar, indem
**15 Blöcke** gesetzt werden. Die 13 ms im ersten Schirm sind größtenteils
einmaliges Laden der Schrift und fallen bei jedem weiteren Block weg.

Speicher der laufenden Anwendung, PSS der Prozessgruppe über
`/proc/<pid>/smaps_rollup`, wie SPEC Abschnitt 9 es festlegt:

| Fixture | GSK `vulkan` (Voreinstellung hier) | GSK `cairo` |
| --- | ---: | ---: |
| Leeransicht | 73 MiB | **27 MiB** |
| small (100 KiB) | 78 MiB | **31 MiB** |
| medium (1 MiB) | 81 MiB | **35 MiB** |
| large (10 MiB) | 121 MiB | **74 MiB** |

Zum Vergleich aus [009](009-native-renderer.md): ViewMD liegt bei 41 MiB für
100 KiB und **147 MiB für 1 MiB**; der WebView-Stand bei 193 MiB für 100 KiB.

**Die Zusage aus SPEC Abschnitt 9 wird eingehalten.** Die Budgetzeile lautet:
PSS bei 10 MiB höchstens PSS bei 100 KiB plus das Vierfache der Dateigröße,
also 31 + 4 × 10,0 = 71,6 MiB. Gemessen sind 74 MiB — 3 % darüber, bei n=1 und
ohne jede Optimierung. Der Zuwachs beträgt 43 MiB für 10,0 MiB Datei, also das
4,1-fache. Und der Unterschied zur Latte ist deutlich: zwischen 100 KiB und
1 MiB wächst Hashline um 4 MiB, ViewMD um 106 MiB.

### Der GSK-Renderer entscheidet über den Speicherboden

Das ist der überraschendste Einzelwert: die leere Anwendung kostet mit `vulkan`
73 MiB, mit `gl` 119 MiB, mit `ngl` 63 MiB und mit `cairo` 27 MiB. Der
Dokumentanteil ist in allen Fällen derselbe — **der Boden ist die
Renderer-Wahl**, nicht die Anwendung.

Damit ist auch die Rechnung aus 009 Abschnitt 1 nachträglich geschärft: Der
„GUI-Boden" von 41 MiB, den ViewMD markiert, ist selbst eine Renderer-Frage.
Mit `cairo` liegt Hashline mit geladenem 100-KiB-Dokument bei 31 MiB und damit
unter ViewMDs Leerwert.

**Es wird hier noch nicht entschieden.** `cairo` zeichnet auf der CPU, und
Risiko 4 — Frametimes bei 60 und 120 Hz — ist ungemessen. Ein Speichergewinn,
der das Scrollen ruckeln lässt, verletzt Priorität 1 aus SPEC Abschnitt 1. Die
Entscheidung fällt, wenn die Compositor-Messung vorliegt, und wird hier
nachgetragen.

## 5. Was M0 noch nicht entkräftet hat

Ehrlich gegen die vier Risiken aus SPEC Abschnitt 14 gestellt:

| Risiko | Stand |
| --- | --- |
| 1. Virtualisiertes Blocklayout ohne springenden Inhalt | Rechnung getestet, **interaktiv unbestätigt** |
| 2. Auswahl über Blockgrenzen, Hit-Test, Zwischenablage | Modell getestet, **Ziehen und Kopieren unbestätigt** |
| 3. 10 MiB öffnen, scrollen, suchen | Öffnen und Speicher gemessen; **Scrollen ungemessen, Suche existiert nicht** |
| 4. Scroll-Frametimes 60/120 Hz | **Ungemessen** |

Zu Risiko 1: `BlockPlan::set_measured` gibt zurück, um wie viel sich alles
darunter verschoben hat, und das Widget addiert diesen Betrag auf den
Scrolloffset, wenn der Block über der Leseposition lag. Tests halten die
Rechnung fest — dass das Zusammenspiel mit `GtkScrolledWindow` beim echten
Scrollen tatsächlich stillsteht, ist damit **nicht** gezeigt.

Zu Risiko 2: Das Auswahlmodell — `(Blockindex, Byteoffset)` mit Ordnung über
Paaren — ist vollständig getestet, einschließlich Rückwärtsziehen, Teilblöcken
an beiden Enden und ganzen Blöcken in der Mitte. Die Gestenauswertung und
`copy_selection` sind geschrieben, aber nicht von Hand bedient worden.

Der Grund für beide Lücken ist derselbe: Bildschirmaufnahmen sind auf dieser
Maschine über D-Bus untersagt (`org.freedesktop.DBus.Error.AccessDenied`), und
ohne Aufnahme lässt sich weder ein Sprung noch eine Auswahlmarkierung belegen.
Die Anwendung startet, öffnet die 10-MiB-Fixture und bleibt fehlerfrei stehen;
mehr ist von hier aus nicht nachweisbar.

**Weitere offene Punkte:**

- **Die extrem lange Zeile kostet 190 ms.** Eine Zeile von 1 MB in einem
  Pango-Layout ist von Natur aus teuer, und 190 ms sind das Zwölffache des
  16-ms-Budgets aus SPEC Abschnitt 9. Die Fixture steht dort als Sonderfall und
  braucht eine eigene Entscheidung: Umbruch erzwingen, die Zeile in synthetische
  Teilblöcke schneiden, oder jenseits einer Grenze abschneiden.
- **Listenzeichen und Aufzählungsnummern fehlen.** Sie fügen Text ein, den das
  Dokument nicht enthält, und würden die 1:1-Zuordnung aus Abschnitt 2 brechen.
  Sie brauchen eine Marker-Offset-Abbildung, die **einmal** entworfen wird —
  Aufgabe von M1, nicht ein Flüchtigkeitsfehler.
- **Tabellen setzen noch als Text**, ohne Spaltenraster. 009 Abschnitt 5 nennt
  das ausdrücklich als Eigenleistung; es ist noch nicht erbracht.
- **Bilder, Syntaxhervorhebung, Suche, Inhaltsverzeichnis, Dateibeobachtung**
  existieren nicht. M1 und M2.
- **Die Referenzmaschine ist nicht festgelegt** und die Vergleichsreihe mit
  n=30 gegen ViewMD und den WebView-Stand ist nicht nachgeholt. Beides steht in
  SPEC Abschnitt 14 als Auftrag von M0.
- **Die Referenzaufnahmen der WebView-Fassung** fehlen weiter; siehe
  [docs/design/README.md](../design/README.md). Sie werden vor M1 gebraucht.

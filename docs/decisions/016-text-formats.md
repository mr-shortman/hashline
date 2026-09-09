# 016 — Textformate für v2: Grundprogramm und Formatmodule

Status: Vorschlag, noch nicht beschlossen. Stand: 9. September 2026.
Vorgänger: [009-native-renderer.md](009-native-renderer.md),
[010-op-buffer-for-the-native-renderer.md](010-op-buffer-for-the-native-renderer.md),
[014-competitive-targets.md](014-competitive-targets.md),
[015-renderer-choice.md](015-renderer-choice.md).
Bezug: [SPEC.md](../../SPEC.md), Abschnitte 1, 2, 4 und 9.

**Dieses Dokument ändert `SPEC.md` nicht und betrifft v1 nicht.** Es hält den
Umfang für v2 fest, damit die Frage nicht mitten in der v1-Abnahme erneut
aufkommt. Nichts hier darf vor der Abnahme nach
[M3](../acceptance/M3.md) umgesetzt werden; die offenen Befunde in
[Einschränkungen](../limitations.md) haben Vorrang.

## 1. Der Vorschlag

Hashline wird in v2 vom Markdown-Viewer zum **Leser für textförmige Dateien**.
Das Programm zerfällt dafür in zwei Teile:

- Das **Grundprogramm** ist alles, was heute schon da ist und mit Markdown
  nichts zu tun hat: Fenster, Dokumentwidget, Blockplan und Virtualisierung,
  Textsatz mit Pango, Zeichnen mit GSK, Auswahl, Suche, Inhaltsverzeichnis,
  Themes, Zoom, Nachladen, Lesepositionen, Barrierefreiheit.
- Ein **Formatmodul** ist ein Erzeuger. Es bekommt Text und liefert ein
  `OpDocument`. Mehr nicht.

Der Schnitt existiert bereits. `crates/hashline` berührt aus dem Parser nur
`OpDocument`; `hashline_markdown::parse` ist die einzige Stelle, an der
Markdown in die Anwendung eintritt. Der Schritt zu v2 ist deshalb keine
Umstellung, sondern eine Verallgemeinerung: `OpDocument` wird als
Dokumentschnittstelle festgeschrieben, und `hashline-markdown` wird ein
Erzeuger unter mehreren.

Damit erbt jedes neue Format ohne eigenes Zutun die Virtualisierung, die
dokumentweite Suche, die blockübergreifende Auswahl, das Nachladen unter Erhalt
der Leseposition und die Themes.

## 2. Die Abgrenzung: textförmig, nicht seitenförmig

**PDF, EPUB, DjVu, PostScript, Comic-Archive, Office-Dateien und Bildformate
gehören ausdrücklich nicht dazu und sollen es auch später nicht.**

Der Grund ist kein Geschmack, sondern die Begründung des Programms. Diese
Formate sind paginiert oder binär. Sie brauchen eine zweite Rendering-Pipeline
und eine Fremdbibliothek in der Größenordnung von poppler, mupdf oder einer
WebView. Damit fällt genau die Eigenschaft, für die [009](009-native-renderer.md)
die WebView zurückgenommen hat: der Speicherboden und der Kaltstart. Ein
EPUB-Modul auf WebKit wäre die Rücknahme der Rücknahme.

Hinzu kommt die Modellfrage. Das Dokumentmodell ist ein durchgehender
Textstrom mit einem Blockplan und Byte-Offsets in ein Textblob. Auswahl, Suche
und Leseposition sind darauf gebaut. Eine Seite mit fester Größe ist kein
Block in diesem Sinne, und Seitennavigation ist keine Blockvirtualisierung. Ein
paginiertes Format wäre nicht ein weiterer Erzeuger, sondern ein zweites
Programm im selben Prozess.

Wer PDF lesen will, hat Zathura, Papers und Okular. Die Lücke, die dieses
Projekt füllt, liegt woanders.

## 3. Warum das die These aus 014 schärft

[014](014-competitive-targets.md) begründet das Programm nicht mit „Markdown
schnell", sondern mit: **Öffnungszeit und Speicher sind von der Dokumentgröße
unabhängig.** Gemessen dort: ViewMD erreicht bei 10 MiB innerhalb von 30
Sekunden keinen Frame, Hashline unter `cairo` den ersten Frame in 69 ms.

Dieser Vorsprung ist ausgerechnet bei Markdown am wenigsten wert. Markdown-
Dateien von 10 MiB sind ein Fixture, kein Alltag. Wertvoll ist er bei genau den
Dateien, an denen jeder grafische Editor unter Linux scheitert: ein Logfile
nach einem langen Lauf, ein CSV-Export, ein erzeugter JSON-Dump, eine große
generierte Quelldatei. Dafür gibt es heute `less` und sonst nichts Grafisches.

Der Umfangszuwachs verwässert das Ziel aus 014 also nicht, er stellt es dorthin,
wo es am stärksten trägt. Die Zielformel bleibt unverändert: **bei kleinen
Dateien nie schlechter, bei großen um Größenordnungen besser.**

## 4. Die Auswahl bei der Installation

Das Bild dahinter: eine Schale, und bei der ersten Installation wählt man die
Formate, die man haben will. Wer wenig wählt, bekommt einen leichteren Prozess.

Das Bild ist als **Architektur** richtig und als **Auslieferung** überflüssig.
Die Zahlen sagen, warum.

| Größe | Wert | Quelle |
| --- | ---: | --- |
| Ausgeliefertes Programm, entkleidet | 3,67 MiB | `target/release/hashline` |
| Das `.deb` insgesamt | 1,35 MiB | `packaging/build_deb.py` |
| Davon syntects Syntaxpaket im Programm | 360 KiB | `default_newlines.packdump` |
| PSS, leeres Fenster unter `cairo` | ~33 MiB | [Einschränkungen](../limitations.md) |
| PSS im Leerlauf unter `cairo`, 100 KiB | 43,0 MiB | [015](015-renderer-choice.md) |

Das **ganze** Programm ist kleiner als 4 MiB, und der Boden, auf dem es läuft,
liegt über 30 MiB. Dieser Boden ist GTK, Pango und der Renderer, nicht
Hashline. Ein Erzeuger für CSV, JSON oder Logs ist reiner Rust-Code ohne
Anlagen; er wiegt Zehnerkilobytes im Programm und nichts im Speicher, solange
er nicht läuft. Eine Installationsauswahl über solche Module verteilt einen
Posten, der gegen den Boden nicht messbar ist.

**Vorschlag: der Mechanismus wird gebaut, die Auswahl wird nicht angeboten.**
Formatmodule sind Cargo-Features, sodass ein Paket ohne ein Format überhaupt
baubar ist. Ausgeliefert wird genau ein Paket, und darin ist alles. Der
Nutzer trifft keine Entscheidung, die er nicht treffen kann, weil ihm die
Zahlen zur Entscheidung fehlen.

### Die Bedingung, unter der das kippt

Die Rechnung hängt daran, dass Textparser klein sind. Sie kippt, sobald ein
Format eine **Anlage** mitbringt. syntect liegt mit 360 KiB heute knapp unter
der Grenze und ist der einzige solche Posten. Tree-sitter wäre der Fall, der
sie überschreitet: dort ist jede Grammatik ein eigener erzeugter Parser, und
vierzig Sprachen sind Megabytes.

Die Regel dafür:

1. Ein Erzeuger ist **bedingungslos enthalten**, wenn er unter 500 KiB
   installierter Größe bleibt und im Leerlauf nichts belegt.
2. Ein Erzeuger kommt **hinter ein Feature und in ein eigenes Paket**, sobald er
   diese Grenze reißt. Erst dann bekommt der Nutzer eine Auswahl, und dann eine
   mit einer Zahl daran.
3. **Jeder Erzeuger lädt erst beim ersten Dokument seiner Art.** Das ist die
   eigentliche Eigenschaft, die „alles in einem" trägt. Sie ist am
   Syntaxhervorheber bereits erprobt: `SyntaxSet` liegt hinter einem
   `OnceLock` und wird auf dem Arbeitsthread geladen, der ihn zuerst braucht,
   nie beim Start. Was nie geöffnet wird, kostet nie.

Punkt 3 gilt ohne Ausnahme und ist die Abnahmebedingung für jedes Modul: der
Speicherboden des leeren Fensters darf durch ein zusätzliches Format nicht
messbar steigen.

## 5. Die Kandidaten

Die Spalte „Gestaltung" ist der eigentliche Aufwand. Ein Erzeuger ist ein
Wochenende; eine Darstellung, die dem Anspruch aus SPEC.md Abschnitt 1 genügt,
ist es nicht.

| Format | Erzeuger | Gestaltung | Hängt an |
| --- | --- | --- | --- |
| Klartext | trivial | keine, ein Absatzstrom | — |
| Quellcode | trivial, syntect liegt schon | Zeilennummernspalte, Gliederung aus Funktionen | — |
| man-/roff-Seiten | Untermenge, Markdown-nah | gering | — |
| Logdateien | Zeilenparser, Zeitstempel- und Stufenerkennung | Zeitstempelspalte, Stufenfarben, Filter | — |
| CSV/TSV | Trennzeichen- und Zitierregeln | Tabelle mit fixierter Kopfzeile, Spaltenausrichtung, waagerechtes Scrollen | **offene Befunde** |
| JSON/YAML/TOML | ein Parser je Format | Faltung, Baumeinzug, Gliederung aus Schlüsseln | Faltung ist neu |
| AsciiDoc, reStructuredText, Org | aufwendig, je eigene Grammatik | Markdown-nah | — |

**Klartext und Quellcode sind fast umsonst.** syntect ist als Abhängigkeit
bereits bezahlt, weil Codeblöcke sie brauchen. Eine `.rs`-Datei ist ein
`OpDocument` mit einem Codeblock, und der zerfällt seit
[013](013-oversized-blocks.md) in virtualisierte Teilblöcke. Deshalb steht das
Paar am Anfang: es prüft die These, ohne dass eine einzige Gestaltungsfrage neu
zu beantworten wäre.

**CSV ist gesperrt, bis zwei bekannte Befunde erledigt sind.** Breite Tabellen
blockieren den Hauptthread mit 53 ms gegen ein 16-ms-Budget, und überbreite
Tabellen sind beschnitten, weil internes waagerechtes Scrollen fehlt. Eine
CSV-Datei ist der Härtefall genau dieser beiden Lücken. CSV vor ihrer Behebung
zu bauen hieße, den schlechtesten Teil des Programms zum Hauptweg zu machen.

## 6. Was jedes Format zusätzlich klären muss

- **Gliederung.** Das Inhaltsverzeichnis ist heute die Überschriftenfolge. Bei
  Quellcode wären es Funktionen, bei JSON die obersten Schlüssel, bei einem Log
  nichts. Die Gliederung ist eine Angabe des Erzeugers, keine Eigenschaft des
  Grundprogramms.
- **Erkennung.** Endung zuerst, dann Inhalt. Der Rückfall ist immer Klartext,
  damit der Leser keine Datei verweigert.
- **Binäres.** Enthält der Anfang der Datei Nullbytes, wird nicht dargestellt,
  sondern ein Hinweis gezeigt. Megabytes Müll zu setzen ist kein Ergebnis.
- **Zuordnung.** Der Desktop-Eintrag bekommt weitere MIME-Typen. Die Regel aus
  SPEC.md Abschnitt 7 bleibt: eine bestehende Standardzuordnung wird nicht
  übernommen.
- **Messung.** Jedes Format bringt eigene Fixtures in die Suite aus 014 mit,
  klein und groß. Ein Format ohne Fixture gilt als nicht ausgeliefert.

Was **nicht** je Format zu klären ist, weil es am Textblob hängt und nicht an
der Grammatik: Suche, Auswahl, Zoom, Themes, Nachladen, Leseposition.

## 7. Reihenfolge

1. v1 nach SPEC.md abnehmen. Bis dahin ändert sich am Umfang nichts.
2. Den Schnitt festschreiben: `OpDocument` als Dokumentschnittstelle,
   Erzeugerauswahl, Erkennung, Klartext-Rückfall.
3. Klartext und Quellcode. Das ist v2.0 und die Probe aufs Exempel.
4. Danach, nach eigener Entscheidung und je mit Gestaltung und Messung: Logs,
   man-Seiten, JSON. CSV erst nach den Tabellenbefunden.
5. Weitere Auszeichnungssprachen zuletzt, wenn überhaupt.

## 8. Was dieses Dokument nicht behauptet

- **Die 3,67 MiB und die 360 KiB sind Dateigrößen, keine Speichermessung.** Was
  ein zusätzlicher Erzeuger im Leerlauf wirklich kostet, ist bisher nur für
  syntect belegt, und dort durch die Bauart, nicht durch eine Reihe.
- **Der Speicherboden von 33 MiB stammt aus dem leeren Fenster** und ist unter
  denselben Vorbehalten erhoben wie alles in [015](015-renderer-choice.md):
  Entwicklungsmaschine, kleines n, nicht die Referenzhardware.
- **Kein Aufwand hier ist geschätzt.** „Fast umsonst" bezieht sich auf die
  Abwesenheit neuer Abhängigkeiten und neuer Gestaltungsfragen, nicht auf eine
  Zeitangabe.
- **Die Namensfrage bleibt offen.** Hashline heißt nach dem Doppelkreuz. Für
  Quellcode und Logs trägt der Name noch; ob eine generische Bezeichnung jenseits
  von „Markdown Viewer" nötig wird, entscheidet 016 nicht.

## 9. Zu entscheiden

1. Gilt die Abgrenzung aus Abschnitt 2 als dauerhaft, also textförmig ja,
   paginiert nie?
2. Gilt Abschnitt 4, ein Paket mit allem statt einer Installationsauswahl, mit
   der 500-KiB-Regel als Umschaltpunkt?
3. Ist Klartext plus Quellcode der Umfang von v2.0?

# 012 — Rendering-Vertrag und Navigation (M1 und M2)

Status: Umgesetzt. Stand: 8. September 2026.
Vorgänger: [011-m0-foundation.md](011-m0-foundation.md),
[010-op-buffer-for-the-native-renderer.md](010-op-buffer-for-the-native-renderer.md).
Bezug: [SPEC.md](../../SPEC.md), Abschnitte 3, 6, 7, 8 und 14 (M1, M2).

Hält die Entscheidungen fest, die M1 und M2 verlangt haben, und benennt am Ende
was offen bleibt.

## 1. Ein Block ist nicht ein Pango-Layout

M0 hat einen Block als genau ein `pango::Layout` gesetzt. Der Rendering-Vertrag
verträgt das nicht: ein Listeneintrag braucht ein Zeichen, das nicht im
Dokument steht, und eine Tabellenzelle braucht eine eigene Umbruchbreite an
einer eigenen Position.

Ein Block ist deshalb jetzt eine Menge von **Pieces** — eines je
Listeneintrag, eines je Tabellenzelle, sonst genau eines — plus flacher
**Dekoration** dahinter: die Linie unter einer H2, der Balken am Zitat, die
Fläche unter einem Codeblock, Kopffläche und Linien einer Tabelle.

Damit fällt die Annahme, auf der M0 überall aufsetzte: dass ein Layout-Offset
und ein Dokument-Offset dieselbe Zahl sind. Ein Aufzählungszeichen steht nicht
im Dokument.

**Die Zuordnung wird nicht aufgegeben, sondern mitgeführt.** Jedes Piece trägt
eine [`TextMap`](../../crates/hashline/src/layout/textmap.rs): die Läufe, die
es aus dem Dokument übernommen hat. Eingefügte Dekoration hat einfach keinen
Lauf und damit keine Dokumentposition — was der Sachlage entspricht. Auswahl,
Treffertest, Suchmarkierung und Linkzuordnung gehen alle durch diese eine
Abbildung, statt dass jede ihre eigene Vorstellung davon hätte, wo etwas liegt.

Ein Piece kann außerdem **Bedienelement** sein — das „Kopieren" am Codeblock.
Ein solches Piece ist nie Ziel einer Auswahl oder eines Treffertests, und seine
TextMap ist leer. Das ist die technische Seite von SPEC Abschnitt 11:
Dokumentinhalt darf keine Bedienelemente imitieren, und umgekehrt ist ein
Bedienelement kein Dokumentinhalt.

## 2. Zwei Fehler, die der erste Render sichtbar gemacht hat

**Das Zeichen stand hinter dem Text.** Ein Listenzeichen ist erst bekannt, wenn
der Eintrag durchlaufen ist: ein geordneter Eintrag braucht seine Nummer, ein
Aufgabeneintrag ersetzt das Zeichen durch seinen Kasten, und die entsprechende
Operation kommt *nach* dem Öffnen des Eintrags. Das Zeichen wird deshalb
**vorangestellt**, mit Verschiebung der bereits gesammelten Abbildung, statt
den Text zu puffern.

**CLOSE wurde geraten.** Die erste Fassung entschied anhand dessen, was auf
einem anderen Stapel offen war, was gerade geschlossen wird. Bei
`<li>Text<ul><li>tief</li></ul></li>` schluckte das die äußere Ebene. Es gibt
jetzt einen ausdrücklichen Rahmenstapel (`List`, `Item`, `Span`, `Void`), der
jedem CLOSE das zuordnet, was tatsächlich schließt.

**Ein Softbreak ist ein Leerzeichen.** In HTML ist ein Zeilenumbruch im
Textinhalt Weißraum, und die Gestaltungsreferenz ist HTML. Ein Softbreak wird
deshalb als Leerzeichen gesetzt. Die Ersetzung ist **ein Byte für ein Byte**,
also bleibt jeder Offset unverändert — ein harter Umbruch behält seinen
Zeilenumbruch, und der Aufrufer kündigt ihn an.

## 3. Bilder nur dort, wo sie ohnehin stehen

Ein Absatz, dessen ganzer Inhalt ein Bild ist, wird ein Bildblock. So kommen
Bilder in der Praxis vor. Ein Bild **innerhalb** laufenden Textes müsste in die
Zeile geformt werden; Pango kann das, aber die Offset-Abbildung kann es noch
nicht mitgehen. Solche Bilder zeigen ihren Alternativtext — lesbar und ehrlich,
statt an der falschen Stelle zu erscheinen.

Das Layout liest **keine Dateien**. Es fragt über `ImageSource` nach der
intrinsischen Größe, um Platz zu reservieren; die Ansicht besitzt Cache,
Budget und Zugriffsregeln. Das Budget wird aus den **Kopfdaten** geprüft, bevor
ein Pixel dekodiert wird, und ein Pfad wird nur gelesen, wenn er nach
Kanonisierung im Dokumentverzeichnis bleibt — damit kann ein Symlink nicht
hinausführen (SPEC Abschnitte 7 und 11).

Die Entscheidung `glycin` gegenüber `gdk-pixbuf` aus SPEC Abschnitt 4 ist
**noch nicht getroffen**. Verwendet wird derzeit `gdk-pixbuf` über
`gdk::Texture`. Budget und Verzeichnisgrenze liegen an einer Stelle, sodass ein
Wechsel des Loaders sie nicht berührt.

## 4. Die Suche legt keine zweite Kopie des Dokuments an

Der naheliegende Weg wäre, den Textblob einmal klein zu schreiben und eine
Offset-Tabelle daneben zu halten. Für die 10-MiB-Fixture kostet das weitere
10 MiB Text plus 40 MiB Offsets — das **gesamte** Speicherbudget aus SPEC
Abschnitt 9, ausgegeben für eine Funktion, die meistens ruht.

Stattdessen wird je Kandidatenstelle mit Faltung im Vergleich gesucht. Das
behandelt Groß-/Kleinschreibung und jeden Akzentbuchstaben.

**Die aufweitenden Faltungen werden nicht ausgeführt**: `ß` findet kein `ss`,
`ﬁ` kein `fi`. Sie ändern die Bytelänge und würden genau die Tabelle
erzwingen, die hier vermieden wird. Für einen deutschen Leser ist das eine
echte Einschränkung, und sie ist als Test festgehalten statt versteckt — auch
damit sie nicht versehentlich zu einem Präfixtreffer wird.

## 5. Syntect ohne Syntect-Theme

Verwendet wird nur der **Parser** von `syntect`, nicht seine Theme-Maschinerie.
Die Palette trägt genau drei Syntaxfarben — Schlüsselwort, Zeichenkette, Zahl
—, also müsste ein Sublime-Farbschema ohnehin auf diese drei eingeschmolzen
werden. Die Zuordnung geschieht direkt über die stabilen Präfixe der
TextMate-Scope-Konvention (`keyword`, `storage`, `string`, `constant.numeric`).
Damit bleiben die Design-Tokens die einzige Farbquelle und es kommt keine
Theme-Datei in den Build — dasselbe Prinzip, das
[003-highlighting.md](003-highlighting.md) für highlight.js festgelegt hat.

`regex-fancy` statt `regex-onig`, damit keine C-Bibliothek dazukommt.
Hervorgehoben wird je sichtbarem Codeblock auf einem Arbeitsthread, das
Ergebnis je Block zwischengespeichert, Blöcke über 128 KiB bleiben unkoloriert
(SPEC Abschnitt 10). Eine unbekannte Sprache ergibt lesbaren, unkolorierten
Code — nie eine Vermutung.

## 6. Beobachtet wird das Verzeichnis, nicht die Datei

Ein Editor, der atomisch speichert, schreibt eine temporäre Datei und benennt
sie über das Ziel. Das zerstört die Inode, an der eine Dateibeobachtung hängt.
Beobachtet wird deshalb das **Elternverzeichnis** und auf den einen Namen
gefiltert. Ereignisse kommen in Schüben; sie werden zusammengefasst, indem die
Warteschlange geleert und 150 ms Ruhe abgewartet wird.

Ein **Digest** des Quelltexts verhindert, dass ein Ereignis ohne inhaltliche
Änderung ein Neurendern kostet. Scheitert die Beobachtung, bleibt manuelles
Nachladen verfügbar; das ist kein Fehlerfall, der den Start verhindert.

Als **Leseanker** dient die Überschrift, unter der der Leser steht, plus der
Abstand von ihr zum oberen Viewportrand. Eine Überschrifts-ID ist der
stabilste verfügbare Griff: sie leitet sich aus dem Text ab, bleibt also gleich,
wenn sich Absätze darüber ändern. Fällt sie weg, dient der Blockindex als
Rückfall, danach der Dokumentanfang.

## 7. Einstellungen scheitern nicht am fehlenden Schema

`data/de.kalendium.Hashline.gschema.xml` existiert jetzt; installiert wird es
mit M3. Bis dahin findet ein Entwicklungsbaum das Schema nur über
`GSETTINGS_SCHEMA_DIR`. Der Speicher prüft deshalb erst, ob das Schema
vorhanden ist, und arbeitet sonst **wirkungslos** weiter statt zu scheitern —
SPEC Abschnitt 7 verlangt ausdrücklich, dass fehlende oder beschädigte
Einstellungen den Start nicht blockieren. Der Test dazu überspringt sich selbst
mit Meldung, wenn kein Schema da ist, statt auf nichts hin zu bestehen.

## 8. Was offen bleibt

- **Eigenscroll in Blöcken.** Ein Codeblock oder eine Tabelle, die breiter ist
  als die Lesespalte, wird auf die Spalte **beschnitten**. Die Breite, die der
  Inhalt will, ist bekannt (`content_width`); die Eingabebehandlung dafür
  fehlt. Das Dokument scrollt nie waagerecht, aber der überstehende Teil ist
  derzeit nicht erreichbar. Das ist die auffälligste Lücke im Rendering-Vertrag.
- **Fußnoten, Definitionslisten und `details`/`summary`** erhalten noch keine
  eigene Gestaltung; sie werden als Absätze gesetzt.
- **Bilder in laufendem Text**, siehe Abschnitt 3.
- **Barrierefreiheit.** Das Dokumentwidget meldet noch nicht die Rolle
  `Document` und noch keinen Text über `GtkAccessibleText`. Die
  GTK-Mindestversion ist dafür bereits auf 4.14 gesetzt
  ([011](011-m0-foundation.md), Abschnitt 2), die Umsetzung fehlt.
- **Die vier M0-Risiken bleiben interaktiv unbestätigt.** Bildschirmaufnahmen
  sind auf dieser Maschine untersagt; Frametimes bei 60 und 120 Hz sind
  ungemessen, und damit ist die Renderer-Wahl aus
  [011](011-m0-foundation.md) Abschnitt 4 weiter offen.
- **Die extrem lange Zeile** kostet weiter 190 ms
  ([011](011-m0-foundation.md), Abschnitt 5).
- **Referenzaufnahmen und Referenzmaschine** fehlen weiter; siehe
  [docs/design/README.md](../design/README.md).

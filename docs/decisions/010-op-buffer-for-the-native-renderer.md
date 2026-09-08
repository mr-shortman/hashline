# 010 — Der Op-Buffer für den nativen Renderer

Status: Umgesetzt. Stand: 8. September 2026.
Vorgänger: [009-native-renderer.md](009-native-renderer.md),
[008-parser-reference.md](008-parser-reference.md).
Setzt [SPEC.md](../../SPEC.md), Abschnitt 6 („Änderungen am Op-Buffer") um.

Diese Entscheidung hält fest, wie die vier in SPEC Abschnitt 6 verlangten
Änderungen konkret aussehen, und dokumentiert drei Punkte, die die
Spezifikation offen gelassen hat.

## 1. Ausgangslage: der Parser hatte keine eigenen Tests

Die 652 CommonMark-Beispiele liefen über TypeScript gegen den
WebAssembly-Build und verglichen DOM-Bäume ([008](008-parser-reference.md)).
Mit der Node-Toolchain verschwindet dieses Netz — und zwar genau in dem Moment,
in dem der Encoder invasiv geändert wird. Ohne Ersatz wäre die Migration eine
Änderung ohne Prüfung gewesen.

**Zuerst das Netz, dann die Änderung.** Die Konformitätsprüfung ist nach Cargo
portiert, bevor eine Zeile am Encoder geändert wurde: `crates/markdown/tests/`.
Das Verfahren aus 008 bleibt erhalten — die Operationen werden zu HTML
zurückgespielt, beide Seiten durch denselben HTML-Parser geschickt und die Bäume
nach den Normalisierungen verglichen, die 008 erlaubt. Nur der Wirt ist ein
anderer. `html5ever` ist dafür Dev-Dependency; das Produkt bleibt frei davon.

Beim Portieren fiel eine Falle auf, die erwähnt sei, weil sie beinahe ein
grünes Ergebnis vorgetäuscht hätte: `RcDom` gibt seinen Baum beim Verwerfen
frei. Ein `Handle`, der die `RcDom` überlebt, ist leer — und ein Vergleich
zweier leerer Bäume ist immer wahr. Der erste Lauf meldete deshalb 652 von 652
bestandenen Beispielen, ohne irgendetwas verglichen zu haben. Der Test prüft
seitdem zusätzlich, dass er Unterschiede in Text, Element und Attribut
überhaupt bemerkt.

## 2. Die vier Änderungen

1. **UTF-8-Byteoffsets.** Die UTF-16-Zählung existierte, damit JavaScript
   `substring` ohne Übersetzungstabelle verwenden konnte. Alle Offsets und
   Längen in `ops`, `attrs`, `sections`, `headings` und `blocks` sind jetzt
   Byteoffsets. Die Hilfsfunktion `utf16_len` entfällt; Längen sind `str::len`.
   Der Test schneidet die Blobs mit genau diesen Offsets — ein `&str`-Schnitt
   neben einer Zeichengrenze paniert, die Prüfung ist also die Kodierung selbst.
2. **Pro Block ein Textbereich.** Siehe Abschnitt 3.
3. **`strings` und `text` bleiben** zwei Blobs, unverändert.
4. **Tag- und Attribut-IDs bleiben** unverändert.

## 3. Offen gelassen: wie der Textbereich je Block getragen wird

SPEC Abschnitt 6 verlangt „pro Block ein Textbereich", ohne die Darstellung
festzulegen. Eine Operation trägt vier Wörter und hat keinen Platz mehr.

**Gewählt: ein eigenes Feld `blocks`**, fünf Wörter je Block — Tag, opStart,
opCount, textStart, textLen. Das ist zugleich genau die Struktur, die SPEC
Abschnitt 5 für den **Blockplan** beschreibt: „für jeden Block Art, Textbereich
und eine geschätzte Höhe". Die Höhe kommt vom Layout dazu, alles andere steht
schon im Puffer. Der Blockplan ist damit ein Durchlauf über `blocks`, ohne die
Operationen überhaupt anzufassen.

Ein Block ist ein **Flusselement der obersten Ebene**. Eine verschachtelte Liste
öffnet keinen eigenen Block, und eine Tabelle ist ein Block, gleich wie viele
Zeilen sie hat — die Ansicht scrollt eine Tabelle innerhalb ihres Blocks
(SPEC Abschnitt 3). Damit ist die Blockliste dieselbe Liste, über die
virtualisiert wird.

**Trennzeichen gehören keinem Block.** Zwischen zwei Blöcken steht ein `\n` im
Textblob, damit ein Suchtreffer nicht über eine Blockgrenze läuft. Der Bereich
eines Blocks beginnt an seinem ersten eigenen Textlauf, nicht am Trennzeichen
davor; sonst wäre jeder Blocktext um ein Zeichen versetzt und jede Auswahl an
jedem Blockanfang um eins daneben. Die Bereiche sind dadurch lückenhaft, aber
überschneidungsfrei und geordnet — das ist, was die Zuordnung braucht.

## 4. Offen gelassen: wie rohes HTML als Quelltext aussieht

SPEC Abschnitt 6 verlangt „als Quelltext dargestellt, monospace und dezent
abgesetzt". Als Operationen ausgedrückt:

- Ein roher HTML-**Block** wird `pre > code` mit der Klasse `raw-html`.
- Ein rohes **Inline**-Fragment wird `code` mit derselben Klasse, innerhalb des
  umgebenden Absatzes — der Absatz bricht nicht auf.
- Die Klasse unterscheidet fremdes Markup vom Codeblock des Autors, damit die
  Ansicht es dezenter setzen kann, ohne den Unterschied raten zu müssen.
- `OpDocument::raw_html` meldet, ob das Dokument überhaupt rohes HTML enthielt.
  Das ist die Grundlage für den „einmaligen ruhigen Hinweis", den SPEC
  verlangt: ein Zähler im Dokument statt einer Suche durch die Operationen.

Der Text des Markups liegt im Textblob und ist damit durchsuchbar — es ist
Text, und es verhält sich wie Text.

**Was das kostet, in Zahlen:** 72 der 652 CommonMark-Beispiele enthalten rohes
HTML und weichen jetzt ab. Die übrigen 580 stimmen exakt mit der Spezifikation
überein. Der Test führt beide Gruppen getrennt und **pinnt die 72**: driftet ein
Beispiel zusätzlich auf den Quelltextpfad, schlägt er fehl. Das ist die
Funktionsminderung, die 009 Abschnitt 4 bewusst eingegangen ist — sie ist damit
gemessen statt behauptet.

## 5. Folgeänderungen

- **`SECTION_FALLBACK`, die HTML-Erzeugung und `boundary.rs` entfallen.** Mit
  ihnen entfällt ein **vollständiger zweiter Durchlauf** über das Dokument: der
  Vorabtest, ob rohes HTML einen Container offen lässt und deshalb eine
  Abschnittsgrenze verbietet. Nichts stromabwärts parst noch HTML, also kann
  keine Grenze mehr einen Baum verändern. Für die 10-MiB-Fixture ist das
  ersparte Arbeit, kein Umbau.
- **Abschnitte tragen sechs statt neun Wörter**: opStart, opCount, hashLow,
  hashHigh, textStart, textLen. Die Wörter für Flags, htmlOffset und htmlLen
  hatten nur den Fallback zu beschreiben.
- **`packet.rs` bleibt vorerst**, obwohl es die WebView bedient. Es fällt mit
  `src-tauri`, nachdem M0 abgenommen ist (SPEC Abschnitt 13), und wird bis
  dahin nicht mehr gepflegt: die Frontend-Decoder erwarten neun Wörter je
  Abschnitt und UTF-16-Offsets und sind mit dieser Änderung überholt.

## 6. Auswirkung auf die Reihenfolge der Migration

SPEC Abschnitt 13 will den alten Stand erhalten, bis M0 abgenommen ist, damit
Vergleichsmessung und visueller Abgleich möglich bleiben. Diese Änderung macht
die WebView-Fassung **darstellungsseitig überholt** — sie zeigte rohes HTML
gerendert, die neue Fassung zeigt es als Quelltext, und die Offsets stimmen
nicht mehr überein.

Der alte Stand bleibt trotzdem erreichbar: Commit `482a33c` trägt das Tag
`webview-final`. Die Referenzaufnahmen aus SPEC Abschnitt 12 sind daraus
weiterhin herstellbar und **noch nicht erstellt** — siehe
[docs/design/README.md](../design/README.md). Sie sind vor M1 nachzuholen, nicht
vor M0: M0 entkräftet technische Risiken und braucht den optischen Abgleich
noch nicht.

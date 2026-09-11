# 008 — Parserreferenz: CommonMark statt marked, DOM-Vergleich statt Stringvergleich

Status: Entscheidung getroffen, Umsetzung mit P2.3. Stand: 8. September 2026.
Vorgänger: der Performancepfad des WebView-Stands, Paket P2.3 (Tag
`webview-final`). Diese Entscheidung war Voraussetzung dafür, dass für den
Rust-Parser überhaupt Code entsteht; sie gilt für den nativen Stand unverändert
weiter.

## 1. Warum diese Entscheidung nötig ist

Die 652 CommonMark-Beispiele in `tests/fixtures/commonmark-0.31.2.json` sind
heute **kein Konformitätstest**. Sie vergleichen zwei Wege durch _dieselbe_
marked-Ausgabe miteinander:

- `tests/sections.test.ts` prüft, dass der abschnittsweise Aufbau zeichengleich
  dasselbe HTML ergibt wie der Aufbau am Stück (`root.innerHTML` gegen
  `sanitizeContent(...).html`),
- und seit P2.2 zusätzlich, dass der Op-Buffer-Replay knotengleich dasselbe
  Ergebnis liefert wie der bereinigte HTML-Pfad.

Referenz ist in beiden Fällen marked. Das Feld `html` der Beispiele — die
Ausgabe, die die CommonMark-Spezifikation _vorschreibt_ — wird nirgends gelesen.

Mit P2.3 verschwindet marked. `pulldown-cmark` ist CommonMark-konform, erzeugt
aber nicht zeichengleiches HTML: andere Attributreihenfolge in Randfällen, andere
Behandlung von Zeilenumbrüchen zwischen Blockelementen, andere Escape-Formen für
denselben Text. Ein Stringvergleich gegen eine neue Referenz würde deshalb an
Kosmetik scheitern, nicht an Semantik. Ohne Entscheidung würde die naheliegende
Reaktion darin bestehen, die Erwartungswerte an die neue Ausgabe anzupassen —
also den Test seiner Aussage zu berauben.

## 2. Entscheidung

**Die Referenz wird die CommonMark-Spezifikation.** Verglichen wird gegen das
`html`-Feld der Beispieldatei, nicht mehr gegen eine zweite Ausgabe desselben
Parsers.

**Verglichen wird der DOM, nicht der String.** Beide Seiten werden in ein
`<div>` gebaut, `normalize()` fasst benachbarte Textknoten zusammen, und
`isEqualNode` entscheidet. Damit sind Elementstruktur, Attributmenge,
Attributwerte und Textinhalt verbindlich; Attributreihenfolge, Whitespace
zwischen Blockelementen und die Wahl der Escape-Form sind es nicht.

**Verglichen wird die Parserausgabe, nicht das Produkt.** Der Vergleich sitzt
vor der Inhaltspolitik. Die Politik (`classifyUrl`, blockierte Bilder,
`data-link` statt `href`, `disabled` an Checkboxen) verändert das Dokument
absichtlich gegenüber der Spezifikation; sie hat mit
`tests/policy-fragment.test.ts` und den Bereinigungstests eigene Verträge, die
unverändert bleiben. Für den Spezifikationsvergleich wird der Op-Buffer deshalb
mit `structuralSink` abgespielt.

**Zwei bewusst ausgenommene Unterschiede**, jeweils an genau einer Stelle im
Test normalisiert und hier begründet:

1. **Überschriften-IDs.** Der Parser vergibt `id="doc-…"`; die Spezifikation
   kennt keine IDs. Der Test entfernt vor dem Vergleich `id`-Attribute an
   `h1`–`h6`, die mit `doc-` beginnen. Die Slugregel selbst bleibt in
   `tests/markdown.test.ts` einzeln und bitgenau geprüft (siehe Abschnitt 4).
2. **Rohes HTML.** Abschnitte mit rohem HTML sind nicht als Operationen
   ausdrückbar und behalten laut 007 den DOMPurify-Pfad. Für sie liefert der
   Parser den HTML-Text des Abschnitts mit; der Test setzt ihn per `innerHTML`
   und vergleicht das Ergebnis wie jedes andere. Der Vergleich prüft dort also
   den Parser, nicht die Bereinigung — die Bereinigung ist Sache der
   Politiktests.

## 3. Wo der Test läuft — eine Parserimplementierung, zwei Ziele

Der Vergleich braucht einen DOM (`isEqualNode`), der Parser läuft künftig in
Rust. Beides zugleich ist nur erreichbar, wenn **derselbe** Rust-Code auch im
Browser läuft. Der Parser wird deshalb als eigenständige Bibliothek
(`src-tauri/markdown`) geschrieben und in zwei Ziele übersetzt:

| Ziel                     | Verwendung                                         |
| ------------------------ | -------------------------------------------------- |
| natives `src-tauri`      | Desktop: `read_document` liefert den Op-Buffer mit |
| `wasm32-unknown-unknown` | Browser-Vorschau, Playwright und Vitest            |

Es gibt danach **eine** Parserimplementierung. Die Alternative — den
Spezifikationsvergleich als Rust-Test gegen einen HTML-Parser in Rust zu führen —
wurde verworfen: sie hätte eine zweite HTML-Baumsemantik (html5ever) als
Schiedsrichter eingeführt, während der Produktionspfad die Baumsemantik von
WebKit verwendet.

Die WebAssembly-Datei wird über einen dynamischen Import geladen und ist damit
ein eigener Chunk. Der Desktop-Pfad fordert sie nie an; er bekommt seine Puffer
über IPC.

## 4. Was diese Umstellung **nicht** lockern darf

Der DOM-Vergleich ist toleranter als der Stringvergleich. Die folgenden
Eigenschaften verlieren dadurch ihre Absicherung und bekommen deshalb eigene,
strengere Tests:

- **Slugregel bitgenau.** `slugBase` (NFKC, Kleinschreibung, `\p{L}\p{N}\s_-`,
  Leerzeichen/Unterstriche zu `-`, Fallback `section`), Präfix `doc-`,
  Deduplizierung `-1`, `-2` …. Geprüft wird gegen dieselbe in JavaScript
  formulierte Regel, angewandt auf dieselben Überschriftentexte — nicht gegen
  eine in Rust nachgeschriebene Erwartung. `\s` ist dabei die JavaScript-Menge
  (inklusive `U+FEFF`, ohne `U+0085`), nicht `Unicode White_Space`.
- **Abschnittsgrenzen.** Ein Abschnittsschnitt darf das Ergebnis nicht
  verändern: die Konkatenation aller Abschnitte muss knotengleich dem
  ungeschnittenen Dokument sein, einschließlich der Fälle mit rohem HTML über
  Tokengrenzen hinweg (`isClosedHtml`).
- **Fallbackrate.** Die bestehende Schranke bleibt: Abschnitte dürfen nicht
  stillschweigend auf den DOMPurify-Pfad zurückfallen.
- **Attributwertpolitik.** Unverändert in `tests/policy-fragment.test.ts`.

## 5. Abschnittsidentität

Der Viewport erkennt unveränderte Abschnitte heute am gleichen HTML-String. Ohne
HTML-String tritt an dessen Stelle ein 64-Bit-Hash über die Operationen, die
Attribute und die referenzierten Zeichen des Abschnitts, den der Parser
mitliefert. Eine Kollision würde einen fremden, bereits gerenderten Abschnitt
wiederverwenden; bei den hier auftretenden Abschnittszahlen (unter 10⁴ je
Dokument) liegt diese Wahrscheinlichkeit unter 10⁻¹¹. Das wird in Kauf genommen,
weil die Alternative — den vorigen Puffer vollständig zum Vergleich zu halten —
bei 10 MiB zweistellige Megabyte zusätzlich belegen würde.

## 6. Folgen

- `tests/sections.test.ts` vergleicht nicht mehr zwei marked-Wege, sondern
  Parserausgabe gegen Spezifikation.
- Abweichungen von der Spezifikation, die durch die eingeschalteten
  GFM-Erweiterungen (Tabellen, Strikethrough, Tasklists, Footnotes) entstehen,
  werden **nicht** durch angepasste Erwartungswerte versteckt, sondern in
  Abschnitt 7 einzeln aufgeführt und begründet.
- `tests/markdown.test.ts`, `tests/tokenizer.test.ts` und `tests/worker.test.ts`
  verlieren mit marked, dem Tokenizer und dem Worker ihren Gegenstand.
  Der Inhaltsvertrag aus `markdown.test.ts` (Slugs, Politik, `classifyUrl`)
  bleibt und wandert auf den neuen Parser.

## 7. Gemessene Abweichungen von der Spezifikation

**Alle 652 Beispiele bestehen den Vergleich.** Es gibt keine offene Abweichung.
Das war beim ersten Lauf nicht so; die 42 Abweichungen zerfielen in vier
Gruppen, drei davon eigene Fehler, eine eine zulässige Normalisierung:

| Fälle | Ursache                                                                                             | Behandlung                                                                                                            |
| ----: | --------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
|    15 | URLs unkodiert übernommen (`/föö` statt `/f%C3%B6%C3%B6`, Leerzeichen, `\`, `` ` ``, `[`, `]`)      | behoben: `escape_href` mit der Zeichenmenge der CommonMark-Referenz, dieselbe Wirkung wie marked's `encodeURI` vorher |
|    11 | fehlender Zeilenumbruch nach `<br>`                                                                 | behoben: der Umbruch wird als Text emittiert, er gehört zum Dokumenttext                                              |
|    10 | Zeilenumbruch zwischen Listeneintragstext und geschachteltem Block (`<li>foo\n<ul>`)                | Normalisierung: Whitespace an Blockgrenzen, siehe unten                                                               |
|     7 | leere `<a>`/`<em>` vor einem Bild — verschachtelte Auszeichnung im Alternativtext erzeugte Elemente | behoben: innerhalb eines Bildes wird nur noch Text gesammelt                                                          |
|     2 | E-Mail-Autolinks ohne `mailto:`                                                                     | behoben                                                                                                               |

Die eine verbliebene Normalisierung neben den Überschriften-IDs ist **Whitespace
an Blockgrenzen**: der HTML-Renderer schreibt Zeilenumbrüche zwischen
Blockelementen zur Lesbarkeit, die Operationen tragen sie nicht. Der Test trimmt
Whitespace nur dort, wo ein Textknoten an ein Blockelement oder an den Rand
seines Blockelternteils grenzt — Whitespace **zwischen Inline-Elementen** und
innerhalb von `pre` und `code` bleibt unangetastet und ist damit weiterhin
verbindlich. Beide Normalisierungen stehen an genau einer Stelle in
`tests/sections.test.ts`.

Zwei bewusste Abweichungen betreffen Konstrukte, die die
CommonMark-Beispielmenge nicht enthält, und sind deshalb hier zu nennen statt
gemessen zu werden:

1. **Tabellenausrichtung als `align`, nicht als `style`.** Der HTML-Renderer von
   `pulldown-cmark` schreibt `style="text-align: left"`. `style` steht nicht auf
   der Attributliste und ist im Op-Buffer nicht ausdrückbar; `align` trägt
   dieselbe Information und stand schon bisher auf der Liste.
2. **Fußnotennummern in Fallback-Abschnitten.** Der HTML-Renderer zählt je
   Aufruf; ein Abschnitt mit rohem HTML wird einzeln gerendert und beginnt
   deshalb wieder bei 1. Dokumente mit rohem HTML _und_ Fußnoten sind der
   einzige betroffene Fall.

## Nachtrag: Anker wie bei GitHub (11. September 2026)

Die Slugregel aus Abschnitt 4 ist nicht mehr bitgenau die von `parser.ts`. Sie
faltete Unterstriche und Leerzeichenfolgen zu einem einzigen `-`, sodass aus
„Kopf_zeile“ `doc-kopf-zeile` wurde und ein für GitHub geschriebener Link
`#kopf_zeile` ins Leere ging. Seitdem gilt, was GitHub mit dem Text einer
Überschrift macht: `\p{L}\p{M}\p{N}\p{Pc}`, `-` und Leerraum bleiben, jedes
Leerraumzeichen wird zu einem eigenen `-`. NFKC, Kleinschreibung, Trimmen, der
Fallback `section`, das Präfix `doc-` und die Deduplizierung bleiben. Einen
Fragmentlink löst der Leser in dieser Reihenfolge auf: die ID wie geschrieben,
mit `doc-` davor, und zuletzt der Slug des Fragments selbst, damit auch
`#Kopf_Zeile` trifft (`crates/hashline/src/outline/mod.rs`).

Fußnoten tragen seitdem die ID `fn-<label>`, Verweis und Definition mit derselben
Schreibweise, auch wenn das Label im Dokument in unterschiedlicher Groß- und
Kleinschreibung steht — `pulldown-cmark` ordnet sie ohne Rücksicht darauf zu. Der
Parser liefert die Definitionen als eigene Tabelle `anchors` aus, damit ein
Klick auf eine Fußnote ihren Block findet, ohne dass die Überschriften dafür
herhalten müssen.

Eine gespeicherte Leseposition unter einer Überschrift, deren ID sich dadurch
geändert hat — sie enthält `_` oder doppelten Leerraum —, findet diese nicht
wieder. Gespeichert sind nur Überschrift und Abstand, kein Blockindex, also
öffnet die Datei dann nahe ihrem Anfang.

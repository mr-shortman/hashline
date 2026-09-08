# 007 — Performancepfad: Plattformentscheidung und Arbeitsauftrag Phase 1/2

Status: Entscheidung getroffen, Umsetzung offen. Stand: 8. September 2026.
Vorgänger: [004-rendering-gate.md](004-rendering-gate.md),
[Übergabestand](../performance-progress.md), [Benchmarkbericht](../../benchmarks/REPORT.md).

Dieses Dokument ist ein **Arbeitsauftrag**. Es ist so geschrieben, dass ein anderer
Agent die Pakete einzeln und in der angegebenen Reihenfolge umsetzen kann.

## 1. Messung, die die Entscheidung begründet

Alle Werte auf der Entwicklungsmaschine (Ubuntu 26.04, Ryzen 9 9900X, Wayland,
WebKitGTK 2.52.6), gemessen am 8. September 2026 über `/proc/<pid>/smaps_rollup`
(PSS, rekursive Prozessgruppe) beziehungsweise über die Zeit bis zum Spawn des
`WebKitWebProcess`.

| Stack                                  | PSS im Leerlauf | Anmerkung                                               |
| -------------------------------------- | --------------: | ------------------------------------------------------- |
| WebKitGTK `MiniBrowser`, `about:blank` |   **139,2 MiB** | UI 66,2 / Network 14,2 / WebProcess 58,8                |
| GTK4 `gnome-calculator`                |       117,1 MiB | natives GTK4, kein WebView                              |
| GTK4 `gnome-text-editor`               |       148,8 MiB | natives GTK4 mit Textwidget                             |
| Ghostty                                |       155,1 MiB | Zig, eigenes GPU-Textrendering                          |
| Hashline, 100-KiB-Datei                | 207,9–237,7 MiB | aus `final-stability.json` / `sectioned-stability.json` |

| Startmessung                                            |       Wert |
| ------------------------------------------------------- | ---------: |
| `exec` → `WebKitWebProcess` (MiniBrowser, Median aus 5) | **162 ms** |
| Hashline → Frontend-Modul bereit                        |     731 ms |
| Hashline → erster Textframe                             |   1.219 ms |

**Einschränkung der Messung:** PSS verteilt geteilte Bibliotheksseiten anteilig.
Läuft nur eine WebKit-Anwendung, trägt sie `libwebkit2gtk` allein. Die Zahlen sind
Einzelstichproben nach 4–5 s Beruhigung, keine n=30-Reihe, und stammen von einer
Maschine mit diskreter GPU. Sie sind belastbar genug für die Größenordnung und
für die Richtungsentscheidung, nicht für eine Abnahme.

### Was daraus folgt

1. **Ein Toolkit-/Plattformwechsel löst das Speicherproblem nicht.** Jeder
   GUI-Stack auf dieser Maschine liegt zwischen 117 und 155 MiB, bevor er Inhalt
   anzeigt. Ghostty mit vollständig eigenem GPU-Textrenderer liegt über der leeren
   WebView. Die 200-MiB-Grenze ist keine WebKit-Untergrenze, sondern eine
   Desktop-GUI-Untergrenze. Hashline legt **+70 bis +100 MiB** auf diesen Boden —
   für ein 100-KiB-Dokument mit 16.187 Parser-DOM-Knoten. Dieser Aufschlag ist
   Anwendungscode und damit adressierbar.
2. **Der Start ist zu etwa 80 % Anwendungscode.** 162 ms Plattformboden gegen
   731 ms bis zum bereiten Frontend.
3. **Die bisherige Optimierung lag auf der falschen Ebene.** Der Parser wurde von
   1.160 auf 800 ms verbessert; Parsing ist rund 6 % der 10-MiB-Gesamtkosten.
   Die vier strukturellen Engpässe sind unverändert im Code (siehe Phase 1).

### Entscheidung

**Der Web-Renderer bleibt.** Ein Rewrite auf ein natives Toolkit wird nicht
begonnen, weil die beiden Metriken, die ihn motiviert hätten — Speicher und Start —
nach dieser Messung überwiegend anwendungsseitig sind.

**Tauri bleibt vorerst, ist aber nicht die Cross-Platform-Antwort.** Tauri liefert
drei Engines (WebView2/WKWebView/WebKitGTK); der Linux-Fokus liegt damit auf der
langsamsten. Electron/CEF scheidet wegen 200–300 MiB Grundlast aus. Ein nativer
Rust-Stack (winit + wgpu + `parley`/`cosmic-text` + AccessKit) bleibt der einzige
kohärente Endpunkt für „maximal schlank **und** echt cross-platform"; realistisch
erreichbar sind dort ~60–100 MiB und ~100 ms Start gegenüber ~160 MiB und ~400 ms
im besten WebView-Fall. Der Abstand rechtfertigt heute keine eigene Layout-Engine
(Blockfluss, Tabellen, Selektion über Blockgrenzen, Bidi, Hit-Testing, A11y).

**Phase 2 ist deshalb so geschnitten, dass sie diese Entscheidung umkehrbar macht:**
Parser, Suchindex und Layout-Metriken wandern nach Rust hinter eine
Op-Buffer-Schnittstelle. Danach ist der Renderer ein austauschbares Modul, und die
Frage WebView-vs-nativ wird von einer Wette zu einer Implementierungsentscheidung.

## 2. Grundregeln für die Umsetzung

- **Reihenfolge einhalten.** P1.1 bis P1.5 sind unabhängig voneinander, aber
  P1.6 (Diagnose) muss vor Phase 2 abgeschlossen sein. Phase 2 setzt Phase 1 voraus.
- **Ein Arbeitspaket pro Commit.** Jedes Paket hat unten ein eigenes Abnahmekriterium.
- **Keine Sicherheitsregression.** Jede Änderung an `policy.ts` oder am
  Renderpfad muss die bestehenden Bereinigungstests unverändert bestehen. Wo eine
  Prüfung verschoben wird, ist im Commit zu begründen, wo sie stattdessen greift.
- **Diese Tests müssen grün bleiben:** `tests/markdown.test.ts`,
  `tests/sections.test.ts` (CommonMark-Vergleich für Parser _und_ bereinigten
  Abschnittspfad), `tests/preferences-search.test.ts`, `tests/controller.test.ts`,
  `tests/worker.test.ts`, `tests/tokenizer.test.ts`, `tests/document-packet.test.ts`
  sowie die Playwright-Suite unter `tests/ui/`.
- **Messen vor und nach jedem Paket** mit derselben eingefrorenen Fixture-Menge.
  Referenzreihe ist `benchmarks/results/optimized-distinct-open.json`
  (je 30 klein/mittel, 17 groß). Neue Reihen unter neuem Namen ablegen,
  bestehende Rohdaten und eingefrorene Builds **nicht überschreiben**.
- **Keine Zielverschiebung.** Budgets aus `SPEC.md` Abschnitt 9 bleiben unverändert.

## 3. Phase 1 — die unangetasteten Engpässe

Erwartete Gesamtwirkung: vollständiger 10-MiB-Aufbau deutlich unter dem aktuellen
Median von 13.561 ms, Scroll näher an der 99-%-Grenze, Speicheraufschlag über dem
Plattformboden spürbar kleiner. Kein Paket ändert die Architektur.

---

### P1.1 — `indexText` aus dem Renderpfad entfernen

**Problem.** `src/features/document/DocumentViewport.tsx:256` baut den Suchindex
für jeden Abschnitt **während** des Einfügens, innerhalb der 18-ms-Renderschleife —
auch wenn nie gesucht wird.

```ts
// DocumentViewport.tsx:256, aktuell
indexes.current.set(shells[i], indexText(shells[i]));
```

**Lösung.** Zeile ersatzlos streichen. Der Suchpfad baut fehlende Indizes bereits
faul nach:

```ts
// DocumentViewport.tsx:688-692, unverändert bestehend
let index = indexes.current.get(shell);
if (!index) {
  index = indexText(shell);
  indexes.current.set(shell, index);
}
```

**Warum das keine Suche verlangsamt.** Die Suche wartet ohnehin auf den
vollständigen Aufbau (`DocumentViewport.tsx:679`:
`if (query && root.current?.dataset.renderState !== 'complete') return;`).
Die Kosten verschieben sich auf die _erste_ Suche und entfallen für alle
Dokumente, in denen nicht gesucht wird.

**Fallstricke.**

- Wiederverwendete Abschnitte (`retainedSections`) behalten ihren Index in der
  `WeakMap`, weil die Shell-Knoten überleben. Das bleibt korrekt.
- `indexes.current.delete(shell)` nach dem Syntax-Highlighting
  (`DocumentViewport.tsx:445`) bleibt notwendig und unverändert.

**Abnahme.** `hashline.open-to-complete` bei 10 MiB sinkt messbar.
`tests/preferences-search.test.ts` und die Suchtests in `tests/ui/` grün.

---

### P1.2 — `indexText` einpassig ohne `closest()`

**Problem.** `src/features/search/index.ts:11-37` ruft pro Textknoten **drei**
`closest()`-Aufrufe auf. Gemessen an euren Fixtures:

| Fixture | Textknoten | Ancestor-Walks |
| ------- | ---------: | -------------: |
| small   |     10.406 |         31.218 |
| medium  |    105.302 |        315.906 |
| large   |    ~1 Mio. |      ~3,2 Mio. |

**Lösung.** Eine einzige rekursive Traversierung, die den Kontext als Parameter
mitführt statt ihn pro Knoten neu zu erlaufen. Die Semantik muss **exakt** erhalten
bleiben:

```ts
const BLOCKS = new Set([
  'P',
  'LI',
  'PRE',
  'TD',
  'TH',
  'H1',
  'H2',
  'H3',
  'H4',
  'H5',
  'H6',
  'SUMMARY',
  'DT',
  'DD',
]);

export function indexText(root: HTMLElement): TextIndex {
  let text = '';
  const parts: TextPart[] = [];
  let lastBlock: Element | null = null;

  const emit = (node: Text, block: Element | null) => {
    if (lastBlock && block !== lastBlock) text += '\n';
    lastBlock = block;
    const start = text.length;
    text += node.data;
    parts.push({ node, start, end: text.length });
  };

  // ignored: innerhalb button/[hidden]/[data-search-ignore]
  // closed:  innerhalb eines details ohne [open]
  // summary: innerhalb eines summary (hebt `closed` auf)
  const visit = (
    el: Element,
    ignored: boolean,
    block: Element | null,
    closed: boolean,
    summary: boolean,
  ) => {
    for (let child = el.firstChild; child; child = child.nextSibling) {
      if (child.nodeType === Node.TEXT_NODE) {
        if (!ignored && (!closed || summary)) emit(child as Text, block);
        continue;
      }
      if (child.nodeType !== Node.ELEMENT_NODE) continue;
      const e = child as Element;
      const tag = e.tagName;
      visit(
        e,
        ignored ||
          tag === 'BUTTON' ||
          e.hasAttribute('hidden') ||
          e.hasAttribute('data-search-ignore'),
        BLOCKS.has(tag) ? e : block,
        closed || (tag === 'DETAILS' && !e.hasAttribute('open')),
        summary || tag === 'SUMMARY',
      );
    }
  };

  visit(root, false, null, false, false);
  return { text, parts };
}
```

**Bekannte Abweichung, bewusst in Kauf genommen.** Das bisherige
`parent.closest(...)` läuft über `root` hinaus bis zum Dokument. Die neue Fassung
betrachtet nur den Teilbaum ab `root`. Da `root` immer eine `.markdown-section`
innerhalb `<article class="markdown">` ist und dort keiner der geprüften Selektoren
vorkommt, ändert das kein Ergebnis. Diese Annahme ist im Code zu kommentieren.

**Fallstricke.**

- Rekursionstiefe entspricht der DOM-Tiefe. `benchmarks/generated/deep-list.md`
  ist der Belastungsfall; prüfen, dass kein Stapelüberlauf auftritt. Falls doch,
  auf einen expliziten Stapel umstellen — nicht auf `TreeWalker` zurückfallen,
  der liefert keine Austrittsereignisse.
- `lastBlock` ist bewusst über die gesamte Traversierung zustandsbehaftet;
  Reihenfolge der Emissionen muss Dokumentreihenfolge bleiben.

**Abnahme.** Neuer Vergleichstest in `tests/preferences-search.test.ts`: alte und
neue Implementierung liefern für `tests/fixtures/hostile.md`,
`tests/fixtures/reader.md` und ein neues Fixture mit
`<details>`/`<summary>`/`<button>`/`[hidden]`/`[data-search-ignore]` identische
`text`-Strings und identische `parts`-Grenzen. Die alte Implementierung dafür
temporär als `indexTextLegacy` im Test mitführen und nach grünem Lauf entfernen.

---

### P1.3 — Suchtreffer als Offsets statt lebender `Range`

**Problem.** `findRanges` (`src/features/search/index.ts:39`) erzeugt dokumentweit
lebende `Range`-Objekte; `ranges.current` und `sectionRanges` halten sie. WebKit
muss **jede** lebende Range bei **jeder** DOM-Mutation nachführen. Während `fill()`
wird pro Abschnitt mutiert — der Aufwand wächst überproportional. Dazu kommt der
Speicher.

**Lösung.** Treffer als schlichte Datenobjekte halten, `Range` nur für sichtbare
und aktive Treffer materialisieren.

```ts
export interface Match {
  startNode: Text;
  startOffset: number;
  endNode: Text;
  endOffset: number;
}

export function findMatches(index: TextIndex, query: string): Match[];

export function toRange(match: Match): Range {
  const range = document.createRange();
  range.setStart(match.startNode, match.startOffset);
  range.setEnd(match.endNode, match.endOffset);
  return range;
}
```

`findRanges` bleibt als dünner Adapter (`findMatches(...).map(toRange)`) für
bestehende Tests erhalten.

**Anzupassende Stellen in `DocumentViewport.tsx`:**

- `ranges` und `sectionRanges` halten `Match` statt `Range`.
- `paintVisible()` (`:635`) materialisiert Ranges nur für die Abschnitte in
  `visibleSections` plus den aktiven Treffer und verwirft sie beim nächsten Aufruf.
- `drawFallback()` (`:650`) materialisiert nur den aktiven Treffer.
- `:711` `reveal(range.startContainer.parentElement)` →
  `reveal(match.startNode.parentElement)`.
- `:713-716` und `:750-753` (Sprung zum Treffer) materialisieren den einen Treffer.

**Fallstricke.**

- `Match` hält direkte Referenzen auf `Text`-Knoten. Werden Abschnitte entfernt
  (`retireContent`), müssen die zugehörigen Einträge in `sectionRanges` verworfen
  werden — das geschieht bereits über `sectionRanges.current.clear()` in `mount()`.
- Syntax-Highlighting ersetzt Textknoten. Der bestehende Schutz über
  `textVersion` und `indexes.delete(shell)` bleibt zwingend erforderlich.

**Abnahme.** Suchnavigation bis zum letzten Treffer, Trefferzählung und
Fallback-Overlay unverändert (`tests/ui/reader.spec.ts`). `hashline.search` bei
1 MiB nicht schlechter als 64,5 ms Median. Speicher während aktiver Suche auf der
1-MiB-Fixture messbar niedriger.

---

### P1.4 — Baumtraversierungen zusammenfassen

**Problem.** Pro Abschnitt laufen nach DOMPurifys eigenem Walk **sieben** weitere
vollständige Traversierungen:

| Ort                        | Selektor                                                        |
| -------------------------- | --------------------------------------------------------------- |
| `policy.ts:144`            | `[id], [class], input, [width], [height], [colspan], [rowspan]` |
| `policy.ts:178`            | `a`                                                             |
| `policy.ts:190`            | `img`                                                           |
| `DocumentViewport.tsx:217` | `img[data-remote-source]`                                       |
| `DocumentViewport.tsx:230` | `table`                                                         |
| `DocumentViewport.tsx:239` | `pre`                                                           |
| `DocumentViewport.tsx:252` | `img` (für `dataset.hasImages`)                                 |

**Lösung.** `sanitizeFragment` führt **einen** Walk aus und liefert die
gesammelten Knoten mit zurück:

```ts
export interface SanitizedFragment {
  fragment: DocumentFragment;
  blockedImages: number;
  remoteImages: number; // ersetzt querySelectorAll('img[data-remote-source]')
  hasImages: boolean; // ersetzt querySelector('img')
  tables: HTMLTableElement[];
  pres: HTMLPreElement[];
}
```

Umsetzung mit einem `TreeWalker` über `NodeFilter.SHOW_ELEMENT` und einem
`switch (element.tagName)`; die Attributprüfungen aus `policy.ts:147-176` bleiben
wortgleich, werden aber nur noch für die tatsächlich betroffenen Elemente
ausgeführt. `tables` und `pres` werden **gesammelt und erst nach dem Walk mutiert**
(`replaceWith` während der Traversierung ist unzulässig).

Der Viewport verwendet danach die zurückgegebenen Listen statt eigener Abfragen.

**Fallstricke.**

- `sanitizeContent` (`policy.ts:213`) ist der String-Adapter für die
  Vertragstests und muss erhalten bleiben.
- `remaining`/Heading-ID-Prüfung (`policy.ts:143-155`) hat Sicherheitscharakter:
  nur parsergenerierte Überschriften-IDs überleben. Diese Logik unverändert
  übernehmen, nicht vereinfachen.
- DOMPurify bleibt in diesem Paket unverändert aktiv. Es wird erst in P2.2 vom
  heißen Pfad genommen.

**Abnahme.** `hashline.sanitize` und `hashline.insert` sinken messbar.
`tests/sections.test.ts` mit allen 652 CommonMark-Vergleichen des bereinigten
Abschnittspfads grün.

---

### P1.5 — `capture()` vom Scroll-Frame entkoppeln

**Problem.** `DocumentViewport.tsx:287-325` läuft in **jedem** Scroll-rAF
(`:346-358`) und erzwingt dabei Layout: Binärsuche mit `getBoundingClientRect()`
über die Shells (`:296`), danach `querySelectorAll` der Überschriften plus ein
Rect pro Überschrift (`:304-309`). Auf `content-visibility: auto`-Abschnitten
erzwingt jeder Rect-Zugriff das übersprungene Layout. Das ist die wahrscheinlichste
Ursache für 93,1 % statt geforderter 99 % Frames innerhalb 16,7 ms.

**Lösung.** Zwei getrennte Pfade:

1. **Pro Frame (billig).** Aktive Überschrift über einen `IntersectionObserver`
   auf den Überschriften bereits gerenderter Abschnitte führen. Der Observer
   pflegt eine nach Dokumentreihenfolge sortierte Menge sichtbarer IDs; aktiv ist
   die letzte oberhalb der 96-px-Schwelle. `progress` aus `scrollTop` und einem
   bei `ResizeObserver` aktualisierten, **zwischengespeicherten** `scrollHeight`.
   Kein `getBoundingClientRect()` im Scroll-Frame.
2. **Selten (exakt).** `capture()` in seiner heutigen Form **unverändert**
   beibehalten für `persist()` (`:359`, `pagehide`), für den Cleanup-Pfad und für
   `restore()`. Diese Aufrufe sind selten; ihre Genauigkeit ist wichtiger als ihre
   Kosten.

`onScroll` ruft danach nur noch den billigen Pfad, `paintVisible()` und
`drawFallback()`.

**Fallstricke.**

- Dies ist das riskanteste Paket der Phase 1: Leseanker, Sprungziele und die
  Outline-Hervorhebung hängen an `capture()`. Die exakte Fassung darf nicht
  gelöscht werden.
- `onHeading` muss weiterhin bei jeder Änderung der aktiven Überschrift feuern,
  damit die Outline mitläuft — aber nicht mehr in jedem Frame.
- Der `IntersectionObserver` darf nur Überschriften **gerenderter** Abschnitte
  beobachten; Beobachtung in übersprungenen Teilbäumen erzwingt Layout (dieselbe
  Falle, die für `highlightObserver` in `:397-420` bereits dokumentiert ist).

**Abnahme.** `benchmarks/scroll-profile.py` auf kleiner und mittlerer Fixture:
Anteil der rAF-Intervalle innerhalb 16,7 ms steigt von 93,1/93,9 % messbar an;
größte Lücke sinkt. `tests/ui/reader.spec.ts`, `tests/desktop/sections.py` und der
Leseanker-Smoke-Schritt unverändert grün.

---

### P1.6 — Speicherdiagnose (Voraussetzung für Phase 2)

**Ziel.** Die +70 bis +100 MiB über dem Plattformboden erklären. Ohne diese Zahl
ist jede weitere Speicherarbeit Raten.

**Aufgaben.**

1. `benchmarks/process-sample.py` um eine **Aufschlüsselung pro Prozess**
   erweitern (UI / NetworkProcess / WebProcess / GPUProcess getrennt ausweisen,
   nicht nur die Summe). Die bestehenden Python-Verträge müssen grün bleiben.
2. **Hashline ohne Dokument messen** — Fenster offen, keine Datei geöffnet. Das
   ergibt den Hashline-eigenen Grundaufschlag gegenüber 139,2 MiB.
3. Nacheinander messen: leeres Fenster → 100 KiB → 1 MiB → 10 MiB → zurück zu
   100 KiB. Die Rückkehr zeigt, was nicht freigegeben wird.
4. Heap-Snapshot der 100-KiB-Situation über den WebKit-Inspector
   (`WEBKIT_INSPECTOR_SERVER`), Auswertung nach Konstruktor: JS-Heap gegen
   DOM/Render-Objekte trennen.
5. Ergebnis als Tabelle in `benchmarks/REPORT.md` unter eigener Überschrift, mit
   Verweis auf dieses Dokument.

**Abnahme.** Die Frage „wie viel von +70…100 MiB ist JS-Heap, wie viel DOM/Render,
wie viel wird nach Dokumentwechsel nicht freigegeben?" ist mit Zahlen beantwortet.

---

### P1.7 — Startpfad

**Aufgaben, nach erwarteter Wirkung sortiert.**

1. **Datei in Rust vorladen.** `src-tauri/src/main.rs`: In `setup()` läuft
   `enqueue(...)` bereits vor dem Frontend-Start. Direkt danach einen
   `spawn_blocking`-Task starten, der den ersten anstehenden Pfad liest und das
   fertige Binärpaket (u32-LE-Länge + JSON-Kopf + UTF-8-Quelle, wie in
   `read_document` gebaut) in einem `Mutex<Option<(PathBuf, Vec<u8>)>>` ablegt.
   `read_document` prüft diesen Cache zuerst und entnimmt ihn. Damit überlappt
   das Lesen mit den ~730 ms WebView-Boot.
   - **Wichtig:** Die Autorisierungsprüfung (`state.approved`) und die
     Session-Registrierung dürfen **nicht** übersprungen werden. Der Cache liefert
     nur die Bytes; `read_document` behält seinen vollständigen Prüfpfad.
   - Cache nach erster Entnahme leeren; bei abweichendem Pfad verwerfen.
2. **`src-tauri/Cargo.toml`:** `lto = "fat"` statt `"thin"`, zusätzlich
   `panic = "abort"`. `codegen-units = 1` und `strip = true` bleiben.
   Danach `cargo clippy -D warnings` und die nativen Verträge erneut laufen lassen —
   `panic = "abort"` ist gegen Tauri/tao zu verifizieren, nicht anzunehmen.
3. **Bundle.** Das Inlining des Entry-Chunks in `index.html` ist **nicht**
   ohne Weiteres möglich: `tauri.conf.json` setzt `script-src 'self'`, ein Inline-Script
   bräuchte einen `'sha256-…'`-Eintrag in der CSP, der bei jedem Build neu berechnet
   werden müsste. Dieses Paket deshalb **zurückstellen**; der wesentliche
   Bundle-Gewinn kommt ohnehin aus P2.5 (React-Entfernung, ~190 KB von 276 KB).

**Abnahme.** `hashline.native-main-to-frontend` und
`hashline.native-main-to-first-frame` über `benchmarks/desktop.py --mode startup`,
n=30, gegen 731,6 / 1.219,1 ms Median.

---

## 4. Phase 2 — Rust-Kern, Renderer austauschbar

Voraussetzung: Phase 1 abgeschlossen und vermessen. Ziel ist nicht nur
Geschwindigkeit, sondern die Umkehrbarkeit der Plattformentscheidung aus
Abschnitt 1.

---

### P2.1 — Op-Buffer-Format festlegen

**Motivation.** Heute ist der HTML-String ein reiner Zwischenschritt, der dreimal
bezahlt wird: der Worker serialisiert einen Baum zu Text, DOMPurify parst den Text
zurück zu einem Baum in einem Fremddokument, und die Knoten werden anschließend
adoptiert. Gemessen in V8/jsdom (Verhältnis übertragbar, Absolutwerte nicht;
deckt sich mit BASELINE 268 ms Parse gegen 378 ms Bereinigung):

```
medium (995 KiB Quelle → 1.535 KiB HTML)
  marked ..................  76 ms
  DOMPurify ............... 979 ms
  davon reiner HTML-Reparse  534 ms
```

**Format.** Drei übertragbare Puffer plus ein Kopf. Alle Offsets in Byte,
alle Längen in Byte, `strings` ist ein einziger UTF-8-Blob.

```
ops:    Uint32Array, 4 Wörter je Operation
        [kind, a, b, c]

        kind 0  OPEN     a = tagId        b = attrStart   c = attrCount
        kind 1  CLOSE    a,b,c = 0
        kind 2  TEXT     a = strOffset    b = strLen      c = 0

attrs:  Uint32Array, 3 Wörter je Attribut
        [nameId, strOffset, strLen]

strings: Uint8Array, UTF-8

sections: Uint32Array, 2 Wörter je Abschnitt
        [opStart, opCount]

headings: Uint32Array, 4 Wörter je Überschrift
        [level, idOffset, idLen, sectionIndex]
```

`tagId` indiziert eine **fest einkompilierte** Tag-Tabelle, die exakt der heutigen
`ALLOWED_TAGS`-Liste aus `policy.ts:66-106` entspricht. `nameId` indiziert die
Tabelle aus `ALLOWED_ATTR` (`policy.ts:107-124`).

**Sicherheitsargument.** Nur Tags aus der Tabelle sind überhaupt darstellbar, nur
Attributnamen aus der Tabelle sind kodierbar. Unerlaubtes Markup ist nicht
_ausdrückbar_, statt nachträglich entfernt zu werden — das ist strenger als heute.
**Attributwerte bleiben prüfpflichtig:** `classifyUrl` für `href`/`src`, die
Zahlenprüfung für `width`/`height`/`colspan`/`rowspan` und die
Heading-ID-Regel aus `policy.ts:143-155` wandern in den Replay, arbeiten dort aber
auf einer kleinen bekannten Menge statt auf einem DOM-Walk.

**Rohes HTML im Markdown.** Für `html`-Tokens gibt es keinen sicheren Opcode.
Diese behalten den heutigen Pfad: als Text-Blob transportiert, im Replay durch
DOMPurify geschickt und als Fragment eingehängt. Das ist der seltene Fall und
darf nicht optimiert werden.

**Abnahme.** Format in `src/core/markdown/opbuffer.ts` als Typen plus Enkoder/
Dekoder mit Rundlauftests. Noch keine Verhaltensänderung im Produkt.

---

### P2.2 — Renderer: Op-Buffer statt HTML-String

**Aufgaben.**

1. `parseMarkdown` (`src/core/markdown/parser.ts`) erzeugt zusätzlich zum
   heutigen `sections[].html` einen Op-Buffer. Beide Wege bestehen parallel,
   damit die CommonMark-Verträge weiter vergleichen können.
2. Der Worker (`src/workers/markdown.worker.ts`) überträgt die Puffer per
   `postMessage(response, [ops.buffer, attrs.buffer, strings.buffer, ...])` —
   **zero-copy**, statt heute eine vollständige String-Kopie.
3. Neuer Replay im Viewport ersetzt `sanitizeFragment` auf dem heißen Pfad:

```ts
const text = new TextDecoder().decode(strings); // einmal pro Dokument
const stack: (Element | DocumentFragment)[] = [fragment];
for (let i = opStart; i < opStart + opCount; i++) {
  const o = i * 4;
  switch (ops[o]) {
    case OPEN: {
      const el = document.createElement(TAGS[ops[o + 1]]);
      applyAttributes(el, ops[o + 2], ops[o + 3]); // inkl. classifyUrl
      stack[stack.length - 1].append(el);
      stack.push(el);
      break;
    }
    case CLOSE:
      stack.pop();
      break;
    case TEXT:
      stack[stack.length - 1].append(
        document.createTextNode(
          text.substring(ops[o + 1], ops[o + 1] + ops[o + 2]),
        ),
      );
      break;
  }
}
```

Der `strings`-Blob wird **einmal pro Dokument** dekodiert; die Textknoten
entstehen per `substring`. Kein HTML-Parse, kein Fremddokument, keine Adoption,
keine der sieben Traversierungen aus P1.4.

**Fallstricke.**

- `text.substring` mit Byte-Offsets funktioniert nur, wenn der Enkoder
  UTF-16-Code-Unit-Offsets speichert, nicht Byte-Offsets. **Entweder** im Format
  auf UTF-16-Offsets festlegen (einfacher für JS, unbequem für Rust) **oder** im
  Dekoder eine Byte→UTF-16-Offsettabelle aufbauen. Diese Entscheidung ist in P2.1
  zu treffen und zu dokumentieren; sie ist die häufigste Fehlerquelle des Ansatzes.
- Der String-Adapter `sanitizeContent` und der HTML-Pfad bleiben für die
  Vertragstests erhalten.

**Abnahme.** `tests/sections.test.ts` vergleicht zusätzlich den **Op-Buffer-Pfad**
gegen den bereinigten HTML-Pfad für alle 652 CommonMark-Beispiele: identische
DOM-Struktur (`isEqualNode` auf dem Ergebnisfragment). `hashline.sanitize` und
`hashline.insert` sinken deutlich.

---

### P2.3 — Parser nach Rust

**Erst beginnen, wenn P2.2 grün ist.** Dann ist der Austausch der Quelle des
Op-Buffers eine lokale Änderung.

**Aufgaben.**

1. `pulldown-cmark` in `src-tauri`, Optionen passend zu GFM: Tabellen,
   Strikethrough, Tasklists, Footnotes.
2. Emission desselben Op-Buffers wie P2.1, ausgeliefert über den bestehenden
   binären IPC-Weg (`read_document` liefert bereits ein Binärpaket; das Format
   wird um die Puffer erweitert, `src/platform/document-packet.ts` entsprechend).
3. Überschriften-Slugs müssen **bitgenau** der heutigen Regel entsprechen:
   `slugBase` in `parser.ts:9-19` (NFKC, Kleinschreibung, `\p{L}\p{N}\s_-`,
   Leerzeichen/Unterstriche zu `-`, Fallback `section`), Präfix `doc-`,
   Deduplizierung mit `-1`, `-2` …
4. Danach entfallen: `marked` als Abhängigkeit, `src/workers/markdown.worker.ts`,
   `src/core/markdown/service.ts`, `tokenizer.ts`, `html-boundary.ts`.

**Die eigentliche Entscheidung dieses Pakets.** Die 652 CommonMark-Tests
vergleichen heute gegen **marked**. `pulldown-cmark` ist CommonMark-konform,
erzeugt aber nicht zeichengleiches HTML. Die Referenz muss deshalb von „marked"
auf „CommonMark-Spezifikation" umgestellt werden — mit DOM-Vergleich
(`isEqualNode`) statt Stringvergleich. Das ist eine bewusste Änderung der
Testsemantik und gehört in eine eigene Entscheidung (008), bevor Code entsteht.

**Nicht messen ohne Basis.** Auf dieser Maschine ist kein `cargo` installiert;
ich konnte `pulldown-cmark` **nicht** selbst vermessen. Der erwartete Gewinn
(1 MiB im einstelligen Millisekundenbereich gegenüber heute 163 ms) beruht auf
veröffentlichten Durchsatzwerten. **Erster Schritt dieses Pakets ist ein
20-Zeilen-Benchmark gegen `benchmarks/generated/*.md`**, bevor irgendetwas
umgebaut wird. Fällt er schlechter aus als erwartet, entfällt P2.3 und P2.2
bleibt der Endstand.

---

### P2.4 — Suchindex ohne DOM

**Mit P2.3 wird der Suchindex kostenlos.** Der `strings`-Blob _ist_ der
Dokumenttext in Dokumentreihenfolge. Suche läuft direkt darauf; das Mapping
Treffer → DOM-Knoten geht über Textoffset → Op-Index → Knoten (der Replay führt
eine Zuordnung Op-Index → erzeugter Knoten für die TEXT-Operationen des jeweils
gerenderten Abschnitts).

Damit entfallen `indexText`, die `indexes`-WeakMap und die Kopplung der Suche an
`renderState === 'complete'` (`DocumentViewport.tsx:679`) — **vollständige Suche
ist ab dem ersten Frame verfügbar**, nicht erst nach dem Aufbau. Das ist der
Punkt, an dem die in `REPORT.md` als offen geführte Einschränkung „Vollständige
Suche wartet auf vollständige DOM-Einfügung" verschwindet.

**Abnahme.** Volltextsuche auf 10 MiB unter 150 ms ab letzter Eingabe, gemessen
mit `benchmarks/desktop.py --mode interaction --fixtures large --repetitions 30`.

---

### P2.5 — React entfernen

**Voraussetzung:** P2.2, weil der Viewport dann vollständig imperativ ist.

`DocumentViewport.tsx` ist bereits ein einziger `useLayoutEffect` mit direkter
DOM-Manipulation; React trägt dort nichts bei. Verbleibende React-Nutzung:
`App.tsx`, `Titlebar.tsx`, `Outline.tsx`, `SearchBar.tsx`, `Icon.tsx` — eine
Titelleiste, eine Liste, ein Eingabefeld und ein Menü. Der Zustand liegt bereits
hinter `DocumentController` mit `subscribe`/`getSnapshot` und ist damit
React-unabhängig.

Erwartung: ~190 KB von 276 KB Bundle entfallen, entsprechend weniger Parse-,
Compile- und Initialisierungszeit im Start sowie weniger JS-Heap.

**Abnahme.** Bundlegröße, `hashline.native-main-to-frontend`, PSS gegen die
Werte aus P1.6. Alle Playwright-Tests unverändert grün.

---

## 5. Messprotokoll für jede Abnahme

1. Build einfrieren: Binary unter neuem Namen in `benchmarks/generated/` ablegen,
   SHA-256 und Quellmanifest festhalten. Bestehende Builds nicht überschreiben.
2. Native Abschnittsverträge und Desktop-Smoke-Test mit **diesem** Binary.
3. Je 30 Öffnungen klein/mittel/groß mit `--distinct-content`. Währenddessen
   keine weiteren Builds, UI-Tests oder Desktoptests starten — die in `REPORT.md`
   dokumentierten Störbedingungen (fremdes Fenster überdeckte das Benchmarkfenster)
   dürfen sich nicht wiederholen.
4. Identische Inhalte unter wechselnden Pfaden **getrennt** messen und berichten,
   um den Anteil der DOM-Wiederverwendung auszuweisen.
5. Ergebnisse in `benchmarks/REPORT.md` gegen die unveränderten SPEC-Budgets
   auswerten. Lokale Doppel-rAF-Hilfsframes bleiben ausdrücklich **kein** Nachweis
   präsentierter Pixel und keine Referenzabnahme.

## 6. Was ausdrücklich nicht zu tun ist

- **Keine weitere Parseroptimierung.** Parsing ist rund 6 % der 10-MiB-Kosten.
  Der Grenznutzen ist ausgeschöpft; P2.3 ersetzt den Parser ohnehin.
- **Kein Toolkit-Rewrite** vor Abschluss von P1.6. Die Messung aus Abschnitt 1
  sagt, dass er die erhoffte Ersparnis nicht bringt.
- **Kein Electron/CEF.**
- **Keine Anhebung der Budgets** aus `SPEC.md`, auch nicht stillschweigend.
- **Kein Zusammenlegen von Arbeitspaketen** in einen Commit. Jedes Paket hat ein
  eigenes Abnahmekriterium; vermischte Änderungen machen die Messreihen wertlos.

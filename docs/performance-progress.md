# Performanceoptimierung – Übergabestand

Stand: 8. September 2026, nach dem Release-Build und der Messreihe zu **P2.2**.
Auftragsgrundlage: [007 – Performancepfad](decisions/007-performance-path.md).
Der frühere Übergabestand (Parser-/Abschnittsarbeit vor Phase 1) liegt im Commit
`aec9605`; er wird hier nicht wiederholt.

## Was umgesetzt ist

| Paket | Inhalt                                              | Commit    | Status                    |
| ----- | --------------------------------------------------- | --------- | ------------------------- |
| P1.1  | `indexText` aus dem Renderpfad entfernt             | `8055e91` | erledigt, vermessen       |
| P1.2  | `indexText` einpassig ohne `closest()`              | `e1cceb9` | erledigt, vermessen       |
| P1.3  | Suchtreffer als Offsets statt lebender `Range`      | `9232ce9` | erledigt, vermessen       |
| P1.4  | sieben Baumtraversierungen zu einer zusammengefasst | `acfdb6a` | erledigt, vermessen       |
| P1.5  | `capture()` vom Scroll-Frame entkoppeln             | –         | **offen, nicht begonnen** |
| P1.6  | Speicherdiagnose                                    | –         | **offen, nicht begonnen** |
| P1.7  | Startpfad                                           | –         | **offen, nicht begonnen** |
| P2.1  | Op-Buffer-Format, Enkoder, Dekoder                  | `7641453` | erledigt                  |
| P2.2  | Replay statt DOMPurify auf dem heißen Pfad          | `7171211` | erledigt, vermessen       |
| P2.3  | Parser nach Rust                                    | –         | **offen, nicht begonnen** |
| P2.4  | Suchindex ohne DOM                                  | –         | **offen, nicht begonnen** |
| P2.5  | React entfernen                                     | –         | **offen, nicht begonnen** |

Phase 1 wurde auf ausdrückliche Weisung bei P1.4 angehalten. P1.5–P1.7 sind
**nicht** umgesetzt; 007 führt P1.6 als Voraussetzung für Phase 2 auf, diese
Reihenfolge wurde auf Weisung übersprungen. Die Speicherfrage aus 007 Abschnitt 1
ist damit weiterhin unbeantwortet — siehe „Offene Punkte“.

## P1.4 als Befund, nicht als Gewinn

P1.4 legte die sieben Traversierungen pro Abschnitt zu einer zusammen. Wirkung
auf `hashline.sanitize` bei 10 MiB: 3.292 → 3.224 ms, also im Rauschen. Damit war
belegt, dass nicht die eigenen Durchläufe teuer waren, sondern DOMPurify selbst:
ein HTML-Reparse je Abschnitt in ein Fremddokument mit anschließender Adoption.
Genau das entfernt P2.2.

## Aktueller Implementierungsstand von Phase 2

- `src/core/markdown/opbuffer.ts` — Format aus 007 Abschnitt 4: `ops`, `attrs`,
  `strings`, `sections`, `headings` als übertragbare typisierte Arrays.
  **Offsetentscheidung:** `strings` bleibt ein UTF-8-Blob, aber alle Offsets und
  Längen in TEXT-Ops, Attributen und Überschriften sind **UTF-16-Code-Unit-Offsets**
  in den einmal pro Dokument dekodierten JS-String. Der Enkoder führt die
  UTF-16-Länge inkrementell mit; der Replay nutzt `substring` ohne
  Umrechnungstabelle, und ein Rust-Enkoder (P2.3) kann die Einheiten im ohnehin
  nötigen Durchlauf mitzählen. Die Begründung steht im Modulkopf.
- **Abweichung vom Format in 007:** `sections` trägt ein drittes Wort `flags`.
  Ein nicht kodierbarer Abschnitt muss von einem Abschnitt ohne Operationen
  unterscheidbar sein; `[opStart, opCount]` kann das nicht ausdrücken.
- Die Tag-Tabelle ist exakt `ALLOWED_TAGS`, die Attributnamentabelle exakt
  `ALLOWED_ATTR`; `policy.ts` importiert beide aus dem Formatmodul, damit sie
  nicht auseinanderlaufen können.
- Textläufe werden **im Enkoder** verschmolzen, wie der HTML-Parser es tut. Das
  macht `isEqualNode` als Vertrag benutzbar und spart Knoten zur Laufzeit.
- `parser.ts` erzeugt den Puffer zusätzlich zu `sections[].html`. Ein Abschnitt
  gilt als **roh**, sobald einer seiner Tokens ein `html`-Token enthält; solche
  Abschnitte behalten den DOMPurify-Pfad. Ohne diese Regel wichen fünf
  CommonMark-Beispiele ab — rohe HTML-Tabellen (implizites `<tbody>`) und ein
  rohes `<pre>` (verschlucktes erstes `\n`).
- `markdown.worker.ts` übergibt die fünf Puffer in der Transfer-Liste statt sie
  zu kopieren.
- `policy.ts` enthält `replayFragment`. Die Attributpolitik ist inhaltlich
  unverändert: `classifyUrl` für `href`/`src`, Zahlenprüfung für
  `width`/`height`/`colspan`/`rowspan`, Sprachklassenregel für `code`,
  Checkbox-Regel für `input`, und nur parsergenerierte Überschriften-IDs
  überleben. **Zwei DOMPurify-Verhalten mussten mitwandern**, sonst wäre der
  Op-Pfad laxer als der HTML-Pfad: die Attributwert-URI-Prüfung
  (`allowedAttributeValue`, zeichengleich aus DOMPurify übernommen) und die
  Tatsache, dass DOMPurify jeden behaltenen Attributwert trimmt.
- `sanitizeFragment` und `sanitizeContent` bleiben unverändert bestehen. Sie sind
  der Vergleichsmaßstab der Verträge, kein Altpfad.

## Messreihe P2.2

Build eingefroren unter `benchmarks/generated/phase2-p22/hashline`,
SHA-256 `7b17a4550a28584336e6e79ddb7e3a2c018b11bc77e37e183b972b11cee292c0`,
[Quellmanifest](../benchmarks/results/phase2-p22-source.json) über 152 Quelldateien.
Rohdaten: [phase2-p22-open.json](../benchmarks/results/phase2-p22-open.json),
Vergleich gegen [phase1-p14-open.json](../benchmarks/results/phase1-p14-open.json).
Beides `--mode open --repetitions 30 --distinct-content`, dieselbe Maschine
(Ubuntu 26.04, Ryzen 9 9900X, RTX 3060, Wayland, WebKitGTK 2.52.6). Während der
Reihe liefen keine Builds, UI- oder Desktoptests.

Mediane in Millisekunden, in Klammern p95:

| Fixture | Metrik                      |            P1.4 |            P2.2 |  Änderung |
| ------- | --------------------------- | --------------: | --------------: | --------: |
| 10 MiB  | `hashline.sanitize`         | 3.224,5 (3.292) |   559,0 (621,0) | **−83 %** |
| 10 MiB  | `hashline.insert`           |   240,5 (263,0) |   139,0 (157,0) |     −42 % |
| 10 MiB  | `hashline.parse`            |   878,0 (907,0) | 1.061,5 (1.162) |     +21 % |
| 10 MiB  | `hashline.open-to-complete` | 11.350 (12.144) | 4.920,5 (5.511) | **−57 %** |
| 10 MiB  | `hashline.max-render-task`  |     67,0 (87,0) |     19,0 (20,0) |     −72 % |
| 10 MiB  | Öffnung inkl. Treiber       | 1.852,5 (1.907) | 1.906,2 (2.071) |      +3 % |
| 1 MiB   | `hashline.sanitize`         |   308,0 (335,0) |     45,5 (61,0) | **−85 %** |
| 1 MiB   | `hashline.insert`           |     23,0 (35,0) |     14,0 (17,0) |     −39 % |
| 1 MiB   | `hashline.parse`            |     85,0 (88,0) |   107,0 (111,0) |     +26 % |
| 1 MiB   | `hashline.open-to-complete` |   741,0 (785,0) |   432,0 (469,0) | **−42 %** |
| 1 MiB   | Öffnung inkl. Treiber       |   270,0 (296,0) |   314,4 (379,7) |     +16 % |
| 100 KiB | `hashline.sanitize`         |     27,0 (31,0) |       4,5 (6,0) | **−83 %** |
| 100 KiB | `hashline.insert`           |       3,0 (6,0) |       1,5 (2,0) |     −50 % |
| 100 KiB | `hashline.open-to-complete` |     72,5 (78,0) |     46,5 (51,0) |     −36 % |
| 100 KiB | Öffnung inkl. Treiber       |   115,7 (124,2) |     91,5 (96,8) |     −21 % |

### Ehrliche Bewertung

**Das Zielmaß ist erreicht.** `hashline.sanitize` fällt auf allen drei Fixtures
um 83–85 %. Der vollständige Aufbau bei 10 MiB sinkt von 11,35 s auf 4,92 s.
`hashline.max-render-task` fällt bei 10 MiB von 67 auf 19 ms und hält damit
erstmals das 50-ms-Budget aus SPEC Abschnitt 9 für Renderaufgaben.

**Zwei Verschlechterungen gehören dazu und werden nicht kaschiert:**

1. `hashline.parse` steigt um 183 ms (10 MiB) beziehungsweise 22 ms (1 MiB). Der
   Enkoder läuft im Worker innerhalb des Parse-Timers. Die Kosten liegen im
   linearen HTML-Scan, im `slice()` der gewachsenen Wortarrays, im `join()` der
   Stringstücke und in `TextEncoder.encode`. Das ist **keine** Parserregression,
   sondern neue Arbeit an anderer Stelle, und sie ist um Faktor 17 kleiner als
   das, was sie auf dem Hauptthread einspart.
2. Die vom Treiber beobachtete Öffnung wird bei 1 und 10 MiB langsamer:
   Median +44 ms (1 MiB) und +54 ms (10 MiB), p95 +84 beziehungsweise +164 ms.
   Über die fünf Phase-1-Reihen lagen diese Mediane bei 264–274 (1 MiB) und
   1.851–1.883 ms (10 MiB); die Verschiebung liegt außerhalb dieser Streuung und
   ist damit real, nicht Rauschen. Ursache: der Worker antwortet erst, wenn das
   **gesamte** Dokument kodiert ist, während der erste Frame nur den ersten
   Abschnitt braucht.
   **Folge für das Budget:** „Große Datei lesbar innerhalb 2 s“ wurde mit P1.4
   knapp gehalten (p95 1.907 ms) und wird jetzt knapp verfehlt (p95 2.071 ms).
   Bei 100 KiB und 1 MiB bleiben die Budgets (≤ 150 / ≤ 500 ms) eingehalten.

Der Op-Buffer hat also den erwarteten Effekt gehabt und zusätzlich einen
Zielkonflikt sichtbar gemacht: schnellerer Gesamtaufbau gegen etwas späteren
ersten Frame bei großen Dateien. Die Auflösung — Kodierung abschnittsweise
streamen oder in Rust erledigen — gehört nach P2.3 und wurde hier bewusst nicht
vorweggenommen.

**Speicher:** Der Puffer für 10 MiB umfasst rund 46,7 MB (ops 35,2 MB, strings
10,5 MB, attrs) und wird über die Lebensdauer des Dokuments gehalten, zusätzlich
zu den weiterhin gehaltenen `sections[].html`. Das ist ein Zuwachs gegenüber
P1.4 und wurde in dieser Reihe **nicht** gemessen. Die 4-Wort-Ops aus 007 sind
dafür die Hauptursache (CLOSE nutzt eines von vier Wörtern). P1.6 ist damit
dringlicher geworden, nicht weniger dringlich.

## Tests

- `npm run lint`, `npm run typecheck`, `npm run build`: bestanden.
- **2.036 Unit-Tests bestanden** (vorher 1.369). Neu: 11 Rundlauftests des
  Formats inklusive Zeichen jenseits der BMP, leeres Dokument, verschachtelte
  Listen, Tabellen, Codeblöcke; der Replay-Vergleich über alle
  652 CommonMark-Beispiele (`isEqualNode` gegen den bereinigten HTML-Pfad,
  Abschnitt für Abschnitt); eine Schranke für die Fallback-Rate; ein
  Politiktest des Replays parallel zu `tests/policy-fragment.test.ts`.
- **18 Playwright-Tests bestanden**, einschließlich des echten Workers mit
  Transfer-Liste.
- **Sechs native Abschnittsverträge bestanden** mit genau diesem Build:
  [phase2-p22-contracts.json](../benchmarks/results/phase2-p22-contracts.json).
- `npm run check` bricht vor den Tests ab, weil `prettier --check` 22 bereits
  vor Phase 1 unformatierte Dateien meldet (Benchmark-Rohdaten und
  `benchmarks/REPORT.md`). Diese Rohdaten dürfen nicht umgeschrieben werden;
  die vier Einzelschritte wurden deshalb getrennt ausgeführt. `policy.ts` war
  ebenfalls betroffen und ist jetzt formatiert.

### Fallback-Rate des Enkoders

| Korpus                           | Abschnitte | über Op-Buffer | Fallback |
| -------------------------------- | ---------: | -------------: | -------: |
| 652 CommonMark-Beispiele         |        651 |            573 |       78 |
| `benchmarks/generated/small.md`  |         10 |             10 |        0 |
| `benchmarks/generated/medium.md` |        100 |            100 |        0 |
| `benchmarks/generated/large.md`  |        995 |            995 |        0 |
| `tests/fixtures/reader.md`       |          1 |              0 |        1 |

Die 78 Fallbacks sind rohes HTML und Zeichenreferenzen außerhalb der
unterstützten Menge (`&ouml;`, `&MadeUpEntity;`, `&#0;`). Beides ist gewollt:
eine unvollständige Entitätentabelle würde stillschweigend anderen Text
erzeugen als der HTML-Parser. `reader.md` besteht aus einem einzigen Abschnitt
mit `<details>`/`<summary>` und fällt deshalb vollständig zurück.

## Offene Punkte

1. **P1.6 (Speicherdiagnose) ist unbeantwortet** und durch den Op-Buffer
   wichtiger geworden. Die +70 bis +100 MiB über dem Plattformboden aus 007
   Abschnitt 1 sind weiterhin nicht aufgeschlüsselt; die rund 46,7 MB
   Pufferspeicher bei 10 MiB kommen ungemessen hinzu.
2. **Erster Frame bei 10 MiB.** p95 der vom Treiber beobachteten Öffnung liegt
   mit 2.071 ms über dem 2-s-Budget. Vor weiteren Paketen entscheiden, ob das
   akzeptiert wird oder ob die Kodierung abschnittsweise geliefert werden muss.
3. **P2.3 zuerst vermessen, nicht umbauen.** 007 verlangt einen
   20-Zeilen-Benchmark von `pulldown-cmark` gegen `benchmarks/generated/*.md`,
   bevor Code entsteht. Fällt er schlechter aus als erwartet, bleibt P2.2 der
   Endstand. Die Umstellung der Testreferenz von „marked“ auf
   „CommonMark-Spezifikation“ gehört vorher in eine eigene Entscheidung 008.
4. `benchmarks/REPORT.md` und Entscheidung 004 sind **nicht** auf diese Reihe
   fortgeschrieben worden; sie beziehen sich weiterhin auf ältere Builds.
5. Die lokalen Doppel-rAF-Hilfsframes bleiben kein Nachweis präsentierter Pixel
   und keine Referenzabnahme. Die Maschine hat eine diskrete GPU und ist nicht
   die SPEC-Referenz.

## Hinweise zur Arbeitsumgebung

- Builds und native Tests **immer** über `resolveDesktopEnvironment` aus
  `scripts/desktop-env.mjs`; Rust-Toolchain (`/tmp/hashline-cargo`) und
  WebKit-Sysroot (`/tmp/hashline-sysroot`) liegen unter `/tmp` und überleben
  einen Neustart nicht.
- Release-Build dieser Reihe: `npx tauri build --no-bundle` mit dieser Umgebung.
- Native Tests liefen über einen eigenen Session-Bus, eigene XDG-Verzeichnisse
  unter `/tmp/hashline-p22/` und die Ports 4565 (tauri-driver) / 4566
  (`WebKitWebDriver` aus dem Sysroot), mit
  `HASHLINE_WEBDRIVER_URL=http://127.0.0.1:4565`. Treiber und Bus wurden nach
  der Reihe beendet; fremde App- oder Treiberprozesse wurden nicht angefasst.
- Eingefrorene Builds unter `benchmarks/generated/` sind git-ignoriert; ihre
  Identität steht ausschließlich in den `*-source.json`-Manifesten.

Weiterführend: [007 – Performancepfad](decisions/007-performance-path.md),
[Benchmarkbericht](../benchmarks/REPORT.md),
[Rendererentscheidung](decisions/004-rendering-gate.md),
[Architektur](architecture.md).

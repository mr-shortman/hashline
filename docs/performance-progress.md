# Performanceoptimierung – Übergabestand

Stand: 8. September 2026, nach Abschluss von **Phase 2** und der Messreihe
`phase2-final`. Auftragsgrundlage: [007 – Performancepfad](decisions/007-performance-path.md),
[008 – Parserreferenz](decisions/008-parser-reference.md).
Der Übergabestand nach P2.2 liegt im Commit `b016892`; er wird hier nicht wiederholt.

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
| P2.5  | React entfernt                                      | `a212efc` | erledigt                  |
| P2.3  | Parser nach Rust                                    | `38fc52f` | erledigt                  |
| P2.4  | Suche ohne DOM                                      | `2d11fde` | erledigt                  |

Phase 2 ist damit abgeschlossen. P1.5–P1.7 sind weiterhin **nicht** umgesetzt;
007 führt P1.6 als Voraussetzung für Phase 2 auf, diese Reihenfolge wurde auf
Weisung übersprungen. Die Speicherfrage aus 007 Abschnitt 1 ist unbeantwortet —
siehe „Offene Punkte".

## Der Stand der Architektur in einem Absatz

Der Parser ist eine eigenständige Rust-Bibliothek (`src-tauri/markdown`,
`pulldown-cmark`) ohne Tauri- oder Plattformabhängigkeit. Sie liest das Dokument
einmal und erzeugt den Op-Buffer; ein HTML-String entsteht nur noch für
Abschnitte mit rohem HTML. Der Desktop bekommt den Puffer über `read_document`,
der Markdowntext überquert die IPC-Grenze nicht mehr. Dieselbe Bibliothek wird
nach WebAssembly übersetzt und bedient Browser-Vorschau und Testlauf, damit es
**eine** Parserimplementierung gibt. Der Renderer erzeugt Knoten direkt aus den
Operationen. Die Suche liest den Textblob des Puffers. Die Oberfläche ist
imperativ und ohne Framework. Damit ist der Renderer das austauschbare Modul,
das 007 Abschnitt 1 als Ziel der Phase nennt.

## Messreihe `phase2-final`

Build eingefroren unter `benchmarks/generated/phase2-final/hashline`,
SHA-256 `57b085e237da869e5e561c94519e5872b8f8ca628abe16ebdb656d6b2e23f1ee`,
[Quellmanifest](../benchmarks/results/phase2-final-source.json) über 251
versionierte Dateien. Dieselbe Maschine wie alle Vorreihen (Ubuntu 26.04,
Ryzen 9 9900X, RTX 3060, Wayland, WebKitGTK 2.52.6). Während der Reihe liefen
keine Builds, UI- oder Desktoptests.

Rohdaten: [open](../benchmarks/results/phase2-final-open.json),
[startup](../benchmarks/results/phase2-final-startup.json),
[interaction 10 MiB](../benchmarks/results/phase2-final-interaction.json),
[interaction 1 MiB](../benchmarks/results/phase2-final-interaction-medium.json),
[Abschnittsverträge](../benchmarks/results/phase2-final-contracts.json).

### Öffnen, je 30 Wiederholungen mit `--distinct-content`

Mediane in Millisekunden, in Klammern p95. „P1.4" ist der Stand vor Phase 2,
„P2.2" der Stand nach dem Op-Buffer-Replay.

| Fixture | Metrik                      |            P1.4 |            P2.2 |    phase2-final | zu P1.4   |
| ------- | --------------------------- | --------------: | --------------: | --------------: | --------- |
| 10 MiB  | `hashline.parse`            |     878,0 (907) | 1.061,5 (1.162) |     132,9 (145) | **−85 %** |
| 10 MiB  | `hashline.sanitize`         | 3.224,5 (3.292) |     559,0 (621) |     416,0 (434) | **−87 %** |
| 10 MiB  | `hashline.insert`           |     240,5 (263) |     139,0 (157) |     137,0 (155) | −43 %     |
| 10 MiB  | `hashline.open-to-complete` | 11.350 (12.144) | 4.920,5 (5.511) | 3.495,5 (3.756) | **−69 %** |
| 10 MiB  | `hashline.max-render-task`  |       67,0 (87) |       19,0 (20) |       19,0 (20) | −72 %     |
| 10 MiB  | Öffnung inkl. Treiber       | 1.852,5 (1.907) | 1.906,2 (2.071) |   897,7 (1.038) | **−52 %** |
| 1 MiB   | `hashline.parse`            |       85,0 (88) |     107,0 (111) |       12,0 (14) | **−86 %** |
| 1 MiB   | `hashline.sanitize`         |     308,0 (335) |       45,5 (61) |       34,0 (44) | **−89 %** |
| 1 MiB   | `hashline.insert`           |       23,0 (35) |       14,0 (17) |       11,0 (15) | −52 %     |
| 1 MiB   | `hashline.open-to-complete` |     741,0 (785) |     432,0 (469) |     303,5 (324) | **−59 %** |
| 1 MiB   | Öffnung inkl. Treiber       |     270,0 (296) |     314,4 (380) |     214,5 (239) | −21 %     |
| 100 KiB | `hashline.parse`            |        9,0 (12) |       10,0 (12) |       1,1 (1,2) | −88 %     |
| 100 KiB | `hashline.sanitize`         |       27,0 (31) |         4,5 (6) |         4,0 (5) | −85 %     |
| 100 KiB | `hashline.open-to-complete` |       72,5 (78) |       46,5 (51) |       34,0 (46) | −53 %     |
| 100 KiB | Öffnung inkl. Treiber       |     115,7 (124) |       91,5 (97) |       81,6 (92) | −29 %     |

**Der Zielkonflikt aus P2.2 ist aufgelöst.** Dort stieg die vom Treiber
beobachtete Öffnung bei 10 MiB auf p95 2.071 ms und verfehlte damit das
2-s-Budget, weil der Worker erst antwortete, wenn das gesamte Dokument kodiert
war. Der native Parser liefert den Puffer in 133 ms; der erste Textframe liegt
jetzt bei 728 ms (Median) gegenüber 1.677,5 ms nach P2.2. Alle drei Budgets aus
SPEC Abschnitt 9 sind eingehalten: p95 92 ms (≤ 150), 239 ms (≤ 500), 1.038 ms
(≤ 2.000).

### Start, `--mode startup`, n=30

| Metrik                                | Basis (`final-startup`) |    phase2-final |
| ------------------------------------- | ----------------------: | --------------: |
| `hashline.native-main-to-frontend`    |      731,6 ms (p95 776) |   712,6 (742,6) |
| `hashline.native-main-to-first-frame` |  1.219,1 ms (p95 1.270) | 1.145,7 (1.171) |
| Treiber bis beobachteter erster Frame |  1.256,1 ms (p95 1.309) | 1.181,9 (1.206) |

**Ehrlich bewertet: hier ist fast nichts passiert.** Der Wegfall von React —
191 KB weniger Bundle, ein Chunk weniger — bringt am Start **19 ms**, also
2,6 %. Der erste Textframe sinkt um 73 ms (6 %), und der größere Teil davon
stammt aus dem schnelleren Parser, nicht aus dem Bundle. Die 731 ms bis zum
bereiten Frontend sind damit weiterhin fast vollständig WebView-Boot und nicht
Anwendungscode. Die Erwartung aus 007 P2.5 („entsprechend weniger Parse-,
Compile- und Initialisierungszeit im Start") hat sich **nicht** bestätigt; das
ist ein Befund, kein Fehlschlag der Umsetzung, und macht P1.7 (Startpfad) zum
nächsten sinnvollen Paket.

### Interaktion, n=30

| Fixture | Metrik                  |             P1.3 | phase2-final |
| ------- | ----------------------- | ---------------: | -----------: |
| 1 MiB   | `hashline.search`       | 32,0 ms (p95 33) |  32,0 (32,0) |
| 1 MiB   | Ergebnis bis Hilfsframe | 54,5 ms (p95 70) |    48,0 (60) |
| 10 MiB  | `hashline.search`       |   keine Vorreihe |    48,0 (51) |
| 10 MiB  | Ergebnis bis Hilfsframe |   keine Vorreihe |    78,0 (84) |

Die Abnahme aus 007 P2.4 — „Volltextsuche auf 10 MiB unter 150 ms ab letzter
Eingabe" — ist mit 48 ms Median und 51 ms p95 erfüllt. In beiden Werten stecken
30 ms Eingabeentprellung; die eigentliche Arbeit sind rund 2 ms bei 1 MiB und
rund 18 ms bei 10 MiB.

**Die eigentliche Änderung steht nicht in dieser Tabelle.** Bei 1 MiB ist die
Suchzeit identisch geblieben. Was sich geändert hat, ist der Zeitpunkt: die
Suche wartete bisher auf `renderState === 'complete'`, also bei 10 MiB auf
11,35 s (P1.4) beziehungsweise 4,92 s (P2.2). Jetzt ist das vollständige
Ergebnis verfügbar, während der Aufbau noch läuft. Der siebte native
Abschnittsvertrag prüft genau das und schlägt fehl, wenn die Kopplung
zurückkehrt.

### Bundle

| Stand                | Eintrittschunk | Worker-Chunk |
| -------------------- | -------------: | -----------: |
| vor P2.5             |      279.392 B |     51.406 B |
| nach P2.5            |       88.080 B |     51.406 B |
| nach P2.3            |       88.780 B |            – |
| nach P2.4 (Endstand) |   **90.602 B** |            – |

Dazu `hashline-markdown.wasm` mit 450.450 B. Diese Datei wird **nur** von der
Browser-Vorschau über einen dynamischen Import geladen; der Desktop fordert sie
nie an, weil er seinen Puffer über IPC bekommt.

## Tests

- `npm run lint`, `npm run typecheck`, `npm run build`: bestanden.
- **713 Unit-Tests**, darunter alle 652 CommonMark-Beispiele — jetzt gegen die
  **Spezifikation** statt gegen marked, mit `isEqualNode` statt Stringvergleich
  (siehe [008](decisions/008-parser-reference.md)). Die Zahl ist niedriger als
  die 2.036 nach P2.2, weil dieselben 652 Beispiele dort in drei `it.each`-Blöcken
  dreifach gezählt wurden; abgedeckt ist mehr, nicht weniger.
- **19 Playwright-Tests** bestanden.
- **Sieben native Abschnittsverträge** mit dem eingefrorenen Build bestanden,
  einer davon neu: „Full search result before the document is built".
- `npm run check` bricht weiterhin vor den Tests ab, weil `prettier --check`
  Benchmark-Rohdaten und `benchmarks/REPORT.md` als unformatiert meldet. Diese
  Rohdaten dürfen nicht umgeschrieben werden; die vier Einzelschritte wurden
  deshalb getrennt ausgeführt.

## Was bewusst nicht so umgesetzt wurde wie beschrieben

- **`indexText` ist nicht verschwunden.** 007 P2.4 erwartet das. Abschnitte mit
  rohem HTML haben keine Operationen, auf die sich ein Textoffset abbilden
  ließe; für sie bleibt ein DOM-Durchlauf die einzige Möglichkeit, einen Treffer
  zu einem Knoten zu machen. Dasselbe gilt für Abschnitte, deren Textknoten das
  Syntax-Highlighting ersetzt hat. Beide Fälle sind selten (0 von 995
  Abschnitten in `large.md`), und die WeakMap ist auf sie beschränkt.
- **Die Testreferenz wurde gewechselt, nicht nur die Vergleichsform.** Das ist
  Gegenstand von 008 und war vor dem Code zu entscheiden.
- **Der Parser läuft zusätzlich als WebAssembly.** 007 sieht nur den nativen Weg
  vor. Ohne das zweite Ziel hätten Browser-Vorschau und Playwright-Suite keinen
  Parser mehr gehabt, und der Spezifikationsvergleich hätte einen zweiten
  HTML-Baum (html5ever) als Schiedsrichter gebraucht. Begründung in 008
  Abschnitt 3.
- **`src/generated/hashline-markdown.wasm` ist eingecheckt.** Sonst brauchte
  `npm test` und `npm run build` eine Rust-Toolchain. Neu gebaut wird sie mit
  `npm run wasm`, nötig nur nach Änderungen an `src-tauri/markdown`.

## Offene Punkte

1. **P1.6 (Speicherdiagnose) ist unbeantwortet.** Die +70 bis +100 MiB über dem
   Plattformboden aus 007 Abschnitt 1 sind weiterhin nicht aufgeschlüsselt. Der
   Op-Buffer hält jetzt zwei Blobs statt eines HTML-Strings je Abschnitt; ob das
   netto mehr oder weniger ist als der Zustand vor Phase 2, wurde **nicht**
   gemessen. Das ist die größte offene Zahl.
2. **P1.7 (Startpfad) ist nach dieser Reihe das nächste Paket.** Der Start ist
   der einzige Wert, den Phase 2 praktisch nicht bewegt hat.
3. **`benchmarks/REPORT.md` und Entscheidung 004 sind nicht fortgeschrieben.**
   Sie beziehen sich weiterhin auf ältere Builds.
4. **`SPEC.md` beschreibt in Abschnitt 6/7 weiterhin marked und einen Worker.**
   Die Budgets sind unverändert und wurden nicht angefasst; die
   Architekturbeschreibung ist überholt und braucht eine eigene Entscheidung.
5. **Fußnotennummern in Fallback-Abschnitten** beginnen je Abschnitt neu; siehe
   008 Abschnitt 7. Betrifft nur Dokumente mit rohem HTML _und_ Fußnoten.
6. Die lokalen Doppel-rAF-Hilfsframes bleiben kein Nachweis präsentierter Pixel
   und keine Referenzabnahme. Die Maschine hat eine diskrete GPU und ist nicht
   die SPEC-Referenz.

## Hinweise zur Arbeitsumgebung

- Builds und native Tests **immer** über `resolveDesktopEnvironment` aus
  `scripts/desktop-env.mjs`; Rust-Toolchain (`/tmp/hashline-cargo`) und
  WebKit-Sysroot (`/tmp/hashline-sysroot`) liegen unter `/tmp` und überleben
  einen Neustart nicht. Für die WebAssembly-Ausgabe wird zusätzlich das Target
  `wasm32-unknown-unknown` gebraucht (`rustup target add`).
- Release-Build dieser Reihe: `npx tauri build --no-bundle` mit dieser Umgebung.
- Native Tests liefen über einen eigenen Session-Bus, eigene XDG-Verzeichnisse
  unter `/tmp/hashline-p23/` und die Ports 4565 (tauri-driver) / 4566
  (`WebKitWebDriver` aus dem Sysroot), mit
  `HASHLINE_WEBDRIVER_URL=http://127.0.0.1:4565`. Treiber und Bus wurden nach
  der Reihe beendet; fremde App- oder Treiberprozesse wurden nicht angefasst.
- Eingefrorene Builds unter `benchmarks/generated/` sind git-ignoriert; ihre
  Identität steht ausschließlich in den `*-source.json`-Manifesten.

Weiterführend: [007 – Performancepfad](decisions/007-performance-path.md),
[008 – Parserreferenz](decisions/008-parser-reference.md),
[Benchmarkbericht](../benchmarks/REPORT.md),
[Rendererentscheidung](decisions/004-rendering-gate.md),
[Architektur](architecture.md).

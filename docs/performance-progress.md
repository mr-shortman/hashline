# Performanceoptimierung – Übergabestand

Stand: 8. September 2026, nach dem Release-Build um 14:23 Uhr (Europe/Berlin).
**Auf ausdrücklichen Nutzerwunsch angehalten. Keine weiteren Optimierungen oder
Messreihen starten, bis die Arbeit wieder beauftragt wird.**

## Auftrag und Ergebnisstatus

Der zuletzt präzisierte Auftrag betrifft zwei Ziele:

1. Große Öffnungen mit der bestehenden WebView weiter beschleunigen; 10 MiB
   sollen unter zwei Sekunden geöffnet werden.
2. Den vollständigen Dokumentaufbau deutlich beschleunigen. Der Ausgangswert
   betrug rund 26,5 Sekunden im Median; vollständige Suche wartet auf diesen Aufbau.

**Beide Ziele sind noch nicht abschließend abgenommen.** Der vollständige Aufbau
ist in den bisherigen Zwischenständen erheblich schneller. Das Öffnungsziel
wurde in längeren Serien bislang nicht zuverlässig erreicht. Die neueste
Parseroptimierung ist gebaut und automatisiert getestet, aber noch nicht in einer
nativen Öffnungsserie vermessen. Frühere kurze Statusmeldungen mit etwa 11–12 s
Aufbauzeit beschrieben kurze Kontrollläufe; die längere Reihe fiel schlechter aus.

Die ursprünglich ebenfalls angefragte Speicher-/Idle- und Referenzabnahme ist
nicht Gegenstand der zuletzt priorisierten zwei Optimierungsziele. Ihr offener
Status bleibt im [Benchmarkbericht](../benchmarks/REPORT.md) dokumentiert.

## Aktueller Implementierungsstand

- `src/core/markdown/parser.ts`: eine Marked-Rendererinstanz für alle Abschnitte
  eines Parses; Unicode-Kleinschreibung ohne den unnötigen Locale-Aufruf.
- `src/core/markdown/tokenizer.ts`: zusätzliche sichere Präfixprüfungen für
  Links, Hervorhebungen, URLs und horizontale Linien; begrenzte Wiederverwendung
  von Listen-Bullet-Regulärausdrücken.
- **Neueste Änderung:** wiederkehrende äußere Listen verwenden ihre Tokenbäume
  innerhalb einer Lexerinstanz weiter. Höchstens 64 Einträge mit je höchstens
  8 KiB Quelltext. Mögliches HTML im Dokument sowie Referenz-/Tasksyntax in der
  Liste schließen diesen Pfad aus. Die globale Inline-Auflösung bleibt in Marked;
  veränderlicher äußerer Tokentext wird kopiert. Keine Wiederverwendung zwischen
  unterschiedlichen Dokumenten.
- `src/core/markdown/service.ts`: Worker-Ruhefrist von einer auf 30 Sekunden
  verlängert. Die vorherige Frist beendete den Worker bereits während des
  DOM-Aufbaus. Beschäftigte Worker werden bei Ersetzung weiterhin sofort beendet.
- `src/core/content/policy.ts`: die zusätzliche Attributprüfung nach DOMPurify
  besucht nur relevante Elemente. DOMPurify selbst bereinigt weiterhin jeden
  neuen Abschnitt vollständig.
- `src/features/document/DocumentViewport.tsx`: unveränderte, bereinigte
  Abschnitte behalten ihre verbundenen DOM-Knoten und Suchindizes. Bildhaltige
  Abschnitte werden immer neu aufgebaut, weil Ressourcenrechte zur Dokumentversion
  gehören. Die Erkennung prüft den tatsächlichen bereinigten DOM-Baum und deckt
  auch HTMLs historische `<image>`-Schreibweise ab. Codekopieren ist delegiert,
  sodass wiederverwendete Knoten keine alten Dokument-Handler behalten.
- `src/features/document/schedule.ts`: abbrechbare Aufgaben über eigene
  Window-Nachrichten. Aufbau und Entfernung streben 18-ms-Aufgaben an; die
  Sucharbeit bleibt in kleineren Aufgaben. Das ist keine Garantie, dass einzelne
  semantische Blöcke oder nachfolgende Browserarbeit unter 50 ms bleiben.
- `src-tauri/src/main.rs`, `src/platform/tauri.ts` und
  `src/platform/document-packet.ts`: native Binärübertragung mit u32-LE-Länge,
  kurzem UTF-8-JSON-Metadatenkopf und UTF-8-Markdown. Strikte Dekodierung und
  Paketgrenzenprüfung; keine versehentliche Entfernung eines zweiten U+FEFF.
- `benchmarks/desktop.py`: `--distinct-content` wechselt sämtliche
  Abschnittsüberschriften, um reine DOM-Wiederverwendung auszuschließen.
  Fixture-Hashes beider Varianten, wiederverwendete Abschnitte, Aufgabenzahl und
  Wartezeiten werden protokolliert. `--fixtures` funktioniert auch für Interaktion.

Ein zwischenzeitlich getesteter allgemeiner Inline-Token-Cache wurde wegen zu
geringen Vorteils wieder entfernt. Er ist **nicht** Teil des aktuellen Codes.

## Messungen sauber getrennt

Alle Zahlen stammen von der Entwicklungsmaschine mit diskreter RTX 3060,
WebKitGTK 2.52.6 und unkontrolliertem Betriebssystem-Dateicache. Öffnungswerte
beobachten einen JavaScript-Doppel-rAF-Hilfsframe; sie sind kein externer Nachweis
tatsächlich präsentierter Pixel und keine Abnahme auf der integrierten Referenz.

| Stand / Rohdaten                | Umfang                                                  | Öffnung 10 MiB inklusive Treiber              | Vollständiger Aufbau 10 MiB                 |
| ------------------------------- | ------------------------------------------------------- | --------------------------------------------- | ------------------------------------------- |
| Ausgangspunkt `final-open.json` | 30 große Öffnungen                                      | p95 2.627,3 ms                                | Median 26.485,5 ms                          |
| `optimization-3-distinct.json`  | 3 große Öffnungen                                       | nur diagnostischer Kurzlauf                   | Median rund 10,9 s                          |
| `optimized-distinct-open.json`  | 30 kleine, 30 mittlere, **17 große** Öffnungen; beendet | Median **2.162,5 ms**, Maximum **3.277,8 ms** | Median **13.561 ms**, Maximum **16.031 ms** |
| Aktueller Build `optimized-v2`  | noch keine native Öffnungsserie                         | **offen**                                     | **offen**                                   |

Die längere Zwischenreihe zeigt damit rund **49 % weniger Zeit** beim vollständigen
Aufbau gegenüber dem Ausgangsmedian, aber kein bestandenes Zwei-Sekunden-Ziel.
Ihre 17 großen Beobachtungen sind keine reguläre n=30-Abnahme. Sie wurde zugunsten
der weiteren Listenoptimierung beendet; die Rohdaten bleiben erhalten.

In derselben Zwischenreihe bestehen vollständige Größenreihen:

| Größe   | n   | Öffnung inklusive Treiber, Median / p95 | Vollständiger Aufbau, Median / p95 |
| ------- | --- | --------------------------------------- | ---------------------------------- |
| 100 KiB | 30  | 119,2 / 126,7 ms                        | 86,5 / 99 ms                       |
| 1 MiB   | 30  | 306,5 / 347,2 ms                        | 989,5 / 1.057 ms                   |

**Neueste Parserdiagnose:** Die Listenoptimierung erreichte in drei direkten
Aufrufen in der nativen WebKit-JavaScript-Umgebung **828 / 790 / 782 ms** für
10 MiB, jeweils 995 Abschnitte. Die vorangehende Präfixvariante erreichte dort
1.160 / 1.160 / 1.165 ms. Das ist eine isolierte Diagnose im Haupt-JS-Kontext,
kein Worker-/Öffnungsbenchmark. Die Zahlen wurden im Werkzeugprotokoll ausgegeben;
hier sind sie für die Übergabe festgehalten. Der Probe-Code liegt nur unter `/tmp`.

## Build und Tests

Der letzte Release-Build wurde erfolgreich abgeschlossen:

- Binary: `src-tauri/target/release/hashline`
- SHA-256: `8893477368e446e9ff48b15e5beac5ef5e125dc9e5eae8f1c2c12edc1c946ef3`
- [Quellmanifest](../benchmarks/results/optimized-v2-source.json): keine Änderungen
  an den erfassten Quelldateien während des Builds. Vor einer Fortsetzung Hash
  prüfen: ein späterer Build kann diese Datei überschreiben.
- **1.362 automatisierte Tests bestanden.** Die Tokenizer-Vergleiche prüfen
  CommonMark auch mit wiederholten Beispielen gegen unverändertes Marked sowie
  wiederholte Listen, Referenzen, Unicode und Paketdekodierung.
- **18 Browser-Bedienungstests bestanden**, einschließlich Abbruch, vollständiger
  Auswahl, Suche, DOM-Wiederverwendung und Bildressourcen-Isolation.
- TypeScript-Prüfung, ESLint und Release-Build bestanden. Rust-Formatprüfung und
  Clippy mit `-D warnings` bestanden vor den letzten ausschließlich frontendseitigen
  Änderungen. Drei Python-Benchmark-Verträge bestanden.
- Sechs native Abschnittsverträge bestanden mit dem früheren Build `opt4`:
  [Rohdaten](../benchmarks/results/optimization-4-contracts.json).
  **Native Verträge für den letzten Build stehen noch aus.**

Der ältere eingefrorene Build `benchmarks/generated/optimized-release/hashline`
hat SHA-256 `f9bdbdd4dadb4677e7954d4da4f122c73c2e45a3c166588e3c3aa5dff5f1ba4a`.
Er gehört zu `optimized-distinct-open.json` und darf nicht mit dem letzten Build
verwechselt werden. Die `optimization-*`-Dateien enthalten weitere Zwischenstände;
einige Reihen sind absichtlich unvollständig oder enthalten einen Treiberfehler.

## Nächste Schritte bei einer ausdrücklich beauftragten Fortsetzung

1. Letzten Binary-/Quellhash prüfen und den Build unter einem neuen Namen einfrieren.
   Die bestehenden Rohdateien und eingefrorenen Builds nicht überschreiben.
2. Native Abschnittsverträge und Desktop-Smoke-Test mit diesem Binary ausführen.
3. Je 30 Öffnungen für kleine, mittlere und große Dateien mit `--distinct-content`
   messen. Währenddessen keine Builds, UI-Tests oder weiteren Desktoptests starten.
   Das Zwei-Sekunden-Ziel anhand der vollständigen Serie beurteilen.
4. Separat identische Inhalte unter wechselnden Dateipfaden messen, um den Nutzen
   der DOM-Wiederverwendung auszuweisen. Beide Szenarien getrennt berichten.
5. Vollständige Suchlatenz bei 10 MiB und Menüreaktion am selben Build messen;
   dazu `--mode interaction --fixtures large --repetitions 30` verwenden.
6. Bei Überschreitungen weiter profilieren. Vollständige Suche wartet weiterhin
   auf vollständige DOM-Einfügung; echtes Streaming und unabhängige Volltextsuche
   vor Abschluss sind nicht implementiert. Atomare große Blöcke und einzelne
   Browser-/GC-Unterbrechungen bleiben bekannte Grenzen.
7. Ergebnisstatus und Zahlen in `benchmarks/REPORT.md` und Entscheidung 004 auf
   den tatsächlich vermessenen letzten Build beziehen. Keine allgemeine
   Performancefreigabe aus diesen lokalen Hilfszeiten ableiten.

## Hinweise zur Arbeitsumgebung

Die Änderungen sind **nicht committed**. Der gemeinsame Arbeitsbaum enthält auch
bereits vorhandene Arbeiten an Remote-Bildern, Barrierefreiheit und Titelleiste;
diese nicht zurücksetzen oder als ausschließlich zu dieser Optimierung gehörig
behandeln. Es wurden keine zusätzlichen Agenten eingesetzt.

Native Tests liefen über einen eigenen D-Bus, Ports 4565/4566 und isolierte
Einstellungen unter `/tmp/hashline-opt4-*`. Temporäre Busadressen und Prozess-IDs
können nach einer Unterbrechung ungültig sein. Für eine spätere Sitzung einen
eigenen Testtreiber starten; keine fremden App-/Treiberprozesse beenden.
Der zuletzt verwendete eigene Testtreiber wurde beim Anhalten beendet.
Builds verwenden `resolveDesktopEnvironment` aus `scripts/desktop-env.mjs`, weil
Rust-Werkzeuge und der WebKit-Sysroot teilweise unter `/tmp` liegen.

Weiterführend: [Benchmarkbericht](../benchmarks/REPORT.md),
[Rendererentscheidung](decisions/004-rendering-gate.md),
[Architektur](architecture.md).

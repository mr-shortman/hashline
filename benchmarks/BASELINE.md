# Hashline — erster Release- und Benchmarkbericht

Stand: 8. September 2026. **Keine v1-Freigabe.** Funktionstests und Paketbau sind
erfolgreich; wesentliche Performancebudgets sind noch nicht erfüllt. Die
[Rendererentscheidung](../docs/decisions/004-rendering-gate.md) hält die M0-Freigabe
deshalb zurück. M1–M3 sind funktional implementiert, aber nicht vollständig abgenommen.

## Umgebung und Verfahren

| Merkmal        | Entwicklungsmaschine                                               |
| -------------- | ------------------------------------------------------------------ |
| Distribution   | Ubuntu 26.04 LTS, x86_64                                           |
| CPU            | AMD Ryzen 9 9900X, 12 Kerne / 24 Threads                           |
| RAM            | etwa 30 GiB für Linux verfügbar                                    |
| Grafik         | NVIDIA GeForce RTX 3060, diskret                                   |
| Massenspeicher | Samsung SSD 970 EVO 500 GB und 870 QVO 1 TB                        |
| Sitzung        | Wayland; zusätzlicher erzwungener X11-Smoke-Test                   |
| WebKitGTK      | 2.52.6 (`libwebkit2gtk-4.1-0`)                                     |
| Build          | Tauri 2.11.5, Rust 1.98.1, Node 22.22.1; Release mit LTO           |
| Binary SHA-256 | `58dc34d602d2c77a07aac5604bd1cdb101058cf6e8ec58ddad653e3bae12fe4d` |

Diese Maschine ist **nicht** die geforderte integrierte-Grafik-Referenz. Physische
Displayfrequenz, Desktop-Skalierung und 120-Hz-Betrieb wurden nicht verifiziert.
Gemessen wurde der echte Release-Build über Tauri-/WebKitGTK-WebDriver, nicht der
Vite-Entwicklungsserver. Das erzeugte `.deb` wurde entpackt und geprüft, aber nicht
mit Rootrechten in eine saubere Zielinstallation installiert. Die Messungen
ersetzen daher auch keine Messung eines installierten Pakets.

Die Rohdaten in [results/webkit.json](results/webkit.json) enthalten 30 Öffnungen
der kleinen und mittleren Fixture, jeweils wechselnde Pfade, sowie einen ersten
10-MiB-Belastungslauf. Es gibt keine kontrollierte kalte Dateicache-Messung.
`cliToObservedFrameMs` enthält CLI-Prozess- und Treiberkosten;
`content-to-frame` misst bis nach zwei Animation-Frames und beweist keine
tatsächliche Bildschirmpräsentation. Start-/Scrollbudgets werden daraus nicht
als bestanden abgeleitet.

## Zeitmessung nach Parserkorrektur

Werte in ms, jeweils Median / p95. Große Datei: ein Einzelwert, keine belastbare
Perzentilstatistik.

| Phase                                       | 100 KiB (30 Läufe) |  1 MiB (30 Läufe) | 10 MiB (1 Lauf) |
| ------------------------------------------- | -----------------: | ----------------: | --------------: |
| Natives Lesen                               |        0,04 / 0,06 |       0,19 / 0,22 |             4,1 |
| Worker-Parsing                              |          26,5 / 30 |         268 / 280 |           2.594 |
| Bereinigung                                 |            28 / 31 |       378,5 / 403 |           3.260 |
| Öffnungsauftrag bis bereinigter Inhalt      |            55 / 60 |         663 / 702 |           6.003 |
| DOM-Einfügung, Layout und Hilfsframe        |          59,5 / 74 |         673 / 716 |           6.430 |
| CLI bis vom Treiber beobachteter Hilfsframe |      181,2 / 188,6 | 1.420,3 / 1.486,5 |        12.736,8 |

Das initiale Parserproblem ist behoben: Der Median der kleinen Fixture sank von
868 auf 26,5 ms. Die [Ausgangsrohdaten](results/webkit-before-tokenizer.json) und
die [Parserentscheidung](../docs/decisions/002-webkit-parser.md) dokumentieren den
Vergleich. Die mittlere und große Fixture überschreiten weiterhin die Ziele;
Bereinigung und Layout blockieren den Hauptthread deutlich länger als 50 ms.

Die Fixture-Metadaten stehen in [results/fixtures.json](results/fixtures.json).
Die gemischten Dateien erzeugen etwa 16.187, 163.803 bzw. 1.620.083 Parser-DOM-Knoten.
Bildlast ist in diesen Zeitmessungen nicht enthalten. Die Bild-Fixture ist ein
1 × 1 Pixel großes PNG mit 68 Bytes; viele/große Bilder wurden noch nicht vermessen.

## Speicher und Leerlauf

Die rekursive App-Prozessgruppe wird über Linux `smaps_rollup` als PSS erfasst.
Die Testtreiber gehören nicht zur summierten App-Prozessgruppe. Rohdaten:
[results/stability.json](results/stability.json).

Der erste Versuch ohne Aufwärmwechsel erreichte etwa 222,4 MiB nach dem ersten
Öffnen und 303,9 MiB nach 50 Wechseln. Ein anschließender Versuch mit zehn
Aufwärmwechseln ergab 264,1 → 294,4 MiB, entsprechend +11,5 %. Die erste Messung
belegt deshalb kein fortlaufendes Leck; das absolute anfängliche Budget von
200 MiB wird auf dieser Maschine trotzdem überschritten.

Die abschließende Reihe mit korrigiertem Sampler und erneut zehn Aufwärmwechseln
ergab **262,0 → 294,1 MiB PSS (+12,3 %)**. Das relative 20-%-Ziel wird in dieser
Reihe eingehalten, das absolute 200-MiB-Budget nicht. Die Prozessgruppe änderte
sich während der Baseline, weshalb dort kein CPU-Wert ausgewiesen wird. Nach
den 50 Wechseln war sie stabil; der Mittelwert betrug **0,034 % eines CPU-Kerns**
über 30 Sekunden. Das ist ein erfolgreicher lokaler Idle-Stichprobentest und
ersetzt keine vollständige Referenzabnahme.

Die ursprüngliche CPU-Differenzbildung konnte bei endenden Hilfsprozessen einen
negativen Wert liefern. Diese Werte in `stability-cold-baseline.json` und
`stability-warm-first.json` sind keine gültige Idle-Abnahme. Der korrigierte
Sampler kennzeichnet wechselnde Prozessgruppen als nicht auswertbar, statt einen
negativen oder zu kleinen Verbrauch zu behaupten. Die endgültige Messung wird in
`stability.json` gespeichert. Kein <1-%-CPU-Nachweis wird aus ungültigen Werten abgeleitet.

## Durchgeführte Prüfungen

- Formatprüfung, ESLint, TypeScript Strict und Frontend-Produktionsbuild erfolgreich.
- 674 Unit-/Vertragstests erfolgreich, einschließlich aller 652 CommonMark-
  Vergleichsfälle, GFM, Bereinigung, URLs, Einstellungen, Suche und veralteter Anfragen.
- Fünf Chrome-Bedienungstests erfolgreich, einschließlich 200 % Textzoom, schmalem
  TOC, Fokus, Original-Codekopie und dem Fallback ohne CSS Custom Highlights.
- Rustfmt, Clippy ohne Warnungen und zwei native Vertragstests erfolgreich;
  Symlink-/Verzeichnisgrenzen, Größenlimit, URL-Dekodierung und FIFO-Abweisung geprüft.
- Echter WebKitGTK-Release: CLI, Single-Instance, aufruferspezifisches Verzeichnis,
  Unicode-/Leerzeichen, lokale Bilder, relative Links/Fragmente, Suche, Auswahl,
  atomisches Speichern, Löschen/Wiederanlegen, Leseanker und native Zugriffssperre.
- Diese sieben nativen Prüfschritte bestehen sowohl mit erzwungenem
  `GDK_BACKEND=wayland` als auch `GDK_BACKEND=x11`:
  [Wayland-Protokoll](results/desktop-wayland.json), [X11-Protokoll](results/desktop-x11.json).
- Debian-Paket erzeugt; Binary/Paketname `hashline`, Desktop-Felder und kanonischer
  Desktop-Dateiname `de.kalendium.Hashline.desktop` kontrolliert.

## Offene Abnahme

| Punkt aus SPEC                                                  | Ergebnis                                                                     |
| --------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Kernfunktionalität im Release                                   | automatisierte Smoke-Tests bestanden; manuelle OS-Aktionen ergänzen          |
| Kleine Datei ≤150 ms in laufender Instanz                       | Hilfswerte vorhanden; exakte Abnahme offen                                   |
| Mittlere Datei ≤500 ms                                          | überschritten                                                                |
| Große Datei lesbar ≤2 s und durchgängig bedienbar               | überschritten; Hauptthread-Phasen blockieren                                 |
| Keine Interaktionsaufgaben >50 ms                               | für Bereinigung/Layout nicht erfüllt                                         |
| Dokumentweite Suche ≤150 ms bei 1 MiB                           | funktional geprüft, Zeitmessreihe offen                                      |
| Kalt-/Warmstart mit externer sichtbarer Darstellung             | offen                                                                        |
| Compositor-Scrollframes bei 60/120 Hz                           | offen                                                                        |
| PSS ≤200 MiB                                                    | auf dieser Maschine überschritten                                            |
| Stabilität nach 50 Wechseln                                     | Rohdaten vorhanden; aufgewärmter Vergleich, keine dauerhafte Langzeitaussage |
| Idle-CPU <1 %                                                   | keine abgeschlossene Abnahme                                                 |
| Saubere Installation, Dateimanager, reales Drag-and-drop        | offen                                                                        |
| Manuelle Theme-/Kontrast-/HiDPI-/fraktionale Skalierungsabnahme | offen                                                                        |

Die verbleibenden Abweichungen sind echte offene Arbeit. Das Paket ist ein
funktionsfähiger Entwicklungsstand und wird nicht als Erfüllung der vollständigen
Definition of Done aus `SPEC.md` ausgegeben.

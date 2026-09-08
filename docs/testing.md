# Prüfungen und reproduzierbare Desktop-Tests

Die regulären Befehle stehen im README. CI baut zusätzlich ein Debian-Paket auf
Ubuntu 24.04. Das ist ein Build-Kompatibilitätscheck; die gemessene lokale
Referenzumgebung ist Ubuntu 26.04 mit WebKitGTK 2.52.6.

## Echte WebView

```sh
cargo install tauri-driver --locked
sudo apt install webkitgtk-webdriver  # auf Ubuntu 24.04: webkit2gtk-driver
npm run bundle
tauri-driver --native-driver /usr/bin/WebKitWebDriver
```

In einem weiteren Terminal derselben grafischen Sitzung:

```sh
python3 tests/desktop/smoke.py src-tauri/target/release/hashline
npm run fixtures
python3 benchmarks/desktop.py src-tauri/target/release/hashline 30
python3 benchmarks/stability.py src-tauri/target/release/hashline
```

Der Smoke-Test verwendet ausschließlich temporäre Dokumente. Er prüft CLI und
Single-Instance samt aufruferspezifischem Arbeitsverzeichnis, Unicode-/Leerzeichen,
relative Links und Fragmente, lokale Bilder, Suche in Code/Tabellen, Auswahl,
atomisches Speichern, Löschen/Wiederanlegen, Leseposition und native Zugriffssperre.
Screenshot und Ergebnis stehen danach in `test-results/`.

Für einen separaten Sitzungstest den Treiber mit `GDK_BACKEND=wayland` bzw.
`GDK_BACKEND=x11` starten. Nicht gleichzeitig dieselbe Hashline-Anwendungs-ID in
zwei Sitzungen testen. Für isolierte Präferenzen vor dem Treiberstart
`XDG_CONFIG_HOME`, `XDG_DATA_HOME` und `XDG_CACHE_HOME` auf eigene Testverzeichnisse
setzen; `XDG_RUNTIME_DIR` und den Session-D-Bus unverändert lassen.

## Performance

`npm run fixtures` erzeugt deterministische 100-KiB-, 1-MiB- und 10-MiB-Dateien sowie
lange Zeile, tiefe Liste, große Tabelle und viele kleine Blöcke. `metadata.json`
enthält exakte Bytes, Tokenblöcke, erwartete Parser-DOM-Knoten und SHA-256.
Die DOM-Zahl umfasst Element-/Textknoten des ungeänderten Marked-HTMLs, nicht die
zusätzlichen Viewport-Bedienelemente. Bildlast muss separat gemessen werden.

Der Desktop-Benchmark speichert Rohdaten, Median und p95. Die CLI-Hilfszeit enthält
Testtreiber-/Prozessaufrufkosten. `content-to-frame` endet nach zwei Animation-
Frames; sie beweist keine tatsächlich präsentierten Compositor-Frames. Die
Spezifikation verlangt zusätzlich externe Start-/Darstellungsaufnahme und
Compositor-/Profiler-Auswertung. Keine dieser Hilfszeiten wird als bestandene
Start- oder Scroll-Abnahme ausgegeben.

```sh
HASHLINE_DIAGNOSTICS=1 hashline benchmarks/generated/small.md
python3 benchmarks/process-sample.py <hashline-pid> 30 > /tmp/hashline-memory.json
```

Der Sampler erfasst die rekursive Prozessgruppe über `/proc`, summiert PSS und
CPU-Ticks und kennzeichnet nicht lesbare PSS-Werte. Nach 50 Dateiwechseln dieselbe
Messung wiederholen und mit der aufgewärmten Ausgangslage vergleichen. Unter
Sandbox-/Ptrace-Beschränkungen kann `/proc/<pid>/smaps_rollup` nicht lesbar sein;
fehlende Werte dürfen nicht als Nullverbrauch interpretiert werden.

## Manuelle Freigabe

Vor einer v1-Freigabe bleiben Installation auf sauberer Zielumgebung, tatsächlicher
Dateimanager-Aufruf und Drag-and-drop, System-Clipboard/Linköffnen, visuelle
Kontrast-/Skalierungsprüfung und die vollständigen SPEC-Performancebudgets zu
bestätigen. Die Anwendungs-ID `de.kalendium.Hashline` ist aus der Spezifikation
übernommen; ihre Herausgeberbestätigung wird nicht aus einem erfolgreichen Build
abgeleitet.

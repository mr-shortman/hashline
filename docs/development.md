# Desktop-Entwicklung

Nach dem Setup aus der README genügt im Projektverzeichnis:

```sh
npm run dev:demo
```

Das startet Vite und ein natives **Hashline Dev**-Fenster mit
`tests/fixtures/reader.md`. Die Dev-App hat eine eigene Anwendungs-ID und getrennte
Einstellungen zur installierten Release-App. Der erste Rust-Build dauert länger;
weitere Starts verwenden den vorhandenen Build.

```sh
npm run desktop                           # Leeres Dev-Fenster
npm run desktop -- --file "./meine Datei.md"
npm run dev:doctor                        # Rust und GTK/WebKitGTK prüfen
npm run desktop -- --no-watch             # Rust-Watcher ausschalten
```

`npm run dev:desktop` ist ein Alias für `npm run desktop`. Vite wird automatisch
gestartet; dafür keinen zweiten `npm run dev`-Prozess öffnen. Port 1420 muss frei
sein. Beenden mit Strg+C beendet auch die gestarteten Dev-Prozesse.

## Was aktualisiert sich automatisch?

| Änderung                                      | Verhalten                                                                                                                                                                   |
| --------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Geöffnetes Markdown extern speichern          | Der native Dateiwatcher lädt den Inhalt nach und erhält die Leseposition.                                                                                                   |
| CSS                                           | Vite tauscht die Styles im laufenden Fenster aus.                                                                                                                           |
| React-Komponente                              | Fast Refresh aktualisiert die Komponente und erhält lokalen Zustand, soweit die Änderung das erlaubt.                                                                       |
| Einstiegspunkt, gemeinsame Module oder Worker | Je nach Modul führt Vite einen vollständigen Seitenreload aus. Im Desktop-Dev-Modus wird die zuletzt erfolgreich geöffnete Datei derselben Webview-Sitzung wieder geöffnet. |
| Rust oder Tauri-Konfiguration                 | Tauri baut die App neu und startet sie erneut. Die mit `--file` angegebene Datei wird erneut übergeben.                                                                     |

Bei einem vollständigen Seitenreload können Suche und andere flüchtige UI-Zustände
zurückgesetzt werden. Die gespeicherte Leseposition wird beim erneuten Öffnen
angewendet. Nach einem nativen Neustart ist eine neue Webview-Sitzung aktiv;
für einen reproduzierbaren Start deshalb `--file` oder `dev:demo` verwenden.

Die Dev-Wiederherstellung verleiht keine zusätzlichen Dateirechte: Rust prüft
weiterhin, ob der Pfad in der laufenden Sitzung freigegeben wurde. Sie ist nicht
im Produktionsbuild enthalten. Die separate Dev-CSP erlaubt Vites WebSocket und
das Inline-Skript für React Fast Refresh. Die Release-CSP bleibt unverändert.

`src-tauri/target`, generierte Tauri-Dateien und Benchmark-Ausgaben lösen keine
unnötigen Frontend-Reloads aus. Das allgemeine Verhalten beschreibt die
[Tauri-Dokumentation zum Entwicklungsmodus](https://v2.tauri.app/develop/).

## Debuggen und laufend prüfen

Strg+Umschalt+I öffnet unter Linux den WebKit-Inspector für DOM, CSS, Console und
Netzwerk. Rust-Ausgaben und Build-Fehler stehen im Startterminal. Strg+R in der App
lädt das Markdown neu; einen vollständigen Frontend-Reload kann man im Inspector
mit `location.reload()` auslösen.

In einem weiteren Terminal lassen sich Prüfungen im Watch-Modus starten:

```sh
npm run test:watch
npm run typecheck:watch
```

Zum manuellen Smoke-Test die Demo starten, die Markdown-Datei im Editor ändern,
eine CSS-Regel anpassen und anschließend `location.reload()` im Inspector
ausführen. Inhalt und Styles sollen aktualisieren; nach dem Seitenreload muss die
Datei wieder erscheinen.

## Lokale Toolchain ohne lange Shell-Befehle

Der Starter berücksichtigt vorhandene Umgebungsvariablen und Rust unter
`~/.cargo/bin`. Individuelle Installationspfade können in einer ignorierten Datei
festgehalten werden:

```sh
cp .env.desktop.example .env.desktop.local
```

Dort bei Bedarf `CARGO_HOME`, `RUSTUP_HOME` und `HASHLINE_DEV_SYSROOT` setzen.
Die Werte sind wörtliche Pfade, ohne Shell-Erweiterungen wie `$HOME` oder `~`.
Bereits exportierte Umgebungsvariablen haben Vorrang.

In der vorbereiteten Arbeitsumgebung erkennt der Starter außerdem die vorhandene
Toolchain unter `/tmp/hashline-cargo` und `/tmp/hashline-rustup` sowie die
Buildpakete unter `/tmp/hashline-sysroot`, falls die normalen Werkzeuge fehlen.
Er meldet diesen Fallback ausdrücklich. Er installiert nichts; temporäre Dateien
können beim Neustart verschwinden. Für ein dauerhaftes Setup die Installation
aus der README verwenden. Die Pfadbehandlung betrifft nur den Dev-Starter;
direkte Cargo-Aufrufe und `npm run bundle` verwenden ihre normale Umgebung.

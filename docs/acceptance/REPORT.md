# Visuelle und Zugänglichkeitsabnahme

Stand: 8. September 2026. Dieser Bericht betrifft die Darstellung, Tastaturbedienung
und Fenstersteuerung sowie die ergänzte Remote-Bildfreigabe. Er erteilt keine
vollständige v1- oder Performancefreigabe; dafür gilt weiterhin der
[Benchmarkbericht](../../benchmarks/REPORT.md).

**Ergebnis: Die angeforderte visuelle und Tastaturabnahme ist auf der
Linux-Referenzmaschine abgeschlossen.** Alle 48 Kombinationen und die nativen
Fensteraktionen bestehen im unten identifizierten installierten Paket.

## Abnahmematrix und Nachweise

Jede Zeile enthält zwölf Fälle: zwei Fensterbreiten × drei Themes × zwei
Dokumenttextgrößen. Breiten und Höhen sind logische Pixel.

| Backend           | Displayskalierung | Systemtheme | Fensterbreiten / Höhe | Fälle | Ergebnis                                           |
| ----------------- | ----------------- | ----------- | --------------------- | ----- | -------------------------------------------------- |
| Wayland           | 100 %             | Hell        | 360 und 1440 / 780    | 12    | [bestanden](results/wayland-100-light/result.json) |
| Wayland           | 125 %             | Dunkel      | 360 und 1200 / 720    | 12    | [bestanden](results/wayland-125-dark/result.json)  |
| Wayland           | 200 %             | Hell        | 360 und 900 / 480     | 12    | [bestanden](results/wayland-200-light/result.json) |
| X11 über Xwayland | 100 %             | Dunkel      | 360 und 1440 / 780    | 12    | [bestanden](results/x11-100-dark/result.json)      |

In jedem Lauf zusätzlich bestanden: Suchfokus einschließlich wiederholtem Strg+F,
Fokusbegrenzung und Rückgabe bei Inhaltsverzeichnis und Einstellungen, horizontales
Tabellenscrollen per Tastatur, sichtbarer Fokus, Details-Schalter, native
Minimierung, Wiederherstellung und Fokussierung über den Anwendungsumschalter,
Maximieren/Wiederherstellen per Enter und Leertaste, Verschieben, Größenänderung
an der Fensterecke, Titelleisten-Doppelklick und Schließen per Tastatur.

Visuell geprüfte Beispiele: [schmales Menü bei 200 % Textzoom](results/wayland-100-light/360-dark-200-menu.png),
[Inhaltsverzeichnis und Fokus](results/wayland-100-light/360-light-200-outline.png),
[Syntaxfarben und Tabellen](results/wayland-100-light/1440-dark-100-doc-code-und-tabellen.png),
[Compositor bei 125 %](results/wayland-125-dark/compositor-test-window.png),
[Compositor bei 200 %](results/wayland-200-light/compositor-test-window.png),
[sichtbarer Bildersatz](results/wayland-200-light/900-dark-200-doc-bilder-und-abschluss.png)
und [X11-Systemtheme](results/x11-100-dark/1440-system-100.png).
Die [125-%-](results/display-1.25.json) und [200-%-Monitorprotokolle](results/display-2.0.json)
bestätigen die tatsächliche Skalierung und Wiederherstellung der Ausgangsanordnung.

Die [Remote-Verträge](results/remote-images.json) und der
[Desktop-Smoke-Test](results/desktop-smoke.json) bestehen ebenfalls.
Die [Buildprüfung](results/build-validation.json) umfasst ESLint, TypeScript,
1.342 Unit-Tests, 16 UI-Tests, vier Rust-Tests, Clippy mit `-D warnings` und das
fertige Debian-Release-Paket. Die [Prüfskript-Prüfsummen](results/test-manifest.json)
identifizieren den verwendeten Ablauf. Die Ergebnisse sind keine vollständige
Screenreader-Zertifizierung oder Abnahme anderer Desktop-Umgebungen.

## Prüfgegenstand und Verfahren

Geprüft wurde die **installierte Release-App**, aus dem fertigen Debian-Paket als
Benutzerinstallation unter `~/.local/lib/hashline/hashline`, mit CLI-Verknüpfung
`~/.local/bin/hashline`, Desktop-Eintrag, Icons und MIME-Registrierung. Die
Installation verändert keine bevorzugte Dateizuordnung. Eine systemweite
Root-Installation auf einer sauberen Zielmaschine gehört nicht zu diesem Bericht.

Paket: `Hashline_0.1.0_amd64.deb`, archiviert unter
`src-tauri/target/acceptance/`. SHA-256:
`35dbdc9d93ab9586d9931c0ddfb0a65f3b411c70f9ef4ef027f1a641ea8a63eb`.
Das tatsächlich gestartete Binary hat SHA-256
`db78bf8996d57124d90b92557075518fc7b98f18919231e731b224b09227d8e1`.
Der Build wurde aus einer fixierten Arbeitskopie erzeugt; ihre
[Quelldatei-Prüfsummen](results/source-manifest.json) und das danebenliegende
Archiv `Hashline_0.1.0_sources.tar.gz` identifizieren den geprüften Stand auch bei
weiteren Änderungen im Arbeitsverzeichnis.

Referenz: Ubuntu 26.04, GNOME Shell/Mutter 50.1, WebKitGTK 2.52.6, NVIDIA RTX 3060.
Die vorhandenen 1920×1080-Ausgaben liefen bei etwa 60 Hz. Die Display-Skalierung
wird im Compositor tatsächlich auf 100, 125 und 200 Prozent gestellt; sie ist
keine Browseremulation und kein CSS-Transform. Monitor- und Systemtheme-Einstellungen
werden nach jedem Lauf wiederhergestellt. Bei 125 und 200 Prozent meldet WebKit
DPR 2; die logischen Bildschirmgrößen unterscheiden sich entsprechend.

Die Prüffixture enthält Fließtext, Überschriften, Unicode, Links, Zitate,
Syntaxfarben einschließlich CSS-Selektoren und Literalen, Tabellen mit und ohne
Überbreite, Code, deaktivierte Aufgaben, Details, lokale und fehlende Bilder.
Jede Matrix kombiniert Hell/Dunkel/System mit 100/200 Prozent **Dokumenttextzoom**
sowie schmalen und breiten Fenstern. UI-Bedienelemente behalten ihre Größe.

`tests/desktop/accessibility.py` verwendet den installierten Build und dessen
native Adapter über Tauri-/WebKitGTK-WebDriver. Die Referenzprüfung verwendet
zusätzlich lokale Mutter-Eingaben für tatsächlichen Betriebssystem-Fokus,
Verschieben, Größenänderung und Doppelklick. Ein erfolgreich versandter
synthetischer DOM-Mausklick gilt nicht als Nachweis einer Fensterbewegung.
WebView-Screenshots zeigen den gerenderten Puffer; separate PipeWire-Aufnahmen
zeigen die Ausgabe des Compositors. Nur Bilder des Testfensters werden archiviert.

## Befunde und Korrekturen

- Syntaxfarben verwenden jetzt vollständige Theme-Tokens: Auch Literale und
  CSS-Selektoren wechseln im dunklen und systemgesteuerten Theme korrekt.
- Sucheingabe-Platzhalter verwendet eine explizite kontrastreiche Farbe.
- Das schmale Inhaltsverzeichnis sperrt den Hintergrund auch für assistive
  Navigation. Fokus bleibt im Overlay und kehrt nach Escape zurück. Die Rückgabe
  erfolgt erst nach Aufheben von `inert`, weil WebKit vorher keinen Fokus annimmt.
- Das Einstellungsfenster hält den Tastaturfokus und scrollt bei geringer Höhe.
  Wiederholtes Strg+F fokussiert auch eine bereits geöffnete Suche.
- Der Dokumentbereich zeigt seinen Fokus innerhalb der sichtbaren Fläche.
  Breite Tabellen und Code bleiben innerhalb ihrer Bereiche scrollbar.
- Überbreite Tabellen zerlegen Spaltenwörter nicht mehr in einzelne Buchstaben.
- Fehlende und gesperrte Bilder zeigen echte Textplatzhalter. Das bloße `alt`-Attribut
  eines Bildes ohne `src` wurde von WebKit nicht zuverlässig gezeichnet; der sichtbare
  Ersatztext ist auch für assistive Navigation verfügbar. Nach Freigabe entfernt
  ein erfolgreich geladenes Remote-Bild seinen Platzhalter.
- Abschnittssprünge behalten ihre Zielposition beim weiteren Einfügen großer
  Dokumente, bis der Benutzer erneut navigiert.
- Unter Linux stellt GTK ein erneut angefordertes Fenster mit einer gemeinsamen
  Präsentationsaktion wieder her. GNOME darf einer CLI-Instanz ohne gültigen
  Aktivierungskontext trotzdem den automatischen Fokus verweigern. Wiederaktivieren
  über den normalen Desktop-Anwendungsumschalter wurde separat geprüft; dieser
  Compositor-Schutz wird nicht umgangen.

Gemessen wird der Kontrast der berechneten Text-/Symbolfarbe gegen den tatsächlichen
Hintergrund einschließlich transparenter Elternflächen. Deaktivierte und versteckte
Elemente sind ausgenommen. Die Messung wartet die 120-ms-Übergänge ab. Der niedrigste
Wert der abgeschlossenen Matrix beträgt **4,8467:1** und überschreitet
4,5:1. Sichtbarer Tastaturfokus wird durch tatsächliche Tab-Eingaben geprüft.

## Remote-Bilder und bewusste Einschränkung

Remote-Bilder sind implementiert und bleiben ohne explizite Freigabe blockiert.
Die Aktion erklärt die IP-Übertragung und gilt ausschließlich für die angezeigte
Dokumentrevision. Geänderte Revisionen und Dateiwechsel setzen sie zurück.
Die Freigabe tauscht den Dokument-DOM nicht aus und erweitert weder die CSP noch
native Dateirechte. Der native Ressourcenpfad prüft Rasterformat, Byte- und
Pixelgrenzen. Downloads sind zeitlich und insgesamt begrenzt; beim Schließen
werden wartende und laufende Aufträge abgebrochen.

Der lokale HTTP-Vertragstest prüft: keine Anfrage vor Freigabe, nativer Zugriff
vor Freigabe verweigert, Bedienung per Tastatur, erfolgreiches Rasterbild und
Weiterleitung, SVG und gefälschter MIME-Typ verweigert, Grenzen von 16 MiB und
24 MP, Weiterleitungsschleife verweigert, keine Cookies/Referrer/Zugangsdaten,
Freigaberücksetzung, unbekannte Handles, maximal 64 Anfragen und vier gleichzeitige
Downloads sowie Dateiwechsel während eines laufenden Downloads.
Die Aufnahmen zeigen den Zustand [vor](results/remote-blocked.png) und
[nach Freigabe](results/remote-approved.png); die erfolgreichen Testbilder sind
absichtlich nur ein Pixel groß, die abgewiesenen Ressourcen behalten Ersatztext.

**SVG bleibt bewusst unimplementiert**, lokal, remote und als eingebettetes HTML.
Die Freigabe betrifft PNG, JPEG, GIF und WebP. Alle Grenzen und ihre Begründung
stehen in [001-resources.md](../decisions/001-resources.md). Animationsspeicher und
die vollständigen App-Performancebudgets werden dadurch nicht als abgenommen erklärt.

## Reproduktion

```sh
# Installiertes Release-Binary und lokal verfügbare Treiber:
export HASHLINE_INSTALLED_BINARY="$HOME/.local/bin/hashline"
export HASHLINE_TAURI_DRIVER="$(command -v tauri-driver)"
export HASHLINE_NATIVE_DRIVER="$(command -v WebKitWebDriver)"
python3 tests/desktop/run_acceptance.py wayland wayland-100-light
python3 tests/desktop/run_acceptance.py wayland remote
python3 tests/desktop/run_acceptance.py x11 x11-100-dark
```

Die Hilfsskripte benötigen Python mit GI/Gio und einen laufenden GNOME/Mutter-Desktop.
Sie verwenden isolierte App-Präferenzen und stellen das Systemtheme anschließend
wieder her. `reference_scale.py` skaliert unterstützte, nicht gedrehte Einzelmonitore
vorübergehend gleich und stellt danach die komplette ursprüngliche Konfiguration
wieder her. Für andere Desktop-Umgebungen die Skalierung über deren Einstellungen
wählen und die GUI-Prüfung mit den jeweiligen nativen Eingabewerkzeugen ausführen.

```sh
python3 tests/desktop/reference_scale.py 1.25 \
  python3 tests/desktop/run_acceptance.py wayland wayland-125-dark --wide 1200 --height 720
python3 tests/desktop/reference_scale.py 2 \
  python3 tests/desktop/run_acceptance.py wayland wayland-200-light --wide 900 --height 480
# Optionale tatsächliche Compositor-Aufnahme (GStreamer/PipeWire erforderlich):
HASHLINE_CAPTURE_COMPOSITOR=DP-3 python3 tests/desktop/run_acceptance.py wayland capture-dark
```

Die Laufprotokolle und Screenshots entstehen unter `test-results/acceptance/`.
Mit `HASHLINE_ACCEPTANCE_OUTPUT` lässt sich ein unabhängiges Ausgabeverzeichnis
wählen, damit ein anderer Testlauf die Nachweise nicht löscht.
Die archivierten Nachweise dieses Berichts liegen im Unterverzeichnis `results/`.

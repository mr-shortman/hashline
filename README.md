# Hashline

A fast, quiet Markdown viewer for Linux. Ein nativer, schreibgeschützter Reader
in Rust auf GTK4 — ein Prozess, ein Fenster, eine Datei, keine Konten, keine
Telemetrie. Keine WebView, kein HTML, kein CSS und keine JavaScript-Laufzeit im
ausgelieferten Programm.

Dateien lassen sich per Dialog, Drag-and-drop oder `hashline datei.md` öffnen;
ein weiterer Aufruf übergibt die Datei an das laufende Fenster. Hashline bietet
GFM-Darstellung, Inhaltsverzeichnis mit Kennzeichnung des aktuellen Abschnitts,
Textsuche, Code-Kopieren, Syntaxhervorhebung, lokale Bilder, relative
Markdown-Links, automatische Aktualisierung unter Erhalt der Leseposition,
Lesepositionen, Themes und Textzoom.

**Status:** v1 ist abgeschlossen, einschließlich `.deb` für Ubuntu 26.04
amd64. Was v1 nicht kann und wo es seine eigenen Ziele verfehlt, steht in
[Einschränkungen](docs/limitations.md); die Zahlen in
[Messwerte](docs/metrics.md). Woran als Nächstes gearbeitet wird, steht in der
[Roadmap](docs/roadmap.md).

Der Dokumentbereich ist ein eigenes Widget: es setzt den Op-Buffer des Parsers
mit Pango und zeichnet mit GSK, blockweise virtualisiert. Für kein Dokument
existiert ein Zustand, in dem alles gesetzt ist — eine 10-MiB-Datei wird lesbar,
indem 15 von rund 145 000 Blöcken gesetzt werden. Ein Block, der viel höher als
ein Schirm ist, zerfällt dafür in Teile: ein Codeblock mit 70 000 Zeilen ist
sonst genau ein Block, und Virtualisierung käme nicht an ihn heran
([Architektur](docs/architecture.md)).

## Entwickeln

```sh
sudo apt install libgtk-4-dev build-essential pkg-config
cargo run -p hashline -- README.md
```

Mehr in [docs/development.md](docs/development.md): Prüfungen, Fenstertests,
Messungen und Paketbau.

## Installation

Paketbau und Installation auf Ubuntu 26.04:

```sh
python3 packaging/build_deb.py
sudo apt install ./target/packages/hashline_0.1.0-1_amd64.deb
```

Voraussetzungen, Entfernen und isolierter Installationstest stehen in
[docs/development.md](docs/development.md). Hashline erscheint im
Anwendungsmenü und unter „Öffnen mit“ für Markdown. Eine bestehende
Standardzuordnung bleibt erhalten.

## Bedienung

Die Fensterleiste ist die native `GtkHeaderBar` mit den Fensterknöpfen des
Systems und trägt Öffnen, Dateiname, Inhaltsverzeichnis, Suche und Menü. Das
Menü enthält Öffnen, Nachladen, den Darstellungsmodus als Schalter aus System,
Hell und Dunkel sowie die Textgröße; Inhaltsverzeichnis und Suche stehen als
eigene Knöpfe daneben und haben deshalb keine Menüzeile.
Der vollständige Pfad steht als Tooltip.

Jede geöffnete Datei bekommt einen eigenen Tab; eine bereits offene Datei wird
nach vorn geholt statt erneut gelesen. Die Tableiste erscheint erst ab dem
zweiten Dokument. Jeder Tab hält seine eigene Leseposition, seine eigene Suche
und seine eigene Dateiüberwachung.

| Aktion | Tastatur |
| --- | --- |
| Öffnen | `Ctrl+O` |
| Suche | `Ctrl+F` |
| Nächster / vorheriger Treffer | `Enter` / `Shift+Enter` im Suchfeld |
| Inhaltsverzeichnis | `Ctrl+Shift+O` |
| Text vergrößern / verkleinern / zurücksetzen | `Ctrl++` / `Ctrl+-` / `Ctrl+0` |
| Nachladen | `Ctrl+R` |
| Tab schließen | `Ctrl+W` |
| Nächster / vorheriger Tab | `Ctrl+Tab` / `Ctrl+Shift+Tab` |
| Menü | `F10` |
| Dokument auswählen / kopieren | `Ctrl+A` / `Ctrl+C` |
| Menü, Inhaltsverzeichnis oder Suche schließen | `Escape` |

Doppelklick wählt ein Wort, Dreifachklick einen Block, Shift-Klick erweitert die
Auswahl; Auswahl reicht über Blockgrenzen. Start ohne Dateiparameter bleibt
leer. Lesepositionen greifen beim erneuten Öffnen einer Datei.

## Inhaltsgrenzen

- Markdown: UTF-8 einschließlich BOM, höchstens 20 MiB. Dateien werden nie
  verändert; Aufgabenlisten bleiben schreibgeschützt.
- **Rohes HTML wird als Quelltext dargestellt, nicht interpretiert.** Der native
  Renderer hat keinen HTML-Parser und keine Bereinigung; damit entfällt die
  gesamte Klasse von Bereinigungsfehlern. Der Preis ist bewusst: HTML-lastige
  Dokumente sehen anders aus als auf GitHub. 580 der 652 CommonMark-Beispiele
  stimmen exakt mit der Spezifikation überein, die 72 mit rohem HTML weichen
  absichtlich ab.
- Lokale Bilder: höchstens 40 Megapixel, geprüft aus den Kopfdaten **vor** dem
  Dekodieren. Automatischer Zugriff bleibt im Dokumentverzeichnis einschließlich
  Unterordnern — nach Kanonisierung, damit ein Symlink nicht hinausführt.
- Remote-Bilder laden in v1 nicht; der Platzhalter nennt die Quelle.
- Links zu Markdown öffnen im selben Fenster, Fragmentlinks springen im
  Dokument. `https:`, `http:` und `mailto:` öffnen nach einem Klick in der
  Systemanwendung. Andere Schemata und Dateitypen werden abgewiesen; aus
  Dokumentinhalt entsteht kein Shell-Aufruf.
- Keine Telemetrie.

[Architektur](docs/architecture.md) ·
[Gestaltung](docs/design.md) ·
[Entwicklung](docs/development.md) ·
[Messwerte](docs/metrics.md) ·
[Einschränkungen](docs/limitations.md) ·
[Roadmap](docs/roadmap.md)

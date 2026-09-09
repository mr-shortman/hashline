# 009 — Nativer GTK4-Renderer statt WebView

Status: Entscheidung getroffen, Umsetzung offen. Stand: 8. September 2026.
Vorgänger: [008-parser-reference.md](008-parser-reference.md).

Diese Entscheidung **kehrt den Performancepfad des WebView-Stands um**. Jener
Pfad hatte den nativen Rewrite geprüft und abgelehnt und seine zweite Phase
ausdrücklich so geschnitten, dass die Ablehnung umkehrbar bleibt. Die Umkehr
tritt jetzt ein, weil eine neue Messung die tragende Annahme widerlegt. Die
Messreihen, auf die sich der Vergleich unten stützt, liegen im Tag
`webview-final`; im Baum stehen nur noch die nativen Reihen unter
`benchmarks/results/native-*`.

## 1. Die Annahme des WebView-Pfads und was sie widerlegt

007, Abschnitt 1 schließt aus fünf Stichproben:

> Jeder GUI-Stack auf dieser Maschine liegt zwischen 117 und 155 MiB, bevor er
> Inhalt anzeigt. […] Die 200-MiB-Grenze ist keine WebKit-Untergrenze, sondern
> eine Desktop-GUI-Untergrenze.

Die Stichprobe bestand aus `gnome-calculator`, `gnome-text-editor`, Ghostty,
WebKitGTK-`MiniBrowser` und Hashline. Sie enthielt keinen schlanken GTK-Viewer.
Am 8. September 2026 wurde ViewMD 0.1.3 — ein GTK-Markdown-Viewer auf `md4c`,
ohne Web-Technologie — mit demselben Verfahren gemessen:

| Fixture 100 KiB              | ViewMD | Hashline `phase2-final` |
| ---------------------------- | -----: | ----------------------: |
| `exec` → erster Buffer-Attach |  93 ms |                  705 ms |
| `exec` → erster Frame-Callback | 160 ms |                  712 ms |
| Protokollruhe nach Aufbau     | 336 ms |                1.238 ms |
| PSS der Prozessgruppe         | 41 MiB |                 193 MiB |

Zusätzlich ViewMD mit der 1-MiB-Fixture: **147 MiB PSS**.

**Der GUI-Boden liegt bei 41 MiB, nicht bei 117 MiB.** Die Zahlen aus 007 waren
die Grundlast konkreter GNOME-Anwendungen, nicht die eines GTK-Fensters mit
Textdarstellung. Damit fällt die Begründung, mit der 007 den Rewrite abgelehnt
hat: Speicher und Start sind eben nicht „überwiegend anwendungsseitig", sondern
der WebView-Anteil ist rund 150 MiB und rund 550 ms größer als nötig.

**Einschränkung der Messung.** n=2 je Wert, Stichprobe statt Reihe. Die geplante
Reihe mit n=12 wurde vom Nutzer abgebrochen, weil die Größenordnung bereits
entschieden war. Auf der Maschine lief während der Messung ein Video im Browser;
das verschlechtert beide Seiten, nicht eine. Die Werte sind belastbar für die
Größenordnung und für die Richtungsentscheidung, **nicht als Abnahme**. Die
Reihe mit n=30 ist in M0 nachzuholen; SPEC Abschnitt 9 führt sie als Auftrag.

Bestätigend, nicht entscheidend: Der eigene Messwert `native-main-to-first-frame`
von 1.145,7 ms aus `phase2-final` deckt sich mit den 1.238 ms Protokollruhe der
externen Messung. Beide Verfahren messen dasselbe.

## 2. Was 007 richtig gesehen hat

Zwei Feststellungen aus 007 bleiben gültig und tragen diese Entscheidung mit:

- **„Ein nativer Rust-Stack bleibt der einzige kohärente Endpunkt."** Genau das
  wird jetzt umgesetzt, nur mit einem anderen Toolkit als dort angenommen.
- **„Der Abstand rechtfertigt heute keine eigene Layout-Engine (Blockfluss,
  Tabellen, Selektion über Blockgrenzen, Bidi, Hit-Testing, A11y)."** Diese Liste
  ist korrekt und war das eigentliche Gegenargument. Sie beschreibt jedoch
  `winit + wgpu + parley + AccessKit`. Unter GTK4 liefert Pango Shaping, Bidi,
  Umbruch und Hit-Testing, und AT-SPI liefert die Barrierefreiheit. Die Liste
  ist damit keine Eigenleistung mehr. Übrig bleiben Blockplan, Virtualisierung
  und Typografie — und das sind genau die Teile, die Hashline ohnehin selbst
  bestimmen will.

Der abgelehnte Weg von 007 und der jetzt gewählte sind also nicht derselbe. 007
hat eine eigene Layout-Engine abgelehnt. Diese Entscheidung baut keine.

## 3. Entscheidung

**Hashline wird eine native GTK4-Anwendung in Rust.** Dokumentansicht ist ein
eigenes Widget, das den Op-Buffer mit Pango setzt und mit GSK zeichnet.
WebView, HTML, CSS, JavaScript und die gesamte Node-Toolchain entfallen aus
Produkt und Build. Details in [SPEC.md](../../SPEC.md), Abschnitte 4 und 5.

Toolkit-Wahl gegenüber `winit + wgpu + parley`: GTK4, weil Textauswahl über
Blockgrenzen, Hit-Testing, IME, AT-SPI, kinetisches Scrollen, fraktionale
Skalierung, Clipboard und Portal-Dateidialoge Anforderungen der SPEC sind und
im puren Rust-Stack jeweils Eigenbau wären. Der Preis ist die C-Bibliothek als
Abhängigkeit und eine schwächere Cross-Platform-Perspektive. Linux ist der
erklärte Fokus; der Preis wird bewusst gezahlt.

**Das Erscheinungsbild bleibt.** Der Wechsel betrifft den Renderer, nicht die
Gestaltung. Die bestehende Fassung ist die verbindliche Referenz; Farben,
Typografie, Lesespalte und Abstände werden unverändert übernommen. Einzige
beabsichtigte optische Änderung ist die Fensterleiste: die selbst gebaute
Titelleiste weicht der nativen `GtkHeaderBar` mit den Fensterknöpfen des Systems,
bei gleicher Anordnung der Bedienelemente. Damit ist die Migration visuell
prüfbar statt Geschmackssache — SPEC Abschnitt 12 verlangt Referenzaufnahmen der
WebView-Fassung, bevor sie entfernt wird.

**Phase 2 war die richtige Vorarbeit.** Der Op-Buffer aus 007 Abschnitt 4 ist
die Schnittstelle, an der der Renderer jetzt ausgetauscht wird. Ohne ihn wäre
diese Entscheidung ein Neuanfang; mit ihm ist sie ein Modultausch. Parser,
Suchindex, Slug-Regel, Abschnittsbildung und deren Tests werden vollständig
übernommen.

## 4. Was ungültig wird

Diese Entscheidung hebt den gesamten WebView-Stand auf. Die Entscheidungen
001 bis 007 beschrieben Tauri, WebKitGTK, `marked`, highlight.js, DOMPurify und
den abschnittsweisen DOM-Aufbau; keines dieser Teile existiert noch. Sie sind
aus dem Baum entfernt und im Tag `webview-final` erhalten. Was von ihnen
weitergilt, steht nicht mehr dort, sondern in `SPEC.md`:

- **Begrenztes, verzögertes Highlighting** — begrenzte Sprachauswahl, nur
  sichtbare Blöcke, keine automatische Spracherkennung. Der Ersatz für
  highlight.js ist `syntect` (SPEC Abschnitt 4 und 10).
- **Der Ressourcenschutz** — automatischer Bildzugriff bleibt an das
  Dokumentverzeichnis gebunden, entweichende Symlinks werden abgewiesen, Größen-
  und Pixelbudgets gelten unverändert (SPEC Abschnitt 7 und 11).
- **Die Grenzfälle aus dem Rendering-Gate** bleiben als Testmaterial gültig:
  lange Zeilen, tiefe Listen, breite Tabellen, große Codeblöcke.
- **Die eigene Titelleiste entfällt.** Unter GTK4 übernimmt `GtkHeaderBar`
  Fensterknöpfe, Ziehflächen, Doppelklick, Tastaturfokus und Barrierefreiheit;
  die Gründe für den Eigenbau sind damit weg (SPEC Abschnitt 3).
- **DOMPurify und die Inhaltsrichtlinie** entfallen mit dem HTML-Pfad. Rohes
  HTML wird als Quelltext dargestellt (SPEC Abschnitt 6). Das ist eine bewusste
  Funktionsminderung zugunsten eines erheblich kleineren Bedrohungsmodells.

Der Op-Buffer selbst ist keine Historie: er entstand im WebView-Stand, wird
vollständig übernommen und ist in [010](010-op-buffer-for-the-native-renderer.md)
und SPEC Abschnitt 6 festgehalten.

## 5. Was die Entscheidung riskiert

Ehrlich benannt, damit M0 sie prüft und nicht umgeht:

1. **Die Scrollhöhe bei geschätzten Blockhöhen.** Der Blockplan schätzt, das
   Layout korrigiert. Eine Korrektur oberhalb der Leseposition muss den
   Scrolloffset mitverschieben, sonst springt der Text. Das ist der häufigste
   Fehler virtualisierter Listen und in SPEC Abschnitt 5 als Abnahmefehler
   festgeschrieben.
2. **Auswahl über Blockgrenzen.** Im DOM war sie geschenkt, hier ist sie
   Eigenleistung über `(Blockindex, Byteoffset)`. M0 muss sie zeigen, nicht
   planen.
3. **Tabellen.** Pango setzt Text, keine Tabellen. Spaltenbreiten, Umbruch in
   Zellen und horizontaler Eigenscroll sind selbst zu bauen.
4. **Der Aufwand ist real.** Ein MVP mit Blocklayout, Textsatz, Scrollen und
   Suche ist bei vorhandenem Parser als mehrwöchige Arbeit anzusetzen, nicht als
   Wochenende.

Verfehlt einer der ersten drei Punkte in M0 sein Budget, wird vor M1 entschieden
— nach derselben Regel, die 007 aufgestellt hat und die hier gerade angewendet
wurde.

## 6. Was die Entscheidung gewinnt

Die Zusage, die den Aufwand rechtfertigt, steht in SPEC Abschnitt 9 als eigene
Budgetzeile: **PSS bei 10 MiB höchstens PSS bei 100 KiB plus das Vierfache der
Dateigröße.** Öffnungszeit und Speicher wachsen nicht mit der Dokumentgröße,
weil nur der Sichtbereich gesetzt wird.

Das hält heute keiner der gemessenen Viewer ein — ViewMD springt zwischen
100 KiB und 1 MiB von 41 auf 147 MiB, der WebView-Stand baut ein DOM über das
ganze Dokument. Es ist damit die erste Eigenschaft, mit der Hashline nicht
„auch ein Markdown-Viewer" ist.

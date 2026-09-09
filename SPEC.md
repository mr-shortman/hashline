# Hashline — Umsetzungsspezifikation

Stand: 8. September 2026. Status: Grundlage für die Implementierung von v1 auf nativem Stack.

Diese Fassung ersetzt die WebView-Spezifikation. Die Begründung des Wechsels, die zugrunde liegenden Messungen und die Rücknahme der gegenteiligen Entscheidung stehen in [009-native-renderer.md](docs/decisions/009-native-renderer.md).

## 1. Produktziel

Hashline ist ein schneller, ruhiger Markdown-Viewer für Linux. Eine Datei soll sich unmittelbar öffnen lassen, angenehm lesbar sein und auch bei umfangreichen Dokumenten flüssig bedienbar bleiben. Die Gestaltung ist modern, reduziert und durch gute Typografie geprägt.

Die Anwendung ist ein natives GTK4-Programm in Rust. Fenster, Dokumentansicht und Bedienelemente sind GTK-Widgets; der Dokumentbereich ist ein eigenes Widget, das den Op-Buffer des Parsers direkt mit Pango setzt und mit GSK zeichnet. Es gibt keine WebView, kein HTML, kein CSS und keine JavaScript-Laufzeit im ausgelieferten Programm.

Der Anspruch ist ausdrücklich beides zugleich. Auf Linux existieren schnelle Viewer mit schlechtem Textsatz und schön gesetzte Viewer auf WebView-Basis mit schlechter Laufzeit. Hashline ist nur dann eine Berechtigung, wenn es beide Seiten erfüllt; eine gewonnene Millisekunde rechtfertigt keine schlechtere Lesbarkeit, und eine gestalterische Idee rechtfertigt kein ruckelndes Scrollen.

Prioritäten bei Zielkonflikten:

1. Reaktionsfähigkeit und verlässliches Lesen, Auswählen und Navigieren.
2. Darstellungsqualität und eine übersichtliche Oberfläche.
3. Verständliche, wartbare Architektur.
4. Zusätzliche Markdown-Erweiterungen und Komfortfunktionen.

Performance-Zahlen in dieser Spezifikation sind Entwicklungs- und Abnahmeziele, keine bereits nachgewiesenen Eigenschaften. Der erste Meilenstein überprüft die technische Grundlage.

### Name und Bezeichnungen

Die Anwendung heißt **Hashline**. Der Eigenname trägt die Wiedererkennung, die generische Bezeichnung die Verständlichkeit im Anwendungsmenü und im Dateimanager. Beide werden gemeinsam angezeigt, nicht gegeneinander ausgetauscht.

| Ebene | Wert |
| --- | --- |
| Produktname | Hashline |
| Generische Bezeichnung | Markdown Viewer |
| Kurzbeschreibung | A fast, quiet Markdown viewer for Linux |
| Ausführbare Datei und CLI | `hashline` |
| Paketname | `hashline` |
| Anwendungs-ID | `de.kalendium.Hashline` |

Die Anwendungs-ID folgt der umgekehrten Domain des Herausgebers. Sie muss über Desktop-Eintrag, `GApplication`-ID, Fenster-Klasse (`app_id`) und Paketmetadaten identisch verwendet werden.

## 2. Umfang von v1

### Enthalten

- Eine aktive Datei in einem Anwendungsfenster.
- Öffnen über Dateidialog, Drag-and-drop, Dateimanager und `hashline <datei>`.
- Bei erneutem Aufruf die Datei an die laufende Instanz übergeben und das Fenster aktivieren.
- CommonMark-Grundsyntax und die vereinbarten GFM-Erweiterungen aus Abschnitt 6.
- Lokale Bilder, Dokumentanker, relative Markdown-Links und externe Links.
- Einklappbares Inhaltsverzeichnis mit Kennzeichnung des aktuellen Abschnitts.
- Dokumentweite Textsuche mit Trefferzahl sowie nächstem/vorherigem Treffer.
- Textauswahl und Kopieren über Blockgrenzen hinweg; Codeblock kopieren.
- Automatisches Nachladen bei Dateiänderungen unter Erhalt der Leseposition.
- System-, Hell- und Dunkelmodus; Textzoom.
- Speicherung von Darstellungspräferenzen, Fensterzustand und zuletzt verwendeten Lesepositionen.
- Installation mit Desktop-Eintrag, Icon und Markdown-Dateizuordnung.

### Spätere Erweiterungen

Editor, Tabs, Projekt-/Dateibaum, Workspace-Verwaltung, Cloud-Sync, Konten, Plugins, PDF-Export, Mermaid, mathematische Formeln und ausführbare Inhalte gehören nicht zu v1. Beliebige HTML-Kompatibilität ist kein Ziel und mit dem nativen Renderer ausdrücklich kein Versprechen mehr.

Die App verändert keine geöffneten Markdown-Dateien. Aufgabenlisten bleiben schreibgeschützt.

## 3. Oberfläche und Interaktion

### Fensteraufbau

- Ein `GtkApplicationWindow` mit `GtkHeaderBar` als Titelleiste. Die frühere selbst gebaute Titelleiste entfällt: unter GTK4 liefert die HeaderBar Fensterknöpfe, Ziehflächen, Doppelklickverhalten, Tastaturfokus und Barrierefreiheit ohne Eigenbau. [006-custom-titlebar.md](docs/decisions/006-custom-titlebar.md) ist damit gegenstandslos und wird in `009` als überholt vermerkt. Dies ist die einzige beabsichtigte optische Änderung gegenüber der bestehenden Fassung.
- Die HeaderBar vereint dieselben Bedienelemente wie bisher in derselben Anordnung: Datei öffnen, Dateiname als Titel, Inhaltsverzeichnis, Suche und Menü. Fensterknöpfe und Fokuszustand kommen vom System statt aus eigenem Code.
- Der vollständige Pfad steht als Untertitel oder Tooltip zur Verfügung, ist aber kein dauerhaftes Gestaltungselement.
- Der Dokumentbereich nimmt den verbleibenden Platz ein. Keine permanente Statusleiste.
- Inhaltsverzeichnis initial geschlossen, umgesetzt als `AdwOverlaySplitView`, falls libadwaita nach M0 aufgenommen wird, sonst als eigenes Overlay über `GtkOverlay`. Die gewählte Sichtbarkeit wird gespeichert.
- Ab etwa 900 logischen Pixeln Fensterbreite öffnet es als Seitenleiste; darunter als Overlay ohne Verengung der Lesespalte.
- Ohne Datei: ruhige Leeransicht mit „Markdown-Datei öffnen“ und Drag-and-drop-Hinweis.
- Mehrere gleichzeitig übergebene Dateien: erste Datei öffnen und kurz auf die Beschränkung auf eine Datei hinweisen.

### Gestaltung und Typografie

**Die native Fassung sieht aus wie die bestehende WebView-Fassung.** Das Erscheinungsbild ist keine offene Frage dieses Projekts und wird nicht neu entworfen. Die aktuelle Umsetzung ist die verbindliche Gestaltungsreferenz; der Stackwechsel ist eine Portierung des Aussehens auf einen anderen Renderer, keine Gelegenheit für eine Überarbeitung.

Die **einzige beabsichtigte optische Abweichung** ist die Fensterleiste: Die selbst gebaute Titelleiste entfällt, an ihre Stelle tritt die native `GtkHeaderBar` mit den Fensterknöpfen des Systems. Sie trägt dieselben Bedienelemente in derselben Anordnung. Jede weitere sichtbare Abweichung ist ein Fehler und wird behoben, nicht nachträglich zur Absicht erklärt.

Referenz sind `src/app/styles.css` und `src/ui/titlebar.css` im Stand vor der Migration. Beide werden nach Abschnitt 13 als Gestaltungsreferenz erhalten, auch nachdem `src/` entfernt ist.

#### Design-Tokens

Die Werte werden unverändert in die Rust-Token-Struktur übernommen, die Hell- und Dunkelvariante aus derselben Definition ableitet. Kein Wert wird an der Verwendungsstelle erfunden.

| Token | Hell | Dunkel |
| --- | --- | --- |
| `bg` | `#fbfaf8` | `#1c201e` |
| `surface` | `#ffffff` | `#222724` |
| `subtle` | `#f1f0ed` | `#2a302c` |
| `text` | `#242826` | `#e4e7e2` |
| `muted` | `#636b65` | `#a5aea7` |
| `border` | `#dedfd9` | `#3b433d` |
| `accent` | `#37684d` | `#9ac6a6` |
| `accent-soft` | `#e6eee7` | `#2d4133` |
| `code` | `#f0f1ee` | `#242b26` |
| `error` | `#963b30` | `#f0a89b` |
| `syntax-keyword` | `#88458f` | `#d8a2de` |
| `syntax-string` | `#326943` | `#abd2a3` |
| `syntax-number` | `#895519` | `#e7bf85` |

Radius 7 px, Grundabstand 8 px, Bewegungsdauer 120 ms. Schrift ist die Systemschrift aus `gtk-font-name`, Code der fontconfig-Alias `monospace`. Keine mitgelieferten oder nachgeladenen Schriften. Bedienelemente folgen der bisherigen Größenstaffel mit 14 px als Wurzelgröße.

#### Dokumentsatz

- **Lesespalte.** Der Textkörper steht in einer im Viewport zentrierten Spalte von 76 Zeichen, gemessen an der tatsächlichen Zeichenbreite der Fließtextschrift über `pango::FontMetrics::approximate_char_width` statt an einer festen Pixelzahl. Innenabstand 42 px oben, 32 px seitlich, 100 px unten. Ein am linken Rand klebender Textblock ist ein Fehler, kein Standardverhalten.
- Unterhalb einer Fensterbreite, bei der die Spalte nicht mehr passt, nutzt der Text die verfügbare Breite abzüglich der seitlichen Abstände.
- **Fließtext** 17 logische Pixel bei 100 Prozent Zoom, Zeilenhöhe 1,65. Der Zoom skaliert diese Grundgröße; alle em-basierten Werte folgen mit.
- **Überschriften** in der bestehenden Skala: H1 2,1 em, H2 1,5 em, H3 1,2 em, H4 1,05 em, H5 und H6 1 em. Schriftstärke 650, Laufweite −0,025 em, Zeilenhöhe 1,3. Abstand 1,8 em davor und 0,7 em danach — der größere Abstand steht vor der Überschrift, damit sie zum folgenden Text gehört. H2 trägt eine untere Trennlinie in `border` mit 0,35 em Abstand.
- **Blöcke** — Absätze, Listen, Zitate, Definitionslisten — mit 1,15 em Abstand nach unten; Listeneinträge 0,25 em, Absätze in Listeneinträgen 0,4 em. Trennlinien 2 em. Der erste Block eines Dokuments hat keinen oberen Abstand.
- **Codeblöcke** in Monospace auf `code`-Fläche mit horizontalem Eigenscroll. Inline-Code 0,85 em. Das Dokument selbst scrollt nie horizontal.
- **Tabellen** bei 0,92 em, mit horizontalem Eigenscroll innerhalb ihres Blocks und sichtbarer Kopfzeile.
- **Bilder** passen sich der Lesespalte an und überschreiten sie nicht. Fehlende oder abgelehnte Bilder erhalten einen unaufdringlichen Platzhalter mit Alternativtext in der Höhe, die das Layout ohnehin reserviert hat.
- Hover- und Fokusübergänge respektieren `gtk-enable-animations`.
- Beim Start und bei Themenwechseln kein Aufblitzen des falschen Farbschemas: das Theme steht fest, bevor das Fenster zum ersten Mal Inhalt zeigt.

Wo Pango und CSS sich unvermeidlich unterscheiden — Zeilenabstandsberechnung, Umbruchentscheidungen, Unterschneidung — gilt der optische Eindruck der Referenz, nicht die rechnerische Gleichheit der Werte. Solche Abweichungen werden in `docs/decisions/` festgehalten.

### Bedienung

| Aktion | Tastatur |
| --- | --- |
| Datei öffnen | `Ctrl+O` |
| Suche öffnen | `Ctrl+F` |
| Nächster / vorheriger Treffer | `Enter` / `Shift+Enter` im Suchfeld |
| Suche oder oberstes Overlay schließen | `Escape` |
| Inhaltsverzeichnis umschalten | `Ctrl+Shift+O` |
| Text vergrößern / verkleinern / zurücksetzen | `Ctrl++` / `Ctrl+-` / `Ctrl+0` |
| Manuell nachladen | `Ctrl+R` |
| Kopieren / alles im Dokument auswählen | `Ctrl+C` / `Ctrl+A`, außerhalb von Eingabefeldern |

Aktionen werden als `GAction` registriert und über `GtkApplication::set_accels_for_action` belegt, damit Menü, Tastatur und Barrierefreiheit dieselbe Quelle haben.

Textzoom: 80–200 Prozent in 10-Prozent-Schritten, nur für das Dokument. Die Lesespalte bleibt in Zeichen definiert und läuft daher beim Zoomen neu um. Scrollen folgt dem Plattformverhalten über `GtkScrolledWindow` einschließlich kinetischem Scrollen; kein eigener Scroll-Animator. Abschnittssprünge dürfen kurze Bewegung verwenden, bei abgeschalteten Animationen sofort springen.

Barrierefreiheit ist Pflicht und läuft über AT-SPI: Das Dokumentwidget meldet die `AccessibleRole::Document`-Rolle, liefert seinen Text über die Text-Schnittstelle und meldet Auswahl- und Cursorbewegungen. Bedienelemente haben zugängliche Namen und sichtbaren Tastaturfokus. Normaler Text erfüllt mindestens 4,5:1 Kontrast. Suche und Overlay geben den Fokus beim Schließen sinnvoll zurück. Inhalte bleiben bei 200 Prozent Textzoom nutzbar.

## 4. Technische Entscheidungen

| Aufgabe | Entscheidung |
| --- | --- |
| Sprache und Laufzeit | Rust, ein einziger nativer Prozess |
| Toolkit | GTK4 über `gtk4-rs`, `libadwaita` nur falls M0 einen konkreten Nutzen zeigt |
| Textsatz | Pango: Shaping, Bidi, Font-Fallback, Zeilenumbruch, Hit-Testing |
| Zeichnen | GSK-Render-Nodes über `gtk::Snapshot`, pro sichtbarem Block |
| Dokumentansicht | Eigenes Widget mit blockweiser Virtualisierung, kein `GtkTextView` |
| Markdown-Parser | `hashline-markdown` (`pulldown-cmark`), unverändert weiterverwendet |
| Syntaxhervorhebung | `syntect` mit begrenzter Sprachauswahl, außerhalb des Zeichenpfads |
| Bilddekodierung | In M0 zu entscheiden: `glycin` (sandboxed) gegenüber `gdk-pixbuf`; Begründung in `docs/decisions/` |
| Dateibeobachtung | `notify` mit Bündelung |
| Einstellungen | `GSettings` unter der Anwendungs-ID |
| Instanzübergabe | `GApplication` mit `HANDLES_OPEN`, ohne eigenes Single-Instance-Plugin |
| Dateidialoge | `GtkFileDialog`, über das Portal wenn verfügbar |
| Paketverwaltung | Cargo mit eingechecktem `Cargo.lock` |

Node.js, npm, Vite, React, TypeScript, Playwright, DOMPurify und WebAssembly entfallen ersatzlos aus dem Produkt und aus dem Build. Die Entwicklungsumgebung braucht Rust und die GTK4-Entwicklungspakete, sonst nichts.

`hashline-markdown` bleibt frei von Toolkit- und Plattformabhängigkeiten. Markdown-Regeln werden an genau einer Stelle implementiert.

## 5. Architektur und Verantwortlichkeiten

```text
GtkApplication
    │ Aktionen / Fensterzustand
    ▼
Window (HeaderBar, Outline, Suchleiste)
    │
    ▼
DocumentController
    ├── DocumentSource ──── Datei lesen, notify-Watcher, Revisionen
    ├── hashline-markdown ─ Markdown → Op-Buffer (eigener Thread)
    ├── Highlighter ─────── syntect, sichtbare Codeblöcke, eigener Thread
    └── Document (unveränderlich: Ops, Text, Abschnitte, Überschriften)
                              │
                              ▼
                       DocumentView (GTK-Widget)
                       ├── BlockPlan      Blockliste, Höhen, Scrollhöhe
                       ├── LayoutCache    Pango-Layouts sichtbarer Blöcke
                       ├── Selection      Position = (Blockindex, Byteoffset)
                       └── snapshot()     GSK-Nodes, nur Sichtbereich
```

### Modulgrenzen

- **`app`** — `GApplication`, Fenster, Aktionen, Tastenbelegung, CLI-Argumente, Instanzübergabe. Keine Layout- oder Parserlogik.
- **`document`** — Öffnen, Nachladen, Ladezustände, Revisionsverwaltung, Fehler, Leseanker. Koordiniert, implementiert weder Parser noch Layout.
- **`markdown`** — die bestehende Crate. Kennt weder GTK noch Dateisystem.
- **`layout`** — Op-Buffer zu Blockplan, Pango-Layouts, Höhenmessung, Zeichenanweisungen. Kennt keine Dateien und keinen Anwendungszustand.
- **`view`** — das Widget: Scrollen, Auswahl, Hit-Testing, Suchmarkierung, Ankersprünge, Zeichnen. Einziger Eigentümer des Layout-Caches.
- **`search`**, **`outline`**, **`preferences`**, **`theme`** — je ein kleines Modul mit klarer Aufgabe.

Kein allgemeines Plugin-System, keine Dependency-Injection, kein globaler Event-Bus, keine Datenbank.

### Das zentrale Layoutproblem

Ein virtualisierter Dokumentviewer muss eine Gesamthöhe kennen, bevor er alle Blöcke gesetzt hat. Diese Spannung ist der Kern des Projekts und wird ausdrücklich geregelt statt umgangen:

- Der **Blockplan** entsteht direkt aus dem Op-Buffer und enthält für jeden Block Art, Textbereich und eine **geschätzte** Höhe aus Zeichenzahl, verfügbarer Breite und Schriftmetrik. Er wird für 10 MiB in einem Durchgang gebaut und ist die einzige Struktur, die über das gesamte Dokument existiert.
- **Gesetzt** werden nur Blöcke im Sichtbereich zuzüglich eines Puffers von etwa einer Bildschirmhöhe in beide Richtungen. Deren gemessene Höhe ersetzt die Schätzung im Plan.
- Eine Korrektur **oberhalb** der aktuellen Leseposition verschiebt den Scrolloffset um denselben Betrag mit, sodass der sichtbare Text stillsteht. Springende Inhalte beim Scrollen sind ein Abnahmefehler, keine Toleranz.
- Der Layout-Cache ist nach Blockindex adressiert und in der Größe begrenzt; verworfene Blöcke behalten ihre gemessene Höhe im Plan.
- Ein Wechsel von Fensterbreite, Zoom oder Schrift verwirft gemessene Höhen und setzt die Schätzung neu auf; die Leseposition wird über den Leseanker aus Abschnitt 7 gehalten.

### Zustandsmodell

Der Dokumentzustand unterscheidet `empty`, `loading`, `ready` und `error`. Beim Nachladen bleibt das vorhandene Dokument sichtbar und erhält einen separaten Aktualisierungsstatus.

- Jeder Öffnungs-/Ladevorgang erhält eine steigende `RequestId`.
- Nur die aktuelle Anfrage darf das aktive Dokument ersetzen.
- Veraltete Lese-, Parse-, Bild- und Highlight-Ergebnisse werden verworfen.
- Ein neuer Auftrag ersetzt wartende Aufträge; laufende Arbeit wird über ein Abbruch-Flag beendet, das die Arbeitsschleife regelmäßig prüft.
- Watcher, Threads, Kanäle und Bild-Handles besitzen einen klaren Lebenszyklus und werden beim Wechsel freigegeben.
- Das Dokument ist nach dem Parsen unveränderlich und wird als `Arc` geteilt. Kein Thread schreibt in ein Dokument, das ein anderer liest.
- Eine Eingabe im Suchfeld parst weder neu noch verwirft sie den Layout-Cache.

### Verzeichnisstruktur

```text
crates/
  markdown/           # bisher src-tauri/markdown, unverändert übernommen
  hashline/
    src/
      app/            # GApplication, Fenster, Aktionen, CLI
      document/       # Controller, Quelle, Watcher, Revisionen, Anker
      layout/         # Blockplan, Pango-Layouts, Höhen, Zeichenanweisungen
      view/           # Dokumentwidget, Scroll, Auswahl, Hit-Test
      search/         # Suche auf dem Textblob, Trefferabbildung
      outline/        # Inhaltsverzeichnis
      preferences/    # GSettings, Lesepositionen
      theme/          # Design-Tokens, Hell/Dunkel
data/                 # Desktop-Eintrag, Icon, GSettings-Schema, MIME
tests/fixtures/       # Markdown-, Bild- und Fehlerfälle
benchmarks/           # Generatoren, Messabläufe und Ergebnisse
docs/                 # Architekturentscheidungen und Betriebsanleitung
```

Dateien werden angelegt, wenn die entsprechende Funktion implementiert wird; keine leeren Strukturen vorbereiten.

## 6. Markdown- und Rendering-Vertrag

### Unterstützte Inhalte

Absätze, Überschriften H1–H6, Hervorhebungen, durchgestrichener Text, Inline-Code, eingerückte und eingezäunte Codeblöcke, Zitate, geordnete und ungeordnete verschachtelte Listen, Trennlinien, Links, Bilder, GFM-Tabellen, Autolinks, Fußnoten und schreibgeschützte Aufgabenlisten.

- Softbreaks und Hardbreaks folgen der Parser-Konfiguration, die mit Fixtures festgehalten wird.
- Überschriften erhalten deterministische, eindeutige IDs; doppelte Überschriften bekommen Suffixe. Umlaute, Unicode und leere Überschriften werden getestet.
- Inhaltsverzeichnis und Dokument verwenden dieselben IDs aus demselben Parse-Vorgang.
- Die Slug-Regel bleibt die dokumentierte bestehende; vollständige GitHub-Ankerkompatibilität wird nicht vorausgesetzt.
- Frontmatter wird als Quellinhalt nach Parser-Verhalten behandelt; es steuert keine Anwendungsfunktionen.
- Unbekannte Code-Sprachen erhalten lesbaren, unkolorierten Code.

### Rohes HTML im Dokument

Der native Renderer hat keinen HTML-Parser und keine Bereinigung. Das ist eine Vereinfachung des Sicherheitsmodells und wird als solche festgeschrieben:

- Rohe HTML-Blöcke und rohe Inline-HTML-Fragmente werden in v1 **als Quelltext** dargestellt, monospace und dezent abgesetzt, mit einem einmaligen ruhigen Hinweis im Dokument.
- Der bisherige Fallback-Pfad des Op-Buffers — Abschnitte mit rohem HTML behalten eine HTML-Repräsentation für DOMPurify — entfällt. `SECTION_FALLBACK` und die HTML-Erzeugung werden aus `hashline-markdown` entfernt.
- Damit verschwindet DOMPurify vollständig, und mit ihm die gesamte Klasse von Bereinigungsfehlern. Der Preis ist bewusst: HTML-lastige Dokumente sehen in Hashline anders aus als auf GitHub.
- Eine spätere Abbildung einer kleinen HTML-Teilmenge auf Operationen bleibt möglich und braucht eine eigene Entscheidung.

### Änderungen am Op-Buffer

Der Op-Buffer bleibt das Format zwischen Parser und Renderer und wird für einen Rust-Konsumenten angepasst:

1. **Offsets und Längen werden UTF-8-Byteoffsets.** Die UTF-16-Zählung existierte nur, damit JavaScript `substring` ohne Übersetzungstabelle verwenden konnte. Kein Konsument braucht sie mehr.
2. **Pro Block ein Textbereich.** Bisher tragen nur Abschnitte `textStart`/`textLen`. Suche und Auswahl brauchen die Zuordnung je Block; der Encoder schreibt sie mit.
3. **`strings` und `text` bleiben** als zwei Blobs erhalten. Sie sind bereits genau das, was ein Pango-Layout und die Suche brauchen.
4. Die Tag- und Attribut-IDs bleiben unverändert. Ihre Namen sind HTML-nah, ihre Bedeutung ist es nicht mehr; sie bezeichnen Block- und Inline-Arten.

Diese Änderungen sind rein additiv beziehungsweise löschend und werden von den bestehenden Parser-Tests abgedeckt.

### Datenfluss

1. Öffnungsauftrag normalisieren; Dateipfad und Zugriff prüfen.
2. Datei asynchron lesen, UTF-8 inklusive BOM unterstützen; ungültige Kodierung verständlich melden.
3. Markdown auf einem Arbeitsthread zum Op-Buffer parsen; Überschriften und Blockbereiche entstehen dabei.
4. Dokument als unveränderliche Revision an den Controller geben.
5. Blockplan mit geschätzten Höhen bauen und die Ansicht auf Leseanker oder Dokumentanfang setzen.
6. Sichtbare Blöcke setzen, messen, zeichnen. Erste lesbare Darstellung messen.
7. Bilder, Syntaxhervorhebung und Suchtreffer nach Priorität ergänzen; bei jedem Ergebnis die Revision prüfen.

Schritt 6 ist der einzige Pfad, der Pango-Layouts erzeugt. Kein anderes Modul setzt Text.

## 7. Dateien, Ressourcen und Aktualisierung

- Relative Pfade beziehen sich auf das Verzeichnis der geöffneten Datei, nicht auf das Prozess-Arbeitsverzeichnis.
- CLI-Pfade werden relativ zum Arbeitsverzeichnis des jeweiligen Aufrufers aufgelöst, auch bei Übergabe an die laufende Instanz.
- Dateinamen mit Leerzeichen, Unicode und URL-Escapes sowie atomisches Speichern über Umbenennung werden unterstützt.
- Ein Klick auf einen Markdown-Link öffnet das Ziel im selben Fenster; reine Fragmentlinks springen im aktuellen Dokument.
- Externe `https:`, `http:` und `mailto:`-Links öffnen nach Benutzerklick über `gtk::UriLauncher` in der Systemanwendung.
- Sonstige Dateitypen und unbekannte URL-Schemata erhalten einen verständlichen Hinweis; kein Shell-Aufruf aus Dokumentinhalt.
- Lokale Bilder werden aus dem Dateisystem geladen, dekodiert und als GDK-Textur gehalten. Es gibt keine Ressourcen-URLs und keine IPC-Kopie mehr.
- Automatischer Bildzugriff gilt für das Dokumentverzeichnis und dessen Unterverzeichnisse. Pfade außerhalb, einschließlich entweichender Symlinks, werden nicht automatisch geladen. Ein expliziter Öffnungsvorgang darf einen neuen Dokumentbereich wählen.
- Remote-Bilder laden in v1 nicht. Der Platzhalter nennt die Quelle. Eine spätere Freigabe braucht eine eigene Entscheidung.
- Dekodierte Bilder unterliegen einem Pixel- und Speicherbudget, das vor der Dekodierung anhand der Bildkopfdaten geprüft wird. Überschreitungen ergeben einen Platzhalter mit Begründung, keinen Dekodierversuch.

Dateiänderungen werden ereignisbasiert beobachtet und ungefähr 150 ms gebündelt. Ein Inhaltsvergleich verhindert unnötiges Neurendern. Auch das Ersetzen der Datei muss die Beobachtung überleben; dafür bei Bedarf das Elternverzeichnis mit Filter auf die aktive Datei beobachten. Watcher-Fehler lassen manuelles Nachladen verfügbar.

Beim Nachladen wird die nächste geeignete Überschrift beziehungsweise ein stabiler Block mit relativem Viewport-Abstand als Leseanker verwendet. Falls dieser entfällt, dienen vorheriger Block und schließlich begrenzter Scrollfortschritt als Fallback. Bei späterem Bildlayout wird der Anker nur korrigiert, solange keine neue Benutzernavigation stattgefunden hat. Das Dokument springt nicht bei jedem Speichern nach oben.

Bei Lesefehlern bleibt ein bereits angezeigtes Dokument sichtbar, mit knapper Meldung und Wiederholen-Aktion. Beim Öffnen einer anderen Datei wird der angezeigte Dateiname erst nach erfolgreicher Übernahme gewechselt; die fehlgeschlagene Zieldatei wird in der Meldung genannt.

Persistenz ist auf Präferenzen und höchstens 100 zuletzt verwendete Pfade mit Leseposition begrenzt. Kein dauerhafter Dokumentinhalt-Cache in v1. Start ohne Dateiparameter zeigt die Leeransicht; gespeicherte Positionen gelten beim erneuten Öffnen. Fehlende oder beschädigte Einstellungen blockieren den Start nicht.

## 8. Suche, Auswahl und Inhaltsverzeichnis

- Die Suche läuft auf dem Textblob des Op-Buffers, nicht auf gesetzten Layouts. Sie ist damit unabhängig davon, wie viel des Dokuments bereits gesetzt wurde.
- v1 bietet eine wörtliche, nicht zwischen Groß-/Kleinschreibung unterscheidende Suche über Unicode-Case-Folding. Keine Regex- oder Fuzzy-Suche.
- Treffer werden über die Blocktextbereiche aus Abschnitt 6 auf Block und Byteoffset abgebildet und beim Zeichnen als Pango-Attribute des betroffenen Blocks markiert. Eine Suche verwirft keine Layouts und ändert kein Dokument.
- Eine Suchanfrage wird kurz gebündelt; alte Ergebnisse dürfen neuere nicht überschreiben.
- Der Sprung zu einem Treffer setzt die Ansicht über den Blockplan, auch wenn der Zielblock noch nie gesetzt wurde.
- Eine Auswahlposition ist `(Blockindex, Byteoffset)` in Dokumentreihenfolge. Auswahl über Blockgrenzen ist damit eine Ordnung auf diesen Paaren, kein Sonderfall.
- Kopieren erzeugt reinen Text in Dokumentreihenfolge, mit Blocktrennung. Kopieren eines Codeblocks übernimmt den ursprünglichen Codetext ohne Bedienbeschriftungen.
- Die aktive Überschrift folgt aus dem Scrolloffset und dem Blockplan, ohne Messung während des Scrollens.

## 9. Performance-Budgets und Messverfahren

### Referenz und Testdaten

Die Referenzmaschine mit SSD und integrierter Grafik sowie Distribution, CPU, RAM, Displayfrequenz, Skalierung und Sitzungstyp werden in M0 dokumentiert. Die Referenz läuft mit 60 Hz; eine zusätzliche Prüfung bei 120 Hz erfolgt, falls Hardware verfügbar ist.

Messungen verwenden einen Release-Build. Die Messwerkzeuge sind toolkit-neutral und überleben den Stackwechsel: Startzeiten über das Wayland-Protokoll des Clients (`exec` → erster Buffer-Attach → erster Frame-Callback), sichtbare Textdarstellung über eine Monitoraufnahme via Mutter-ScreenCast, Speicher über `/proc/<pid>/smaps_rollup` (PSS, rekursive Prozessgruppe), Frame-Präsentation über die Sysprof-Marken des Compositors.

| Fixture | Inhalt |
| --- | --- |
| Klein | Etwa 100 KiB UTF-8-Markdown mit typischer README-Struktur |
| Mittel | Etwa 1 MiB mit vielen Überschriften, Listen, Tabellen und Codeblöcken |
| Groß | Etwa 10 MiB gemischter Text als Belastungstest |
| Sonderfälle | Extrem lange Zeile, tiefe Listen, große Tabelle, viele kleine Blöcke, viele/große Bilder |

Fixtures sind deterministisch und bleiben unverändert, damit Reihen über den Stackwechsel hinweg vergleichbar bleiben.

### Vergleichsbasis

Die Budgets sind an gemessenen Alternativen ausgerichtet, nicht frei gewählt. Stichprobe vom 8. September 2026 auf der Entwicklungsmaschine, **n=2** je Wert; die geplante Reihe mit n=12 wurde abgebrochen und ist nachzuholen.

| Fixture 100 KiB | ViewMD (GTK, nativ) | Hashline `phase2-final` (WebView) |
| --- | ---: | ---: |
| Erster Frame auf dem Schirm | 160 ms | 712 ms |
| Protokollruhe nach Aufbau | 336 ms | 1.238 ms |
| PSS | 41 MiB | 193 MiB |
| PSS bei 1 MiB | 147 MiB | – |

ViewMD ist damit die Latte für Start und Speicher und zugleich das Gegenbeispiel für die Darstellung. Der Sprung von 41 auf 147 MiB zwischen 100 KiB und 1 MiB zeigt, was ein nicht virtualisierter Viewer kostet — das ist die Kennzahl, die Hashline schlagen muss.

### Ziele

| Metrik | Ziel auf der Referenzmaschine |
| --- | --- |
| Prozessstart bis erste lesbare Darstellung, kleine Datei | p95 ≤ 250 ms; ambitioniertes Optimierungsziel ≤ 150 ms |
| Öffnen in laufender Instanz, kleine Datei | p95 ≤ 50 ms |
| Öffnen in laufender Instanz, mittlere Datei | p95 ≤ 120 ms |
| Öffnen in laufender Instanz, große Datei | Lesbar innerhalb 400 ms; Oberfläche durchgehend bedienbar |
| Öffnen von Suche/Menü, normale UI-Reaktion | p95 ≤ 30 ms |
| Suchergebnisse, mittlere Datei | p95 ≤ 100 ms ab letzter Eingabe, inklusive Bündelung |
| Suchergebnisse, große Datei | p95 ≤ 150 ms ab letzter Eingabe |
| Scrollen bei 60 und 120 Hz, alle Fixtures | Mindestens 99 % der Frames innerhalb des Refresh-Budgets über 10 s; kein Stillstand > 50 ms |
| Hauptthread-Arbeit während normaler Interaktion | Keine anwendungsbedingten zusammenhängenden Aufgaben > 16 ms |
| Ruhender Speicher, kleine Datei | Gesamte App-Prozessgruppe ≤ 80 MiB PSS |
| **Speicherzuwachs über Dokumentgröße** | PSS bei 10 MiB höchstens PSS bei 100 KiB **plus das Vierfache der Dateigröße** |
| Leerlauf nach Abschluss aller Arbeiten | Im Mittel < 0,5 % eines CPU-Kerns über 30 s |
| Installierte Größe | ≤ 20 MiB ohne WebView-Abhängigkeit |

Die Zeile zum Speicherzuwachs über die Dokumentgröße ist das eigentliche Alleinstellungsmerkmal und wird bei jeder Messreihe ausgewiesen. Sie ist die Zusage, die weder ViewMD noch der bisherige WebView-Stand einhält: Öffnungszeit und Speicher wachsen nicht mit der Dokumentgröße, weil nur der Sichtbereich gesetzt wird.

Prozessstart bedeutet: kein App-Prozess aktiv, Betriebssystem-Dateicache unkontrolliert. Ein echter Start mit kaltem Dateicache wird separat ausgewiesen. „Erste lesbare Darstellung“ meint tatsächlichen sichtbaren Dokumenttext nach Layout, extern über die Monitoraufnahme belegt; ein leeres Fenster genügt nicht. Bilder und Highlighting müssen dafür nicht fertig sein.

Mindestens 30 Wiederholungen für die regulären Zeitmessungen; Median, p95 und Rohdaten speichern. Parsen, Blockplan, Layout des Sichtbereichs und erstes Zeichnen separat instrumentieren. Scrollwerte über Compositor-Daten prüfen.

Nach 50 Dateiwechseln darf kein fortlaufendes Wachstum verbleiben; nach Beruhigung liegt der Verbrauch höchstens 20 % über der aufgewärmten Ausgangsmessung.

Eine Überschreitung führt zu Profiling und einer dokumentierten Entscheidung. Ziele werden nicht stillschweigend angehoben.

## 10. Regeln für schnelle Darstellung

- Kritischen Startpfad klein halten: Fenster, Theme und erster Sichtbereich zuerst. Kein Fenster ohne Inhalt zeigen, wenn der Inhalt in derselben Bildwiederholung fertig werden kann.
- Keine Netzwerkabhängigkeit beim Start, kein Splashscreen, keine künstliche Mindestladezeit.
- Ab etwa 150 ms darf ein dezenter Ladehinweis erscheinen; schnelle Vorgänge erzeugen kein Ladeflackern.
- Parsen, Syntaxhervorhebung und Bilddekodierung laufen auf Arbeitsthreads. Blockplan, Pango-Layout und Zeichnen bleiben auf dem Hauptthread und damit ausdrücklich im 16-ms-Budget.
- Nur sichtbare Blöcke zuzüglich eines Bildschirmpuffers werden gesetzt. Für kein Fixture existiert ein Zustand, in dem das gesamte Dokument gesetzt ist.
- Bilder nach Nähe zum Sichtbereich dekodieren. Bekannte Abmessungen reservieren Platz.
- Syntaxhervorhebung nur für sichtbare oder bald sichtbare Codeblöcke, Ergebnis pro Block zwischengespeichert. Sehr große Blöcke bleiben unkoloriert.
- Kein Polling im Leerlauf, keine permanente Animationsschleife, kein Timer ohne Zweck. Im Leerlauf zeichnet die Anwendung nicht.
- Messen und Zeichnen nicht verschachteln: erst alle sichtbaren Blöcke messen, dann in einem Durchgang zeichnen.
- Ein konfigurierbarer Entwicklungsgrenzwert von 20 MiB für Markdown verhindert unkontrolliertes Einlesen. Überschreitungen ergeben eine klare Meldung. Bildlimits werden in M0 festgelegt und vor Freigabe dokumentiert.

## 11. Inhalts- und Systemgrenzen

Markdown-Dateien sind Dokumentinhalt und erhalten keine Ausführungsrechte.

Der native Renderer verkleinert das Bedrohungsmodell erheblich: Es gibt keine Skript-Laufzeit, keinen HTML-Parser, keine privilegierte Brücke zwischen Dokumentinhalt und Systemfunktionen und damit keine XSS-artige Angriffsfläche. Die verbleibenden Grenzen sind:

- **Pfad und Dateizugriff.** Pfadkanonisierung, Symlinks, URL-Dekodierung und erlaubte Ressourcenbereiche werden an einer Stelle behandelt. Einschränkungen müssen beim tatsächlichen Lesen wirksam bleiben, nicht nur bei der Prüfung davor.
- **Bilddekodierung** ist die größte verbleibende Angriffsfläche, weil sie fremde Binärdaten in einem C-Decoder verarbeitet. Deshalb Pixel- und Speicherbudget vor der Dekodierung, und in M0 die Entscheidung für einen sandboxenden Loader.
- **Dokumentinhalt darf keine Bedienelemente imitieren.** Die Ansicht rendert Dokumenttext; sie erzeugt keine anklickbaren Bedienelemente aus Dokumentdaten außer Links und Bildern.
- **Links** öffnen nur nach Benutzerklick und nur mit erlaubtem Schema.
- Keine Telemetrie. Lokale Diagnoseprotokolle enthalten standardmäßig weder Dokumentinhalt noch vollständige private Pfade.

## 12. Qualitätssicherung

Tests prüfen Benutzerverhalten und Modulverträge, nicht bloß interne Implementierungsdetails.

- **Unit- und Vertragstests (Cargo):** Parser-Konfiguration und CommonMark-Vergleich, Überschriften-IDs, Op-Buffer-Kodierung und Blocktextbereiche, URL-/Pfadauflösung, Suchtrefferabbildung, veraltete Ergebnisse, Einstellungsvalidierung.
- **Layouttests:** Blockplan-Höhen gegen tatsächlich gemessene Höhen; Korrektur oberhalb der Leseposition verschiebt den Scrolloffset korrekt; Auswahlordnung über Blockgrenzen; Hit-Testing an Blockrändern; Umbruch bei Breiten- und Zoomwechsel. Diese Tests brauchen ein Pango-Kontextobjekt, aber kein Fenster.
- **Integrationstests:** Schnell A und danach B öffnen; spätes Ergebnis von A darf B nicht ersetzen. Speichern über Rename, Löschen/Wiederanlegen, Watcher-Aufräumen und fehlende Dateien prüfen.
- **UI-Abnahme:** Release-App unter Wayland, zusätzlicher X11-Smoke-Test. Dateimanager-Aufruf, Drag-and-drop, Clipboard, Instanzübergabe, Tastaturbedienung, Fokus, Leer-/Lade-/Fehlerzustände, Theme und Zoom. Die frühere Playwright-Suite entfällt ersatzlos.
- **Visuelle Abnahme gegen die Referenzfassung:** Vor dem Entfernen von `src/` werden Referenzaufnahmen der WebView-Fassung erstellt — je Theme, bei mindestens drei Fensterbreiten, 100 und 200 Prozent Zoom, über alle Fixtures des Rendering-Vertrags. Die native Fassung wird gegen diese Aufnahmen gestellt. Abweichungen außerhalb der Fensterleiste sind zu beheben oder mit Begründung in `docs/decisions/` festzuhalten; sie gelten nicht stillschweigend als neue Gestaltung. Zusätzlich HiDPI und fraktionale Skalierung prüfen.
- **Performance:** Messungen aus Abschnitt 9, getrennt von variabler allgemeiner CI. Regressionen auf derselben Referenz vergleichen.

Bei Codeänderungen laufen `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` und ein Release-Build.

## 13. Migration vom WebView-Stand

Der Wechsel ist kein Neuanfang. Was bleibt, ist der Teil, der teuer war und nichts mit dem Renderer zu tun hat.

**Übernommen:**

- `hashline-markdown` einschließlich Op-Buffer, Slug-Regel, Abschnittsbildung und Tests, mit den Änderungen aus Abschnitt 6.
- Der gesamte Benchmark-Bestand: Generatoren, Fixtures, Prozess- und Compositor-Werkzeuge, Rohdaten. Alte Reihen bleiben als historische Vergleichsbasis erhalten und werden nicht überschrieben.
- Diese Spezifikation, die Entscheidungen in `docs/decisions/` und der Produktumfang.
- Desktop-Eintrag, Anwendungs-ID, MIME-Zuordnung und Paketierungswissen aus M3.
- **Das Erscheinungsbild.** `src/app/styles.css` und `src/ui/titlebar.css` wandern vor dem Entfernen von `src/` nach `docs/design/` und bleiben dort als Gestaltungsreferenz erhalten. Dazu die Referenzaufnahmen aus Abschnitt 12, die ohne laufende WebView-Fassung nicht mehr herstellbar wären.

**Entfernt:**

- `src/` vollständig: React-Oberfläche, Viewport, Op-Buffer-Dekoder, Worker, Content-Policy — nachdem die beiden Stylesheets nach `docs/design/` gesichert sind.
- Die Tauri-Hülle in `src-tauri/` außer der Parser-Crate.
- Node-Toolchain, `package.json`, Lockfiles, Vite, Playwright, DOMPurify, der WASM-Build und `scripts/`.
- `docs/decisions/006-custom-titlebar.md` wird als überholt markiert, nicht gelöscht.

**Reihenfolge:** Zuerst die Parser-Crate nach `crates/markdown` verschieben und grün halten. Dann Gestaltungsreferenz und Referenzaufnahmen sichern. Dann M0 als eigenständige Binärdatei daneben aufbauen. Erst wenn M0 abgenommen ist, den alten Stand entfernen — nicht vorher, damit Vergleichsmessung und visueller Abgleich bis zuletzt möglich bleiben.

## 14. Umsetzungsschritte mit Abnahmekriterien

### M0 — Technische Grundlage und Performance-Nachweis

Ein GTK4-Fenster mit dem eigenen Dokumentwidget, das den Op-Buffer setzt. Der Meilenstein hat vier Risiken zu entkräften, und nur diese vier:

1. **Virtualisiertes Blocklayout** mit korrekter Scrollhöhe und ohne springenden Inhalt bei Höhenkorrekturen.
2. **Auswahl über Blockgrenzen** mit Hit-Testing und Kopieren in die Zwischenablage.
3. **10-MiB-Fixture**: Öffnen, Scrollen und Suchen innerhalb der Budgets.
4. **Scroll-Frametimes** bei 60 und 120 Hz über die Compositor-Messung.

Dazu Referenzumgebung festlegen, Entscheidungen zu Bilddekodierung und Highlighter treffen und die abgebrochene Vergleichsreihe gegen ViewMD und den WebView-Stand mit n=30 nachholen.

**Abnahme:** Reproduzierbarer Release-Build, gespeicherter Benchmarkbericht, dokumentierte Entscheidungen. Verfehlt einer der vier Punkte sein Budget, wird vor M1 entschieden — nicht optimiert und weitergebaut.

### M1 — Nutzbarer und gestalteter Reader

Dateidialog, Drag-and-drop, vollständiger Rendering-Vertrag, Design-Tokens, Lesespalte, Themes, Textzoom, Links, Bilder sowie Leer- und Fehlerzustände.

**Abnahme:** Die unterstützten Fixtures sind korrekt und gut lesbar. Die visuelle Abnahme aus Abschnitt 12 ist durchgeführt: Die native Fassung stimmt außerhalb der Fensterleiste mit den Referenzaufnahmen überein, verbleibende Abweichungen sind begründet festgehalten. Auswahl, Kopieren und Tastaturbedienung funktionieren. Kern-Performance bleibt innerhalb der Ziele.

### M2 — Navigation und verlässliche Aktualisierung

Suche, Inhaltsverzeichnis, Code-Kopieren, Syntaxhervorhebung, Dateibeobachtung und Lesepositionen.

**Abnahme:** Schnelle Dateiwechsel und atomisches Speichern erzeugen keine veralteten Ansichten. Suche und Reload zerstören weder Fokus noch Lesefluss. Kein fortlaufendes Speicherwachstum über 50 Wechsel.

### M3 — Linux-Auslieferung und Abschlussprüfung

CLI, Instanzübergabe, Desktop-Eintrag, MIME-Zuordnung, GSettings-Schema, Icon und Installation. Der Desktop-Eintrag verwendet `Name=Hashline`, `GenericName=Markdown Viewer` und `Exec=hashline %f`. Zunächst ein `.deb` für die in M0 festgelegte Referenzdistribution; weitere Formate folgen und dürfen die v1-Abnahme nicht verdecken.

**Abnahme:** Installation und Öffnen per Dateimanager auf einer sauberen unterstützten Umgebung funktionieren. Build-Anleitung, bekannte Einschränkungen und Benchmarkbericht liegen vor. Alle v1-Funktionen sowie Performance- und Darstellungsziele sind überprüft.

### Stand der ergänzten Migration (8. September 2026)

Die folgenden Lücken der nativen Fassung sind umgesetzt. Auch die **M3-Auslieferungsfunktionen sind implementiert**: nativer `.deb`-Build, Desktop-/MIME-Registrierung, Icon, Schema-Installation und Pakettests. Build und Installation stehen in [docs/installation.md](docs/installation.md), Nachweise und offene Freigabekriterien im [M3-Bericht](docs/acceptance/M3.md). **Die vollständige M3-/v1-Abnahme bleibt offen**; Pakettests ersetzen keine Performance- und Darstellungsabnahme.

| Punkt | Umsetzung |
| --- | --- |
| Eine aktive Datei in einem Fenster | `open` und `activate` verwenden denselben Fenstercontroller. Ein weiterer Aufruf ersetzt die Datei und aktiviert das vorhandene Fenster. |
| Aktueller Abschnitt im Inhaltsverzeichnis | Überschriften werden beim Dokumentwechsel aufgebaut; die aktive Zeile folgt Scrollposition, Abschnittssprüngen und Layoutkorrekturen. |
| Menü in der HeaderBar | Öffnen, Nachladen, Inhaltsverzeichnis, Suche, Darstellungsmodus und Zoom verwenden Fensteraktionen. |
| System-, Hell- und Dunkelmodus | Zustandsbehaftete Themenaktion mit GSettings-Präferenz; Systemmodus liest den Desktop-Settings-Portalwert mit GTK-Fallback. Das erste Fenster wartet auf die initiale Themenauflösung. |
| Escape | Schließt zuerst ein geöffnetes Menü, sonst das zuletzt geöffnete Inhaltsverzeichnis oder die Suche; Fokus geht an die verbleibende Ansicht zurück. |
| Mehrere Dateien | Erster Eintrag aus CLI/Dateimanager oder Drag-and-drop wird geöffnet; ein kurz sichtbarer Hinweis bleibt auch nach erfolgreichem Laden erhalten und ist als zugängliche Statusmeldung ausgezeichnet. |
| AT-SPI | Dokumentrolle und `GtkAccessibleText` liefern Text, Unicode-Zeichenoffsets, Textbereiche, visuelle Zeilen, Auswahl und Cursor; Änderungen werden gemeldet. |
| Textauswahl | Markierung verwendet Pango-Layoutkoordinaten je visueller Zeile, einschließlich Bidi-Bereichen. Auswahl bleibt nach Loslassen erhalten; Doppelklick wählt ein Wort, Dreifachklick einen Block, Shift-Klick erweitert die Auswahl. Links werden erst beim Loslassen ohne Auswahl geöffnet. |

Nachweise: Rust-Workspace-Tests, Pango-Regressionstest für umgebrochene Auswahl, GTK-Integrationstest und ein nativer AT-SPI-Test mit zwei echten Anwendungsaufrufen. Reproduzierbare Befehle stehen unter [Native Migration](docs/testing.md#native-migration).

Diese Funktionsprüfungen ersetzen weder den vollständigen visuellen Vergleich und die Performance-Messreihen aus M0–M2 noch die Abschlussprüfung von M3. Der historische WebView-Stand bleibt entsprechend Abschnitt 13 als Vergleichsbasis erhalten, solange die M0-Abnahme nicht belegt ist.

## 15. Definition of Done

v1 ist abgeschlossen, wenn der beschriebene Funktionsumfang im installierten Linux-Build funktioniert, die Architekturgrenzen eingehalten werden und die Abnahmekriterien aus M0–M3 erfüllt sind.

Zwei zusätzliche Bedingungen, die aus dem Produktziel folgen:

- Die Budgets aus Abschnitt 9 sind mit n=30 auf der Referenzmaschine belegt, einschließlich der Zeile zum Speicherzuwachs über die Dokumentgröße.
- Die visuelle Abnahme gegen die Referenzaufnahmen ist durchgeführt. Die native Fassung sieht aus wie die bisherige, abgesehen von der nativen Fensterleiste. Ein schnelles Programm, dessen Textsatz nicht überzeugt, erfüllt die Definition nicht — das war der Anlass des Projekts.

Abweichungen von dieser Spezifikation werden mit Problem, Messung, gewählter Lösung und Auswirkung in `docs/decisions/` dokumentiert.

## 16. Technische Referenzen

- [gtk4-rs: Buch und API](https://gtk-rs.org/gtk4-rs/stable/latest/book/)
- [GTK4: eigene Widgets und `snapshot`](https://docs.gtk.org/gtk4/class.Widget.html)
- [GTK4: `GtkScrolledWindow` und kinetisches Scrollen](https://docs.gtk.org/gtk4/class.ScrolledWindow.html)
- [GTK4: Barrierefreiheit und Rollen](https://docs.gtk.org/gtk4/section-accessibility.html)
- [Pango: Layout, Metriken und Hit-Testing](https://docs.gtk.org/Pango/class.Layout.html)
- [GSK: Render-Nodes](https://docs.gtk.org/gsk4/)
- [GIO: `GApplication` und `HANDLES_OPEN`](https://docs.gtk.org/gio/class.Application.html)
- [GIO: `GSettings`](https://docs.gtk.org/gio/class.Settings.html)
- [pulldown-cmark](https://docs.rs/pulldown-cmark/)
- [syntect](https://docs.rs/syntect/)
- [notify](https://docs.rs/notify/)

# Hashline — Umsetzungsspezifikation

Stand: 8. September 2026. Status: Grundlage für die Implementierung von v1.

## 1. Produktziel

Hashline ist ein schneller, ruhiger Markdown-Viewer für Linux. Eine Datei soll sich unmittelbar öffnen lassen, angenehm lesbar sein und auch bei umfangreichen Dokumenten flüssig bedienbar bleiben. Die Gestaltung ist modern, reduziert und durch gute Typografie geprägt.

Die Anwendung ist ein lokales Desktop-Programm mit Tauri 2, React und TypeScript. Die Darstellung erfolgt mit HTML/CSS in der Linux-System-WebView WebKitGTK. „Native“ bedeutet hier natives Anwendungsfenster und Betriebssystemintegration; die Dokumentansicht und Bedienelemente sind Web-Inhalte.

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

Die Anwendungs-ID folgt der umgekehrten Domain des Herausgebers und wird vor der ersten Paketierung in M3 bestätigt. Sie muss über Desktop-Eintrag, Fenster-Klasse und Paketmetadaten identisch verwendet werden.

## 2. Umfang von v1

### Enthalten

- Eine aktive Datei in einem Anwendungsfenster.
- Öffnen über Dateidialog, Drag-and-drop, Dateimanager und `hashline <datei>`.
- Bei erneutem Aufruf die Datei an die laufende Instanz übergeben und das Fenster aktivieren.
- CommonMark-Grundsyntax und die vereinbarten GFM-Erweiterungen aus Abschnitt 6.
- Lokale Bilder, Dokumentanker, relative Markdown-Links und externe Links.
- Einklappbares Inhaltsverzeichnis mit Kennzeichnung des aktuellen Abschnitts.
- Dokumentweite Textsuche mit Trefferzahl sowie nächstem/vorherigem Treffer.
- Textauswahl und Kopieren über Absatzgrenzen hinweg; Codeblock kopieren.
- Automatisches Nachladen bei Dateiänderungen unter Erhalt der Leseposition.
- System-, Hell- und Dunkelmodus; Textzoom.
- Speicherung von Darstellungspräferenzen, Fensterzustand und zuletzt verwendeten Lesepositionen.
- Installation mit Desktop-Eintrag, Icon und Markdown-Dateizuordnung.

### Spätere Erweiterungen

Editor, Tabs, Projekt-/Dateibaum, Workspace-Verwaltung, Cloud-Sync, Konten, Plugins, PDF-Export, Mermaid, mathematische Formeln und ausführbare Inhalte gehören nicht zu v1. Beliebige Webseiten-Kompatibilität für eingebettetes HTML ist kein Ziel.

Die App verändert keine geöffneten Markdown-Dateien. Aufgabenlisten bleiben schreibgeschützt.

## 3. Oberfläche und Interaktion

### Fensteraufbau

- Vorerst eine selbst gestaltete Fensterleiste (Custom Titlebar) ohne native Fensterdekoration. Die aktuelle Umsetzung ist die Grundlage für v1; die Entscheidung ist in [006-custom-titlebar.md](docs/decisions/006-custom-titlebar.md) festgehalten.
- Die kompakte Leiste vereint Datei öffnen, Dateiname, Inhaltsverzeichnis, Suche, ein kleines Menü und Fenstersteuerung. Sie folgt dem gewählten Theme und zeigt den Fensterfokus an.
- Dateiname und freie Leistenflächen dienen zum Verschieben; ein Doppelklick maximiert das Fenster oder stellt es wieder her. Bedienelemente lösen keine Fensterbewegung aus.
- Unter Linux stehen Minimieren, Maximieren/Wiederherstellen und Schließen rechts in der Leiste zur Verfügung. Größenänderungen an Fensterrändern und -ecken bleiben möglich. Fensteraktionen laufen über den nativen Tauri-Adapter.
- Fensterknöpfe besitzen zugängliche Namen, sichtbaren Tastaturfokus und Tastaturbedienung. In der Browser-Vorschau werden native Fensterknöpfe und Resize-Flächen ausgeblendet.
- Der vollständige Pfad ist bei Bedarf abrufbar, aber kein dauerhaftes Gestaltungselement.
- Der Dokumentbereich nimmt den verbleibenden Platz ein. Keine permanente Statusleiste.
- Inhaltsverzeichnis initial geschlossen. Die gewählte Sichtbarkeit wird gespeichert.
- Ab etwa 900 CSS-Pixeln Fensterbreite öffnet es als Seitenleiste; darunter als schließbares Overlay ohne Verengung des Dokuments.
- Ohne Datei: ruhige Leeransicht mit „Markdown-Datei öffnen“ und Drag-and-drop-Hinweis.
- Mehrere gleichzeitig übergebene Dateien: erste Datei öffnen und kurz auf die Beschränkung auf eine Datei hinweisen.

### Gestaltung

- Zentrale CSS-Variablen für Farben, Abstände, Radien, Typografie und Bewegungsdauer.
- Neutrale Flächen, eine zurückhaltende Akzentfarbe, dezente Trennlinien.
- Keine dekorativen Verläufe, großflächigen Unschärfen oder permanenten Animationen.
- Lesespalte standardmäßig etwa 76 `ch` breit, mit responsiven Außenabständen.
- Fließtext als Ausgangspunkt 17 CSS-Pixel und Zeilenhöhe 1,65; anhand realer Dokumente feinjustieren.
- Systemschrift für Text und lokale Monospace-Fallbacks für Code. Keine Schrift-Downloads beim Start.
- Deutliche Überschriftenhierarchie; konsistente Abstände für Listen, Zitate und Code.
- Codeblöcke und breite Tabellen scrollen horizontal innerhalb ihres Bereichs. Die Anwendung selbst scrollt nicht horizontal.
- Bilder passen sich der Lesespalte an. Fehlende Bilder erhalten einen unaufdringlichen Platzhalter mit Alternativtext.
- Hover- und Fokusübergänge dauern ungefähr 100–150 ms. `prefers-reduced-motion` wird berücksichtigt.
- Bei Themenwechseln und beim Start kein vermeidbares Aufblitzen des falschen Farbschemas.

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

Textzoom: 80–200 Prozent in 10-Prozent-Schritten, nur für das Dokument. Scrollen folgt dem Plattformverhalten; kein eigener Scroll-Animator. Abschnittssprünge dürfen kurze Bewegung verwenden, bei reduzierter Bewegung sofort springen.

Semantische HTML-Elemente, zugängliche Namen, sichtbarer Tastaturfokus und vollständige Tastaturbedienung sind Pflicht. Normaler Text erfüllt mindestens 4,5:1 Kontrast. Suche und Overlay geben den Fokus beim Schließen sinnvoll zurück. Inhalte müssen auch bei 200 Prozent Textzoom nutzbar bleiben.

## 4. Technische Entscheidungen

| Aufgabe | Entscheidung |
| --- | --- |
| Desktop-Laufzeit | Tauri 2 mit System-WebView |
| Oberfläche | React + TypeScript im Strict-Modus |
| Frontend-Build | Vite, statisch gebündelte lokale Assets |
| Gestaltung | CSS, gemeinsame Design-Tokens, gekapselte Komponentenstile |
| Markdown-Parser | `marked` hinter einem kleinen Adapter |
| HTML-Bereinigung | DOMPurify mit expliziter Inhaltsrichtlinie |
| Syntaxhervorhebung | Verzögert geladener Highlighter mit begrenzter Sprachauswahl; Auswahl nach Prototypmessung |
| Systemzugriff | Tauri-Plugins, ergänzt durch kleine native Adapter bei Bedarf |
| Paketverwaltung | npm mit eingechecktem Lockfile; Cargo-Lockfile ebenfalls einchecken |

Node.js wird für die Entwicklung und den Build verwendet. Die ausgelieferte Anwendung braucht keinen Node-Server. Konkrete kompatible Paketversionen werden beim Projektstart festgelegt und durch Lockfiles reproduzierbar gehalten.

Die Rust-Seite bleibt auf Fenster-/Instanzintegration, validierten Systemzugriff und notwendige Ressourcenbereitstellung begrenzt. Markdown-Regeln und Produktlogik werden nicht parallel in Rust und TypeScript implementiert.

## 5. Architektur und Verantwortlichkeiten

```text
React-Oberfläche
    │ Benutzeraktionen / Ansichtszustand
    ▼
DocumentController
    ├── DocumentGateway ── Tauri-Adapter ── lokale Dateien / Watcher
    ├── MarkdownService ── Worker ── Parser / Überschriften
    ├── Inhaltsrichtlinie ── DOMPurify / URL-Prüfung
    └── unveränderliches RenderDocument
                              │
                              ▼
                       DocumentViewport
                       HTML/CSS, Auswahl, Suche, Navigation
```

### Modulgrenzen

- **App/React:** Fensterinhalt, Werkzeugleiste, Sucheingabe, Inhaltsverzeichnis und Einstellungen. Keine direkten Dateizugriffe aus UI-Komponenten.
- **DocumentController:** Öffnen, Nachladen, Ladezustände, Versionsverwaltung, Fehler und Aufräumen. Er koordiniert, implementiert aber weder Parser noch DOM-Manipulation.
- **MarkdownService:** Reine Verarbeitung von Markdown zu HTML und Metadaten. Kennt weder React noch Tauri. Der Parser-Adapter kapselt die konkrete Bibliothek.
- **DocumentGateway:** Lesen, Metadaten, Beobachtung und Ressourcenfreigabe. Der Tauri-Adapter ist die einzige Frontend-Schicht mit Zugriff auf native APIs.
- **DocumentViewport:** Eigentümer des erzeugten Dokument-DOMs, der Auswahl, Suchmarkierungen und Scrollanker. Es erhält eine unveränderliche Parserrevision und bereinigt jeden Abschnitt unmittelbar vor dessen DOM-Einfügung.
- **Preferences:** Kleines versioniertes Einstellungsobjekt mit validierten Werten. Beschädigte Daten fallen auf Defaults zurück.

Diese Grenzen werden mit wenigen Modulen und expliziten Funktionen umgesetzt. Kein allgemeines Plugin-System, kein Dependency-Injection-Framework, kein globaler Event-Bus und keine vorsorgliche Datenbank.

### Zustandsmodell

Der Dokumentzustand unterscheidet `empty`, `loading`, `ready` und `error`. Beim Nachladen bleibt das vorhandene Dokument sichtbar und erhält einen separaten Aktualisierungsstatus.

- Jeder Öffnungs-/Ladevorgang erhält eine steigende `requestId`.
- Nur die aktuelle Anfrage darf das aktive Dokument ersetzen.
- Veraltete Lese-, Parse-, Bild- und Highlight-Ergebnisse werden verworfen.
- Ein neuer Auftrag ersetzt wartende Aufträge. Laufende Arbeit wird, soweit möglich, abgebrochen; lange Worker-Jobs dürfen durch Worker-Neustart beendet werden.
- Watcher, Event-Listener, Observer, Timer und Objekt-URLs besitzen einen klaren Lebenszyklus und werden beim Wechsel freigegeben.
- React hält dauerhaften UI-Zustand; Scrollpositionen und flüchtige DOM-Messungen liegen in Refs bzw. dem Viewport-Adapter.
- Eine Eingabe im Suchfeld darf weder Markdown erneut parsen noch den Dokument-HTML-Baum ersetzen.

### Verzeichnisstruktur

```text
src/
  app/                  # Zusammensetzung, Fensterlayout, globale Stile
  features/
    document/           # Controller, Viewport, Ladezustände
    outline/            # Inhaltsverzeichnis
    search/             # Suchsteuerung und Oberfläche
    preferences/        # Darstellung und Persistenz
  core/
    markdown/           # Parser-Adapter, Typen, Worker-Protokoll
    content/            # Bereinigung, Link-/Ressourcenrichtlinie
  platform/             # Schnittstellen und Tauri-Adapter
  workers/              # Markdown-Worker
  ui/                   # Tatsächlich gemeinsam verwendete Komponenten
src-tauri/              # Kleine native Hülle und Berechtigungen
tests/fixtures/         # Markdown-, Bild- und Fehlerfälle
benchmarks/             # Generatoren, Messabläufe und Ergebnisse
docs/                   # Architekturentscheidungen und Betriebsanleitung
```

Dateien werden angelegt, wenn die entsprechende Funktion implementiert wird; keine leeren Framework-Strukturen vorbereiten.

## 6. Markdown- und Rendering-Vertrag

### Unterstützte Inhalte

Absätze, Überschriften H1–H6, Hervorhebungen, durchgestrichener Text, Inline-Code, eingerückte und eingezäunte Codeblöcke, Zitate, geordnete und ungeordnete verschachtelte Listen, Trennlinien, Links, Bilder, GFM-Tabellen, Autolinks und schreibgeschützte Aufgabenlisten.

- Softbreaks und Hardbreaks folgen der vereinbarten Parser-Konfiguration, die mit Fixtures festgehalten wird.
- Überschriften erhalten deterministische, eindeutige IDs; doppelte Überschriften bekommen Suffixe. Umlaute, Unicode und leere Überschriften werden getestet.
- Inhaltsverzeichnis und Dokument verwenden dieselben IDs aus demselben Parse-Vorgang.
- Die genaue Slug-Regel wird dokumentiert; vollständige GitHub-Ankerkompatibilität wird nicht vorausgesetzt.
- Frontmatter wird in v1 als Quellinhalt nach Parser-Verhalten behandelt; es steuert keine Anwendungsfunktionen.
- Eingebettetes HTML wird auf eine getestete Auswahl passiver Inhaltselemente beschränkt. Skripte, Formulare, eingebettete Frames und Dokument-CSS werden nicht übernommen.
- Unbekannte Code-Sprachen erhalten lesbaren, unkolorierten Code.

### Datenfluss

1. Öffnungsauftrag normalisieren; Dateipfad und erlaubten Zugriff nativ prüfen.
2. Datei asynchron lesen, UTF-8 inklusive BOM unterstützen; ungültige Kodierung verständlich melden.
3. Markdown im wiederverwendeten Worker parsen; HTML, Überschriften und Ressourcenmetadaten erzeugen.
4. HTML auf dem Hauptthread mit DOMPurify bereinigen und Links/Bildquellen nach Inhaltsrichtlinie auflösen. DOMPurify nicht ohne DOM-Unterstützung im Worker voraussetzen.
5. Parserergebnis als unveränderliche Dokumentrevision an den Viewport geben; die Bereinigung aus Schritt 4 erfolgt dort abschnittsweise unmittelbar vor dem Einfügen.
6. Den ersten Abschnitt beziehungsweise Leseanker einfügen, erste lesbare Darstellung messen und weitere Abschnitte in abbrechbaren Aufgaben ergänzen.
7. Bilder, Suchindex und Syntaxhervorhebung nach Priorität ergänzen; Dokumentrevision bei jedem Ergebnis prüfen.

Nur der Viewport darf bereinigte Dokumentfragmente in seinen Dokument-DOM einfügen. Ungeprüftes Parser-HTML darf keinen anderen Weg in den DOM erhalten. Die Bereinigung kapselt auch den Umgang mit IDs, Attributen und Ressourcen-URLs.

Der Markdown-Inhalt wird als zusammenhängendes semantisches HTML gerendert. Eine React-Komponente pro Markdown-Token ist nicht der Standard. Normale UI-Updates dürfen den Dokument-DOM nicht neu erzeugen. Nachladungen ersetzen die Dokumentrevision vollständig; Bereinigung und DOM-Aufbau erfolgen nach Entscheidung 004 abschnittsweise und abbrechbar. Vorhandene Textknoten derselben Revision bleiben für Auswahl und Suche erhalten.

## 7. Dateien, Ressourcen und Aktualisierung

- Relative Pfade beziehen sich auf das Verzeichnis der geöffneten Datei, nicht das Prozess-Arbeitsverzeichnis.
- CLI-Pfade werden relativ zum Arbeitsverzeichnis des jeweiligen Aufrufers aufgelöst, auch bei Übergabe an die laufende Instanz.
- Dateinamen mit Leerzeichen, Unicode und URL-Escapes sowie atomisches Speichern über Umbenennung werden unterstützt.
- Ein Klick auf einen Markdown-Link öffnet das Ziel im selben Fenster; reine Fragmentlinks springen im aktuellen Dokument.
- Externe `https:`, `http:` und `mailto:`-Links öffnen nach Benutzerklick in der Systemanwendung. Sie navigieren niemals die privilegierte App-WebView.
- Sonstige Dateitypen und unbekannte URL-Schemata erhalten einen verständlichen Hinweis; kein Shell-Aufruf aus Dokumentinhalt.
- Lokale Bilder werden über kontrollierte Ressourcen-URLs bereitgestellt, nicht pauschal als große Base64-Daten über IPC kopiert.
- Automatischer Bildzugriff gilt für das Dokumentverzeichnis und dessen Unterverzeichnisse. Pfade außerhalb, einschließlich entweichender Symlinks, werden nicht automatisch freigegeben. Ein expliziter Datei-Öffnungsvorgang darf einen neuen Dokumentbereich wählen.
- Remote-Bilder laden standardmäßig nicht. Eine kompakte Aktion gibt sie nach Benutzerbetätigung für die aktuelle Dokumentrevision frei; Textdarstellung wartet niemals darauf. Die Freigabe gewährt keine nativen API-Rechte.

Dateiänderungen werden ereignisbasiert beobachtet und ungefähr 150 ms gebündelt. Ein Inhaltsvergleich verhindert unnötiges Neurendern. Auch das Ersetzen der Datei muss die Beobachtung überleben; dafür bei Bedarf das Elternverzeichnis mit Filter auf die aktive Datei beobachten. Watcher-Fehler lassen manuelles Nachladen verfügbar.

Beim Nachladen wird die nächste geeignete Überschrift bzw. ein stabiler Inhaltsblock mit relativem Viewport-Abstand als Leseanker verwendet. Falls dieser entfällt, dienen vorheriger Abschnitt und schließlich begrenzter Scrollfortschritt als Fallback. Bei späterem Bildlayout wird der Anker nur korrigiert, solange keine neue Benutzernavigation stattgefunden hat. Das Dokument springt nicht bei jedem Speichern nach oben.

Bei Lesefehlern bleibt ein bereits angezeigtes Dokument sichtbar, mit knapper Meldung und Wiederholen-Aktion. Beim Öffnen einer anderen Datei wird der angezeigte Dateiname erst nach erfolgreicher Übernahme gewechselt; die fehlgeschlagene Zieldatei wird in der Meldung genannt.

Persistenz ist auf Präferenzen und höchstens 100 zuletzt verwendete Pfade mit Leseposition begrenzt. Kein dauerhafter Dokumentinhalt-Cache in v1. Start ohne Dateiparameter zeigt die Leeransicht; gespeicherte Positionen gelten beim erneuten Öffnen. Fehlende bzw. beschädigte Einstellungen blockieren den Start nicht.

## 8. Suche, Auswahl und Inhaltsverzeichnis

- Suche arbeitet auf sichtbarem Dokumenttext, einschließlich Code und Tabellen; Markup und ausgeblendete Inhalte zählen nicht mit.
- v1 bietet eine wörtliche, nicht zwischen Groß-/Kleinschreibung unterscheidende Suche. Keine Regex- oder Fuzzy-Suche.
- Textknoten werden pro Dokumentrevision indiziert. Treffer werden auf DOM-Bereiche abgebildet; Markierungen ersetzen nicht das gesamte Dokument-HTML.
- Eine Suchanfrage wird kurz gebündelt; alte Suchergebnisse dürfen neuere nicht überschreiben.
- Verwendete Highlight-APIs werden gegen die unterstützte WebKitGTK-Version geprüft. Ein Fallback darf Auswahl und Kopieren nicht beschädigen.
- Eine Suche während der Syntaxhervorhebung behält gültige Textzuordnungen oder baut betroffene Zuordnungen kontrolliert neu auf.
- Die aktive Überschrift wird über Observer bzw. gebündelte Messungen bestimmt. Keine vollständige Durchmessung aller Überschriften pro Scrollereignis.
- Textauswahl über mehrere Absätze, Listen und Tabellen wird früh in der echten WebView getestet.
- Kopieren eines Codeblocks übernimmt den ursprünglichen Codetext ohne Bedienbeschriftungen.

## 9. Performance-Budgets und Messverfahren

### Referenz und Testdaten

In Meilenstein 0 werden eine reale Linux-Referenzmaschine mit SSD und integrierter Grafik sowie Distribution, CPU, RAM, Displayfrequenz, Skalierung, Sitzungstyp und WebKitGTK-Version dokumentiert. Die Referenz läuft zunächst mit 60 Hz; zusätzlich wird eine Prüfung bei 120 Hz durchgeführt, falls entsprechende Hardware verfügbar ist.

Messungen verwenden einen installierten Release-Build. Ein Vite-Entwicklungsserver oder ein Chromium-Browsertest ersetzt keine WebKitGTK-Messung.

| Fixture | Inhalt |
| --- | --- |
| Klein | Etwa 100 KiB UTF-8-Markdown mit typischer README-Struktur |
| Mittel | Etwa 1 MiB mit vielen Überschriften, Listen, Tabellen und Codeblöcken |
| Groß | Etwa 10 MiB gemischter Text als Belastungstest |
| Sonderfälle | Extrem lange Zeile, tiefe Listen, große Tabelle, viele kleine Blöcke, viele/große Bilder |

Fixtures sind deterministisch; Bytezahl, Blockzahl und erwartete DOM-Knotenzahl werden erfasst. Bilder werden zusätzlich mit komprimierter Größe und Pixelmaßen beschrieben. Textmessungen und Bildbelastung werden getrennt ausgewiesen.

### Ziele

| Metrik | Ziel auf der Referenzmaschine |
| --- | --- |
| Prozessstart bis erste lesbare Darstellung, kleine Datei | p95 ≤ 500 ms; ambitioniertes Optimierungsziel ≤ 200 ms |
| Öffnen in laufender Instanz, kleine Datei | p95 ≤ 150 ms |
| Öffnen in laufender Instanz, mittlere Datei | p95 ≤ 500 ms |
| Große Datei | Lesbar innerhalb 2 s; Oberfläche bleibt bedienbar und Öffnen einer anderen Datei möglich |
| Öffnen von Suche/Menü, normale UI-Reaktion | p95 ≤ 50 ms |
| Suchergebnisse, mittlere Datei | p95 ≤ 150 ms ab letzter Eingabe, inklusive Bündelung |
| Scrollen bei 60 Hz, kleine/mittlere Datei nach initialem Layout | Mindestens 99 % der Frames innerhalb 16,7 ms über 10 s; kein Stillstand > 50 ms |
| Hauptthread-Arbeit während normaler Interaktion | Keine anwendungsbedingten zusammenhängenden Aufgaben > 50 ms |
| Ruhender Speicher, kleine Datei | Gesamte App-Prozessgruppe ≤ 200 MiB PSS als anfängliches Budget |
| Leerlauf nach Abschluss aller Arbeiten | Im Mittel < 1 % eines CPU-Kerns über 30 s auf der Referenzmaschine |

Prozessstart bedeutet: kein App-Prozess aktiv, Betriebssystem-Dateicache unkontrolliert. Ein echter Start mit kaltem Dateicache wird separat ausgewiesen und nicht mit diesen Messreihen vermischt. „Erste lesbare Darstellung“ meint tatsächlichen sichtbaren Dokumenttext nach Layout; ein leeres Fenster oder ein React-Commit genügt nicht. Bilder und Highlighting müssen dafür noch nicht fertig sein.

Mindestens 30 Wiederholungen für die regulären Zeitmessungen; Median, p95 und Rohdaten speichern. Native Startzeit, Datei-I/O, Worker-Verarbeitung, Bereinigung, DOM-Einfügung und erste Darstellung separat instrumentieren. JS-Zeitmarken und nachfolgende Animation-Frames liefern Hilfswerte; den sichtbaren Start zusätzlich mit externer Aufnahme oder geeigneter Plattformmessung validieren. Scrollwerte über Compositor-/Profiler-Daten prüfen; `requestAnimationFrame` allein beweist keine tatsächlich präsentierten Frames.

Speicher umfasst Haupt-, WebView- und zugehörige Hilfsprozesse. Shared Memory über PSS berücksichtigen; falls nur RSS verfügbar ist, das abweichende Verfahren ausdrücklich benennen. Nach 50 Dateiwechseln darf kein fortlaufendes Wachstum verbleiben; nach Beruhigung liegt der Verbrauch höchstens 20 % über der aufgewärmten Ausgangsmessung.

Eine Überschreitung führt zu Profiling und einer dokumentierten Entscheidung. Ziele werden nicht stillschweigend angehoben. Wenn WebView-Start oder Dokumentlayout die Kernziele grundsätzlich verhindern, wird die Architektur vor weiterem Funktionsausbau neu bewertet.

## 10. Regeln für schnelle Darstellung

- Kritischen Startpfad klein halten: Fenster, Grundstile und Dokumentanzeige zuerst.
- Keine Netzwerkabhängigkeit beim Start, kein vorgeschalteter Splashscreen und keine absichtliche Mindestladezeit.
- Ab etwa 150 ms darf ein dezenter Ladehinweis erscheinen; schnelle Vorgänge erzeugen kein Ladeflackern.
- Parsing im Worker entlastet die Oberfläche. HTML-Bereinigung, DOM-Einfügung und Layout bleiben ausdrücklich Teil des Hauptthread-Budgets.
- Bilder nach Nähe zum sichtbaren Bereich laden. Bekannte Abmessungen reservieren Platz; Pixel- und Ressourcenlimits vor großen Dekodierungen berücksichtigen.
- Syntaxhervorhebung nur für sichtbare bzw. bald sichtbare Codeblöcke, mit beschränkter Sprachauswahl. Sehr große Blöcke bleiben zunächst unkoloriert.
- Worker-Aufträge und Caches bleiben begrenzt. Maximal ein aktueller und ein ersetzbarer wartender Parse-Auftrag; kein unbegrenzter Dokumentcache.
- Keine synchronen Dateizugriffe, kein Polling im Leerlauf und keine permanenten Animation-Loops.
- Layout lesen und schreiben bündeln; keine wiederholten Mess-/Schreibwechsel in Schleifen.
- Virtualisierung und `content-visibility` erst nach Profiling und Prüfung von Auswahl, Suche, Ankern und Barrierefreiheit einsetzen.
- Der 10-MiB-Test ist bewusst ein früher Belastungstest. Falls zusammenhängendes HTML die Oberfläche zu lange blockiert, wird abschnittsweise Darstellung als eigene Architekturentscheidung geprüft.
- Ein zunächst konfigurierbarer Entwicklungsgrenzwert von 20 MiB für Markdown verhindert unkontrolliertes Einlesen. Überschreitungen ergeben eine klare Meldung. Bildlimits werden in Meilenstein 0 anhand von Pixelzahl und Speicherbudget festgelegt und vor Freigabe von v1 dokumentiert.

## 11. Inhalts- und Systemgrenzen

Markdown-Dateien sind Dokumentinhalt und erhalten keine Ausführungsrechte. Diese Trennung ist Teil der Architektur, insbesondere weil eine Tauri-WebView native Funktionen erreichen kann.

- HTML-Bereinigung entfernt Skripte, Event-Handler, aktive Einbettungen, gefährliche URLs und fremde Styles. Links und Bildquellen werden auch nach Umschreibungen erneut gegen die Richtlinie geprüft.
- Dokumentattribute dürfen keine App-Bedienelemente imitieren oder unkontrolliert IDs des App-DOMs überschreiben.
- CSP beschränkt Skripte auf die gebündelte Anwendung. Remote-Inhalte erhalten keinen Zugriff auf Tauri-APIs.
- Native Berechtigungen werden auf benötigte Fenster, Befehle und Ressourcen beschränkt. Keine pauschale Freigabe des gesamten Home-Verzeichnisses oder Shell-Ausführung.
- Eigene Rust-Commands prüfen Argumente und Zugriff selbst; eine TypeScript-Prüfung ist keine Sicherheitsgrenze.
- Pfadkanonisierung, Symlinks, URL-Dekodierung und erlaubte Ressourcen werden im nativen Adapter konsistent behandelt. Einschränkungen müssen beim tatsächlichen Lesen wirksam bleiben.
- Erlaubnisse für Dateien außerhalb des aktuellen Bereichs entstehen aus einem expliziten Öffnungsvorgang; eingebettete Dokumentinhalte dürfen sie nicht selbst erweitern.
- Keine Telemetrie in v1. Lokale Diagnoseprotokolle enthalten standardmäßig weder Dokumentinhalt noch vollständige private Pfade.

## 12. Qualitätssicherung

Tests prüfen Benutzerverhalten und Modulverträge, nicht bloß interne Implementierungsdetails.

- **Unit-/Vertragstests:** Parser-Konfiguration, Überschriften-IDs, Bereinigung, URL-/Pfadauflösung, Worker-Protokoll, veraltete Ergebnisse und Einstellungsvalidierung.
- **Integrationstests:** Schnell A und danach B öffnen; spätes Ergebnis von A darf B nicht ersetzen. Speichern über Rename, Löschen/Wiederanlegen, Watcher-Aufräumen und fehlende Dateien prüfen.
- **Viewport-Tests:** Auswahl über Blockgrenzen, Treffer in Tabellen/Code, mehrfach gleiche Überschriften, Ankersprünge und Leseposition nach Nachladen/Bildlayout.
- **UI-Tests:** Tastaturbedienung, Fokus, Leer-/Lade-/Fehlerzustände, Theme und Zoom. Browserbasierte Tests beschleunigen Feedback, ersetzen aber keine Desktop-Abnahme.
- **Linux-Abnahme:** Release-App in WebKitGTK unter Wayland; zusätzlicher X11-Smoke-Test, sofern von der gewählten Distribution unterstützt. Dateimanager-Aufruf, Drag-and-drop, Clipboard und Single-Instance-Verhalten prüfen.
- **Visuelle Abnahme:** Helle/dunkle Darstellung, schmale/breite Fenster, 100/200 Prozent Textzoom sowie HiDPI-/fraktionale Skalierung auf verfügbarer Hardware.
- **Performance:** Messungen aus Abschnitt 9, getrennt von variabler allgemeiner CI. Regressionen auf derselben Referenz vergleichen.

Bei Codeänderungen laufen Format-/Lintprüfung, TypeScript-Prüfung, relevante Tests und der Frontend-Produktionsbuild. Native Änderungen ergänzen Rust-Formatprüfung, Clippy und passende native Tests. Vor Meilensteinfreigabe ist ein Tauri-Release-Build erforderlich.

## 13. Umsetzungsschritte mit Abnahmekriterien

### M0 — Technische Grundlage und Performance-Nachweis

Tauri, React, TypeScript und Vite einrichten. Eine lokale Datei durch die geplante Pipeline anzeigen; Auswahl, einfache Suche und Tabellen/Code anhand der Fixtures erproben. Referenzumgebung und unterstützte WebKitGTK-/Distributionsversion festlegen. Start, große Dokumente, Scrollen und Speicher messen. Dynamische Dateifreigaben und lokale Bild-URLs im tatsächlichen Tauri-Build nachweisen.

**Abnahme:** Reproduzierbarer Release-Build, gespeicherter erster Benchmarkbericht und dokumentierte Entscheidungen zu Ressourcenbereitstellung, Größenlimits und Highlighter. Kritische Abweichungen von den Performance-Zielen sind behoben oder führen zu einer expliziten Architekturentscheidung vor M1.

### M1 — Nutzbarer und gestalteter Reader

Dateidialog, Drag-and-drop, Rendering-Vertrag, Design-Tokens, responsive Lesespalte, Themes, Textzoom, Links/Bilder sowie Leer-/Fehlerzustände implementieren.

**Abnahme:** Die unterstützten Markdown-Fixtures sind korrekt und gut lesbar. Auswahl, Kopieren und Tastaturbedienung funktionieren; Kern-Performance bleibt innerhalb der Ziele.

### M2 — Navigation und verlässliche Aktualisierung

Vollständige Suche, Inhaltsverzeichnis, Code-Kopieren, verzögerte Hervorhebung, Dateibeobachtung und Lesepositionen implementieren.

**Abnahme:** Schnelle Dateiwechsel und atomisches Speichern erzeugen keine veralteten Ansichten. Suche und Reload zerstören weder Fokus noch Lesefluss. Kein fortlaufendes Speicherwachstum.

### M3 — Linux-Auslieferung und Abschlussprüfung

CLI, Instanzübergabe, Desktop-Eintrag, MIME-Zuordnung, Icon und Installation fertigstellen. Der Desktop-Eintrag verwendet `Name=Hashline`, `GenericName=Markdown Viewer` und `Exec=hashline %f`; Binary, Paket und Anwendungs-ID entsprechen der Tabelle in Abschnitt 1. Zunächst ein `.deb` für die in M0 festgelegte Referenzdistribution erzeugen. Weitere Paketformate folgen nach Bedarf; zusätzliche Formate dürfen die v1-Abnahme nicht verdecken.

**Abnahme:** Installation und Öffnen per Dateimanager auf einer sauberen unterstützten Umgebung funktionieren. Build-/Entwicklungsanleitung, bekannte Einschränkungen und Benchmarkbericht liegen vor. Alle v1-Funktionen sowie Performance- und Darstellungsziele sind überprüft.

## 14. Definition of Done

v1 ist abgeschlossen, wenn der beschriebene Funktionsumfang im installierten Linux-Build funktioniert, die Architekturgrenzen eingehalten werden und die Abnahmekriterien aus M0–M3 erfüllt sind. Ein im Browser gut aussehender Prototyp allein erfüllt die Definition nicht.

Abweichungen von dieser Spezifikation werden mit Problem, Messung, gewählter Lösung und Auswirkung in `docs/decisions/` dokumentiert. Ein kurzer Eintrag genügt; zusätzliche Architekturmechanismen brauchen einen konkreten Nutzen.

## 15. Technische Referenzen

Die folgenden Primärquellen begründen die verwendeten Plattformmöglichkeiten. Die Produktentscheidungen und Performance-Budgets oben sind projektspezifische Vorgaben.

- [Tauri: Architektur und System-WebView](https://v2.tauri.app/concept/architecture/)
- [Tauri: Prozessmodell einschließlich WebKitGTK](https://v2.tauri.app/concept/process-model/)
- [Tauri: React-/TypeScript-Projektvorlagen](https://v2.tauri.app/start/create-project/)
- [Tauri: Vite-Integration](https://v2.tauri.app/start/frontend/vite/)
- [Tauri: Dateisystem-Plugin und Watcher](https://v2.tauri.app/plugin/file-system/)
- [Tauri: Single-Instance-Plugin](https://v2.tauri.app/plugin/single-instance/)
- [Tauri: Capabilities und Grenzen eigener Commands](https://v2.tauri.app/security/capabilities/)
- [Tauri: Debian-Paketierung](https://v2.tauri.app/distribute/debian/)
- [React: Render und Commit](https://react.dev/learn/render-and-commit)
- [React: HTML-Einfügung und DOM-Eigenschaften](https://react.dev/reference/react-dom/components/common)
- [Marked: Verarbeitung und erforderliche HTML-Bereinigung](https://marked.js.org/)

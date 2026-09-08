# Architektur und Rendering-Vertrag

`App` hält Darstellungs- und Navigationszustand. `DocumentController` koordiniert
Dateien, Revisionen und Watcher. `DocumentGateway` begrenzt den Plattformzugriff;
nur `src/platform/tauri.ts` importiert Tauri-Frontend-APIs. Der Browseradapter ist
eine Entwicklungshilfe und besitzt keine nativen Rechte.

Native Dokumente werden als Binärantwort übertragen: vier Bytes Metadatenlänge
(u32 little-endian), UTF-8-JSON-Metadaten und anschließend der unveränderte
UTF-8-Markdowntext. Damit entfällt das JSON-Escaping des gesamten Dokuments.
Der Adapter prüft die Paketgrenzen und dekodiert UTF-8 strikt.

`WorkerMarkdownService` verwendet einen wiederverwendbaren Worker. Ein neuer
Auftrag beendet einen noch beschäftigten Worker; es gibt keine unbeschränkte
Auftragswarteschlange. Nach 30 Sekunden ohne neue Parserarbeit wird der Worker
beendet, um seinen Lexer-Heap freizugeben. Der Controller prüft steigende Anfrage-IDs nach Lesen und
Parsing. Veraltete native Dokumenthandles werden freigegeben. Ein fehlgeschlagener
Dateiwechsel erhält Inhalt und Namen der erfolgreich geöffneten Datei.
Kontextunabhängige wiederkehrende Listen können ihren Tokenbaum innerhalb
derselben Lexerinstanz weiterverwenden. Dieser Cache umfasst höchstens 64
Einträge mit je höchstens 8 KiB Quelltext. Mögliches HTML im Dokument sowie
Referenz-/Tasksyntax in der Liste schließen diesen Pfad aus. Die globale
Inline-Auflösung bleibt in Marked; veränderliche äußere Token werden kopiert.

DOMPurify läuft auf dem Hauptthread mit einer expliziten Element-/Attributliste.
Anschließend werden IDs, Links, Aufgabenlisten und Ressourcen geprüft und
umgeschrieben. Der Worker gruppiert vollständige semantische Blöcke zu
HTML-Abschnitten; globale Referenzen und Überschriften-IDs werden zuvor für das
gesamte Dokument aufgelöst. Nur `DocumentViewport` fügt unmittelbar bereinigte
`DocumentFragment`s in sein DOM ein. Die Bereinigung und Einfügung laufen in
abbrechbaren Aufgaben mit echten Event-Loop-Grenzen. Unsicheres rohes HTML behält
ein gemeinsames Fragment. Das Gruppierungsziel ist keine Größenobergrenze für
atomare Blöcke. Normale UI-Updates und Suche ersetzen den Baum nicht.

Abschnitte behalten ihre Textknoten; `content-visibility: auto` spart ausgelassenes
Layout. Sprungziele und Leseanker werden vorgezogen eingefügt und vor dem Messen
explizit sichtbar gemacht. Beim Wechsel bleiben unveränderte, bereits bereinigte
Abschnitte mit ihren Textknoten und Suchindizes verbunden. Bildhaltige Abschnitte
werden wegen dokumentgebundener Ressourcenrechte erneut aufgebaut. Nur nicht
wiederverwendete Abschnitte werden vom Ende her aufgabenweise entfernt.
Aufbau und Entfernung streben Aufgaben von 18 ms an; eigene Window-Nachrichten
bilden die abbrechbaren Event-Loop-Grenzen. Auswahl und Suche während des Aufbaus sowie die Grenzen
dieser Architektur sind in [Entscheidung 004](decisions/004-rendering-gate.md) beschrieben.

Die Suche indiziert nicht ausgeblendete Textknoten abschnittsweise je Revision,
einschließlich noch nicht gelayouteter Abschnitte. Sie wartet für vollständige
Ergebnisse auf den vollständigen DOM-Aufbau und zeigt den laufenden Zustand an. Geschlossene Details und
Code-Bedienbeschriftungen zählen nicht mit. Eine maskierte literale Regex mit
Unicode-Case-Folding behält die UTF-16-Offsets des Originals bei. Die Treffer liegen
als DOM-Ranges vor; nur sichtbare und aktive Treffer werden gezeichnet, damit
CSS Highlights kein Layout des ganzen Dokuments erzwingen. CSS Custom Highlights funktionieren in der geprüften WebKitGTK
2.52.6. Bei fehlender API zeichnet ein klickdurchlässiges Overlay den aktuellen
Treffer; in diesem Fallback werden weitere Treffer gezählt, aber nicht gleichzeitig
markiert. Die Auswahl wird dabei nicht verändert.

Syntaxhervorhebung wird dynamisch und nur nahe dem sichtbaren Bereich geladen.
Während Auswahl oder Suche werden wartende Highlight-Aufträge zurückgestellt.
Es laufen keine wiederkehrenden Warte-Timer. Nach einer Textknotenänderung wird der
Suchindex ungültig. Codekopien verwenden den Text vor der Hervorhebung.

Überschriftenpositionen werden bei Layoutänderungen gebündelt gemessen. Beim
Scrollen ermittelt eine binäre Suche den aktuellen Abschnitt; anschließend werden
nur dessen relevante Überschriften gemessen. Reload verwendet Überschrift und Pixelabstand, vorherige
Überschrift und anschließend begrenzten Scrollfortschritt. Spätes Bildlayout
korrigiert nur, solange keine neue Navigation stattgefunden hat.

Präferenzen liegen als validiertes Versionsobjekt in lokalem WebView-Storage.
Gespeichert werden Theme, Zoom, TOC-Sichtbarkeit und höchstens 100 Pfade mit
Lesepositionen. Dokumentinhalte werden nicht dauerhaft gespeichert. Fenstergröße,
Position und Maximierung übernimmt das Window-State-Plugin. Beschädigte oder
nicht schreibbare Einstellungen blockieren den Reader nicht.

## Markdown-Regeln

Marked läuft mit `gfm: true`, `breaks: false`, `async: false` im Worker. Softbreaks
bleiben Zeilenumbrüche im Absatz; zwei abschließende Leerzeichen erzeugen `<br>`.
Der Adapter erhält GFM-Tabellen, Autolinks, Durchstreichung und deaktivierte Tasks.

Überschriften-IDs: Inline-Text → NFKC → Kleinschreibung → nur Unicode-Buchstaben,
Ziffern, Leerraum, Bindestrich und Unterstrich behalten → Leerraum/Unterstriche zu
Bindestrichen. Leere Slugs werden `section`. Das Präfix ist `doc-`, Kollisionen
bekommen aufsteigende Suffixe (`doc-title`, `doc-title-1`). Bereits verwendete
Suffix-Namen werden ebenfalls berücksichtigt. TOC und Dokument teilen dieselben
Metadaten. Es wird keine vollständige GitHub-Kompatibilität behauptet. Häufige
HTML-Entities werden für Titel dekodiert; eigene HTML-IDs werden verworfen.

Der optimierte Tokenizer verwendet weiterhin die Marked-Grammatik. Die Absicherung
vergleicht alle 652 offiziellen CommonMark-0.31.2-Beispiele und die GFM-Fixture mit
der ungeänderten, festgeschriebenen Marked-Version. Das ist ein Gleichheitstest des
Adapters, keine Behauptung vollständiger CommonMark-Konformität des GFM-Modus.

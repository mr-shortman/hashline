# Architektur und Rendering-Vertrag

`App` hält Darstellungs- und Navigationszustand. `DocumentController` koordiniert
Dateien, Revisionen und Watcher. `DocumentGateway` begrenzt den Plattformzugriff;
nur `src/platform/tauri.ts` importiert Tauri-Frontend-APIs. Der Browseradapter ist
eine Entwicklungshilfe und besitzt keine nativen Rechte.

`WorkerMarkdownService` verwendet einen wiederverwendbaren Worker. Ein neuer
Auftrag beendet einen noch beschäftigten Worker; es gibt keine unbeschränkte
Auftragswarteschlange. Der Controller prüft steigende Anfrage-IDs nach Lesen und
Parsing. Veraltete native Dokumenthandles werden freigegeben. Ein fehlgeschlagener
Dateiwechsel erhält Inhalt und Namen der erfolgreich geöffneten Datei.

DOMPurify läuft auf dem Hauptthread mit einer expliziten Element-/Attributliste.
Anschließend werden IDs, Links, Aufgabenlisten und Ressourcen geprüft und
umgeschrieben. Nur `DocumentViewport` setzt den gebrandeten Typ `SanitizedHtml` in
das DOM ein. Das zusammenhängende semantische Dokument liegt außerhalb des
React-Abgleichs: normale UI-Updates und Suche ersetzen seinen Baum nicht.

Die Suche indiziert sichtbare Textknoten je Revision. Geschlossene Details und
Code-Bedienbeschriftungen zählen nicht mit. Eine maskierte literale Regex mit
Unicode-Case-Folding behält die UTF-16-Offsets des Originals bei. Die Treffer liegen
als DOM-Ranges vor. CSS Custom Highlights funktionieren in der geprüften WebKitGTK
2.52.6. Bei fehlender API zeichnet ein klickdurchlässiges Overlay den aktuellen
Treffer; in diesem Fallback werden weitere Treffer gezählt, aber nicht gleichzeitig
markiert. Die Auswahl wird dabei nicht verändert.

Syntaxhervorhebung wird dynamisch und nur nahe dem sichtbaren Bereich geladen.
Während Auswahl oder Suche werden wartende Highlight-Aufträge zurückgestellt.
Es laufen keine wiederkehrenden Warte-Timer. Nach einer Textknotenänderung wird der
Suchindex ungültig. Codekopien verwenden den Text vor der Hervorhebung.

Überschriftenpositionen werden bei Layoutänderungen gebündelt gemessen. Beim
Scrollen ermittelt eine binäre Suche die aktive Überschrift, ohne alle Überschriften
erneut zu messen. Reload verwendet Überschrift und Pixelabstand, vorherige
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

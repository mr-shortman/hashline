# 003 — Begrenztes, verzögertes Syntaxhighlighting

Entscheidung: highlight.js 11.12.0 wird erst für sichtbare bzw. nahe Codeblöcke
geladen. Der Kern liegt in einem separaten Chunk (etwa 20,4 kB unkomprimiert).
Registriert sind JavaScript, TypeScript, JSON, Bash, Python, Rust, CSS, XML/HTML
und YAML; übliche Aliase werden aufgelöst. Es gibt keine automatische Spracherkennung.
Unbekannte Sprachen und Blöcke über 12.000 Zeichen bleiben unkoloriert.

Ein IntersectionObserver und einzelne wartende Tasks verteilen die Arbeit.
Aktive Auswahl und Suche verhindern Textknotenänderungen; deren Ende setzt
Aufträge ereignisbasiert fort. Die Suche kann ihren Index nach Highlighting erneuern.
Das ursprüngliche Code-TextContent wird vor dem Hinzufügen des Kopierbuttons erfasst.

Begründung: Kleiner Startpfad, begrenzte Grammatik-/Speicherlast und einfache
Integration mit semantischem HTML. Die Browser- und WebKit-Funktionstests prüfen
Code-Suche und Auswahl; Clipboard-Kopierinhalt wird im Browser geprüft. Der
Highlighter ist keine Voraussetzung für die erste lesbare Darstellung. Die
vollständige 50-ms-Interaktionsabnahme mit vielen sichtbaren Codeblöcken bleibt
Teil der Performance-Freigabe.

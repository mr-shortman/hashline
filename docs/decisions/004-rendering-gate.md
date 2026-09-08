# 004 — Abschnittsrenderer; Referenzfreigabe bleibt offen

Status: Architekturvergleich umgesetzt und funktional geprüft; keine v1-Freigabe.
Stand: 8. September 2026.

## Ausgangslage

Der bisherige zusammenhängende Renderer brauchte bei 1 MiB im Median 378,5 ms
Bereinigung und 673 ms bis zum Einfügungs-/Layout-Hilfsframe. Bei 10 MiB waren es
3.260 bzw. 6.430 ms; die CLI-Beobachtung dauerte rund 12,7 Sekunden. Diese
synchronen Hauptthread-Phasen überschritten das 50-ms-Budget. Die Messmaschine
verwendet eine diskrete NVIDIA-GPU und ist keine SPEC-Referenz.

## Umgesetzter Vergleich

- Marked löst Referenzdefinitionen weiterhin für das gesamte Dokument im Worker
  auf. Vollständige Markdown-Blöcke werden danach zu ungefähr 16 KiB großen
  HTML-Abschnitten zusammengefasst. Überschriften-IDs bleiben dokumentweit eindeutig.
  Zusätzliche Präfixprüfungen vermeiden aussichtslose Setext-/Tabellen-Regeln in
  JavaScriptCore; alle CommonMark-Vergleichsfälle bleiben unverändert.
- Der Controller übergibt Abschnitte an den Viewport. Jeder Abschnitt wird mit
  DOMPurify und derselben Ressourcen-/Attributpolitik bereinigt und als
  `DocumentFragment` direkt eingefügt. Die bisherige Zwischenserialisierung und
  zweite HTML-Analyse entfallen.
- Der erste Abschnitt beziehungsweise gespeicherte Leseanker wird zuerst
  dargestellt. Hintergrundaufgaben bauen die weiteren Abschnitte mit einem
  angestrebten 18-ms-Zeitfenster auf; zwischen Aufgaben liegt eine echte
  Event-Loop-Grenze über eigene Window-Nachrichten. Dokumentwechsel brechen ausstehende Verarbeitung ab.
  Nicht wiederverwendbare alte Abschnitte werden vor dem Neuaufbau vom Dokumentende her
  aufgabenweise entfernt. Auch diese Phase ist abbrechbar; Sprung- und
  Auswahlaufträge werden bis zum Neuaufbau vorgemerkt. Das synchrone Ablösen
  oder Verstecken des ganzen Renderbaums hatte den Wechsel selbst blockiert.
- Unveränderte, bereits bereinigte Abschnitte behalten ihre verbundenen DOM-Knoten
  und Suchindizes. Der Vergleich erfolgt über den vollständigen HTML-Inhalt;
  wiederholte Abschnitte werden eindeutig zugeordnet. Bildhaltige Abschnitte
  werden wegen dokumentgebundener Ressourcenrechte immer neu aufgebaut.
  Codekopieren nutzt einen delegierten Handler des aktuellen Dokuments.
- Ein Renderer pro Parse und zusätzliche Präfixprüfungen für Inline-Regeln
  reduzieren wiederholte Parserarbeit bei gleicher Markdown-Semantik.
  Wiederkehrende äußere Listen verwenden ihre bereits analysierten Tokenbäume
  innerhalb eines Dokuments weiter (höchstens 64 Einträge, je höchstens 8 KiB
  Quelltext). Quellen mit möglichem HTML und Listen mit Referenz-/Tasksyntax
  bleiben ausgeschlossen. Die normale globale Inline-Auflösung bleibt erhalten;
  veränderlicher äußerer Tokentext wird kopiert. CommonMark wird auch mit
  wiederholten Beispielen gegen das unveränderte Marked geprüft.
- `content-visibility: auto` begrenzt Layout auf relevante Abschnitte. DOM und
  Textknoten bleiben für Auswahl, Semantik und Suche vorhanden. Sprünge bauen
  ihren Zielabschnitt bei Bedarf vorzeitig auf und erzwingen dessen Layout vor
  der Positionsberechnung; dies ist in WebKitGTK notwendig. Leseanker werden
  bei Größenänderungen nachgeführt, solange keine neue Benutzernavigation erfolgt.
- Textindizes entstehen abschnittsweise. Die Suche prüft das gesamte Dokument,
  aber nur sichtbare Treffer und der aktive Treffer werden gezeichnet. Alle
  Treffer auf einmal als CSS Highlights zu registrieren hat in WebKit sonst das
  ausgelassene Layout des ganzen Dokuments erzwungen. Ein laufender Suchaufbau
  ist als solcher sichtbar; er wird nicht als „Keine Treffer“ ausgegeben.
- Der Code-Highlighter beobachtet Abschnittshüllen. Beobachtete Codeblöcke in
  ausgelassenen Teilbäumen würden ebenfalls unnötiges Layout auslösen. Auswahl
  und Such-Ranges werden weiterhin vor Textknotenänderungen geschützt.
- Der Worker wird während Öffnungsserien wiederverwendet; nach 30 Sekunden
  ohne neue Parserarbeit wird sein Lexer-Heap durch Beenden freigegeben.

## Nachweise und Grenzen

Die CommonMark-Verträge vergleichen auch den bereinigten Abschnittspfad mit dem
zusammenhängenden Pfad. Weitere Tests decken globale Referenzlinks, kollidierende
IDs, Inline- und dokumentweite Auswahl, Suchnavigation bis zum letzten Treffer,
frühe Sprungziele, Abbruch und Reload mit geänderten Abschnittsgrenzen ab. Der
native WebKit-Test steht in `tests/desktop/sections.py`; Messreihen, Buildhashes
und Ergebnisse stehen im [Benchmarkbericht](../../benchmarks/REPORT.md).

**16 KiB sind ein Gruppierungsziel, keine harte Obergrenze.** Ein einzelner großer
Absatz, eine Tabelle, Liste oder ein Codeblock bleibt semantisch zusammenhängend.
Rohe HTML-Blöcke dürfen Abschnittsgrenzen nur passieren, wenn ihre Container
nachweislich geschlossen sind. Unsichere Syntax, Raw-Text-Modi oder über Tokens
reichende HTML-Container behalten vorsichtshalber ein Dokumentfragment. Diese Sonderfälle
besitzen noch keine garantiert auf <50 ms begrenzte Bereinigung/Layoutphase.
Der Worker lexiert das gesamte Dokument vor der ersten Ausgabe; echtes Streaming
mit Rückstau ist nicht implementiert. Vollständiger Aufbau und vollständige
Suche sind später verfügbar als die erste lesbare Darstellung.

Die Timer für geplante Renderaufgaben schließen spätere Browser-Layout-/Painttasks
nicht ein. Kleine Aufgaben allein sind daher kein Nachweis für durchgängig
flüssige Darstellung. Auch ausgewählte vollständige Dokumente können zusätzliche
Layoutarbeit verursachen. Die Vergleichsreihe ist gegen die unveränderten SPEC-
Budgets auszuwerten; eine lokale Hilfszeit innerhalb des Budgets ersetzt weder
externe Präsentationsmessungen noch die integrierte-Grafik-Referenz.

Die Architektur ist für den überprüfbaren Entwicklungsstand übernommen. Die
M0-/v1-Freigabesperre bleibt für fehlende Referenz-/Präsentationsnachweise und
verbleibende Budgetüberschreitungen bestehen. Weitere Arbeit muss besonders die
atomaren Sonderfälle und den vollständigen Hintergrundaufbau adressieren.

## Bewertung der Nachmessung

Die erste vollständige Abschnittsserie reduziert die 1-MiB-CLI-Hilfszeit auf
342,7 ms p95 bei 30 Läufen. 10 MiB bleiben mit 2.386,3 ms p95 darüber; vollständiger
Aufbau braucht erheblich länger. Die finale Variante begrenzt zusätzlich die
instrumentierte Entfernung alter Abschnitte, zeigt aber in der lokalen
Speicherreihe 377,22 MiB nach 50 Wechseln (+57,04 % gegenüber warmem Ausgangswert).
Damit ist sie **keine freigegebene Lösung der Performancebudgets**.

Die nächste Architekturprüfung muss den gesamten Lebenszyklus messen: Hauptthread-
und Allokationsprofil beim wiederholten großen Wechsel, Rückhaltung alter DOMs
und Textindizes, Arbeit zwischen Worker-Ergebnis und sichtbarer Darstellung,
Layout großer Einzelblöcke sowie den vollständigen Hintergrundaufbau. Ein
vergleichbar instrumentierter alternativer Aufbau darf Auswahl und Suche nicht
auf nur gerade sichtbare Textknoten beschränken. Danach sind Start-/Präsentations-
und Speicherreihen auf der ungestörten integrierten Referenz zu wiederholen.
Die vorhandenen 60-/120-Hz-Compositor-Traces sind wegen überdeckender paralleler
Testfenster und fehlender App-Frame-Zuordnung keine Scrollfreigabe.

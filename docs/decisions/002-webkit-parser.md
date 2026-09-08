# 002 — Marked-Tokenizer für WebKitGTK begrenzen

Problem und Messung: Im ersten Release brauchte Parsing der gemischten 100-KiB-
Fixture in WebKitGTK im Median 868 ms (30 Läufe). Die 1-MiB-Datei überschritt den
15-Sekunden-Testzeitrahmen. V8 zeigte diesen Engpass nicht. Ein isoliertes Profil
in derselben WebView maß 860 ms, davon etwa 208 ms Blockquotes, 188 ms HTML,
187 ms Trennlinien und 120 ms Listen.

Entscheidung: Der Adapter prüft mögliche Blockanfänge mit kurzen Präfixen, bevor
er Marked-Regeln aufruft. Trennlinien erhalten maximal eine Zeile, Tabellen und
Setext-Überschriften maximal Text bis zur nächsten leeren Zeile. Listen werden nur
an einer leeren Zeile vor einem unindentierten Nicht-Listenblock begrenzt.
Einrückungen, weitere Listeneinträge und die globale Referenzlink-Auflösung bleiben
dem regulären Lexer überlassen. Keine parallele Markdown-Grammatik in Rust.

Isolierte Nachmessung: 35 ms für 100 KiB, 289 ms für 1 MiB. Sämtliche 652
CommonMark-Beispiele sowie GFM- und Referenzlink-Fälle liefern identisches HTML zur
ungeänderten Marked-Version. Die vollständigen Release-Nachmessungen stehen in
`benchmarks/results/webkit.json`; der Ausgangszustand bleibt separat gespeichert.

Auswirkung: Der wiederverwendbare Worker bleibt erhalten. Der Adapter ist klein,
aber bibliotheksabhängig: Bei einem Marked-Upgrade müssen die Gleichheitstests und
WebKit-Messungen erneut laufen. Große zusammenhängende DOMs haben weiterhin ein
separates Hauptthread-Budget; der Parser-Fix beweist dessen Einhaltung nicht.

Referenzen: [Marked-Erweiterungen](https://marked.js.org/using_pro),
[CommonMark-Testdaten](https://spec.commonmark.org/0.31.2/).

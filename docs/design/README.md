# Gestaltungsreferenz

[`styles.css`](styles.css) ist die **verbindliche Gestaltungsreferenz** für den
nativen Renderer (SPEC.md, Abschnitte 3 und 13). Es ist eine Kopie von
`src/app/styles.css` im Stand vor der Migration und bleibt hier erhalten,
nachdem `src/` entfernt ist.

Die Datei ist **Referenz, kein Build-Eingang**: im ausgelieferten Programm gibt
es kein CSS. Die Werte sind in `crates/hashline/src/theme/mod.rs` übernommen
(SPEC.md, Abschnitt 3, „Design-Tokens") und werden nicht zur Laufzeit gelesen.
Wer eine Farbe oder ein Maß ändert, ändert beide Stellen und hält sie
vergleichbar.

Die frühere `titlebar.css` beschrieb die selbst gebaute Titelleiste. Sie ist
entfallen: an ihre Stelle tritt die native `GtkHeaderBar`
([009](../decisions/009-native-renderer.md), Abschnitt 4). Welche Bedienelemente
in welcher Anordnung die Leiste trägt, steht in SPEC.md Abschnitt 3.

## Was es nicht gibt

**Referenzaufnahmen der WebView-Fassung.** Sie sind ohne lauffähige
WebView-Fassung nicht herstellbar und werden nicht nachgeliefert; der letzte
Stand mit vollständiger WebView-Fassung liegt im Tag `webview-final`. Der
visuelle Abgleich läuft stattdessen gegen diese Datei und gegen
`examples/render`, das ein Dokument ohne Fenster in ein PNG setzt
([Prüfungen](../testing.md)).

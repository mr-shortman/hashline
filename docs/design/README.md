# Gestaltungsreferenz

Diese beiden Stylesheets sind die **verbindliche Gestaltungsreferenz** für den
nativen Renderer (SPEC.md, Abschnitte 3 und 13). Sie sind Kopien des Standes
der WebView-Fassung vor der Migration und werden hier erhalten, auch nachdem
`src/` entfernt ist.

| Datei | Herkunft |
| --- | --- |
| [`styles.css`](styles.css) | `src/app/styles.css` |
| [`titlebar.css`](titlebar.css) | `src/ui/titlebar.css` |

Sie sind **Referenz, kein Build-Eingang**: im ausgelieferten Programm gibt es
kein CSS. Die Werte werden in die Rust-Token-Struktur übernommen
(SPEC.md, Abschnitt 3, „Design-Tokens"), nicht zur Laufzeit gelesen.

`titlebar.css` beschreibt die selbst gebaute Titelleiste, die entfällt: an ihre
Stelle tritt die native `GtkHeaderBar`. Die Datei bleibt als Beleg dafür
erhalten, welche Bedienelemente in welcher Anordnung die Leiste trug — das ist
die Anordnung, die die HeaderBar übernimmt
([009](../decisions/009-native-renderer.md), Abschnitt 4).

## Was hier noch fehlt

Die **Referenzaufnahmen** der WebView-Fassung, die SPEC.md Abschnitt 12 für die
visuelle Abnahme verlangt, sind noch nicht erstellt. Sie brauchen eine lauffähige
WebView-Fassung und sind ohne sie nicht mehr herstellbar. Der letzte Stand mit
vollständiger WebView-Fassung ist als Tag `webview-final` erhalten.

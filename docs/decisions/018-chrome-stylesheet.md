# 018 — Die Bedienelemente bekommen ein eigenes Stylesheet

Status: beschlossen. Stand: 10. September 2026.
Bezug: [SPEC.md](../../SPEC.md) Abschnitt 3,
[`docs/design/styles.css`](../design/styles.css),
[015](015-renderer-choice.md).

## Die Frage

Der Dokumentbereich zeichnet sich aus der Tokentabelle in
`crates/hashline/src/theme/mod.rs` und folgt damit dem gewählten Modus. Alles
darum herum — Kopfleiste, Suchleiste, Inhaltsverzeichnis, Menü, Tabs — waren
GTK-Widgets ohne eigene Regeln und nahmen ihre Farben aus dem Systemthema. Die
Frage war, ob das so bleiben kann.

## Entscheidung

**Nein.** Die Bedienelemente werden aus derselben Tokentabelle gesetzt, über
einen `GtkCssProvider` in `crates/hashline/src/theme/chrome.rs`, angemeldet mit
`STYLE_PROVIDER_PRIORITY_APPLICATION`. Benannt wird nur, was Hashline zeigt;
alles Übrige — Fensterknöpfe, Fokusringe, Maße — bleibt beim Systemthema.

## Warum

Ein Systemthema darf in beiden Varianten dunkel sein.
`gtk-application-prefer-dark-theme` ist eine Bitte, keine Zusage: GTK sucht
daraufhin `gtk-dark.css` im Themenverzeichnis, und ob dort etwas Helles steht,
entscheidet das Thema. `Orchis-Dark` liefert unter `gtk.css` und unter
`gtk-dark.css` dieselbe Datei aus. Im hellen Modus stand das helle Dokument
dann in dunkler Umgebung — und das ist nach [SPEC.md](../../SPEC.md)
Abschnitt 3 ein Fehler, keine Gestaltung des Nutzers: die Gestaltungsreferenz
setzt Suchleiste und Inhaltsverzeichnis auf `surface` mit einer Linie in
`border`, in beiden Varianten.

Der Weg über benannte Farben des Themas (`@define-color theme_bg_color …`)
wurde verworfen. Welche Namen ein Thema definiert, ist nicht verabredet, und
dasselbe `Orchis-Dark` zeigt, wie es ausgeht: seine Definitionen verweisen auf
CSS-Variablen, die der Farbparser von GTK nicht kennt, und jedes Laden des
Themas meldet „Expected a valid color“. Regeln auf Knotennamen sind dagegen
festgelegt und gelten für jedes Thema gleich.

Die Priorität ist der Grund, warum das ohne `!important` und ohne besonders
spitze Selektoren auskommt: GTK fragt die Provider von der höchsten Priorität
abwärts und nimmt je Eigenschaft den ersten Treffer. Was hier steht, gewinnt
also; was hier fehlt, kommt weiter vom Thema.

## Folgen

`gtk-application-prefer-dark-theme` wird weiter gesetzt, aber nur bei einer
echten Änderung. Es ist jetzt nur noch ein Hinweis für die Teile, die Hashline
nicht selbst zeichnet — vor allem Dateidialoge. Jedes Schreiben lädt das
gesamte Stylesheet neu, und bei einem fehlerhaften Systemthema kostet das eine
Runde Parserwarnungen; ohne echte Änderung gibt es sie nicht mehr.

Der Preis ist, dass eine Bedienleiste jetzt zwei Quellen hat. Ein Widget, das
in der Referenz eine Farbe trägt und hier keine Regel bekommt, fällt still auf
das Systemthema zurück. Zwei Tests halten das kleine Ende davon fest: dass
jedes Token im Stylesheet vorkommt, und dass die helle und die dunkle Fassung
dieselben Regeln haben.

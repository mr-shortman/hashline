# 004 — M0-Freigabe wegen Hauptthread- und Speicherbudgets zurückhalten

Status: dokumentierte Architekturabweichung, Freigabesperre für v1.

Der funktionsfähige zusammenhängende HTML-Renderer wahrt Auswahl, Suche und
Semantik. Die Release-Messreihe mit dem korrigierten Parser zeigt bei der
1-MiB-Fixture aber Median 378,5 ms Bereinigung und 673 ms Einfügung/Layout bis zum
Hilfsframe. Bei 10 MiB sind es 3.260 ms und 6.430 ms. Während dieser synchronen
Hauptthread-Phasen kann die Oberfläche das 50-ms-Budget nicht halten. Der
10-MiB-Test ist nach ungefähr 12,7 Sekunden statt innerhalb 2 Sekunden lesbar.

Die Entwicklungsmaschine hat eine diskrete NVIDIA-Grafikkarte und entspricht
nicht der in SPEC verlangten integrierten Referenzgrafik. Dennoch sind die
Hauptthread-Überschreitungen deutlich genug, um keine Freigabe zu begründen.

Entscheidung: Der vorhandene Release ist ein überprüfbarer Entwicklungsstand;
M0–M3 werden nicht als abgenommen markiert. Performanceziele werden nicht erhöht.
Der Parseradapter bleibt, weil seine Verbesserung gemessen und gegen den
Rendering-Vertrag abgesichert ist. Der volle HTML-Renderer bleibt bis zu einem
eigenständig getesteten Folgeschritt erhalten; Virtualisierung oder
`content-visibility` werden nicht ungeprüft aktiviert.

Nächster erforderlicher Architekturvergleich: Worker-Ausgabe in vollständigen
semantischen Abschnitten, abbrechbare Bereinigung dieser Abschnitte und
abschnittsweise DOM-Einfügung mit kurzen Hauptthread-Aufgaben. Dabei müssen
global aufgelöste Referenzlinks, Inline-/Blockauswahl, aktive Such-Ranges,
TOC-IDs, Sprungziele außerhalb schon eingefügter Abschnitte und Reload-Anker
erhalten bleiben. Extrem große einzelne Absätze, Tabellen und Codeblöcke sind
separat zu behandeln. Erst nach diesem Nachweis und den ausstehenden
Referenz-/Installationsmessungen kann die v1-Freigabe erfolgen.

Das ist eine offene Abweichung von SPEC §§9–10 und der M0-Reihenfolge. Die
Funktionsumsetzung ersetzt diese Performanceabnahme ausdrücklich nicht.

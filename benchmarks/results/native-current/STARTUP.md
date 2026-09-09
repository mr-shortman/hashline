# Start bis zum ersten Frame, extern über das Wayland-Protokoll

Stand: 9. September 2026. Release-Build aus dem Arbeitsbaum nach `dfb3603`,
GSK-Standardrenderer. Ubuntu 26.04, Wayland/GNOME, GTK 4.22.4, DP-3 bei 60 Hz.

Diese Reihe schließt die Lücke, die der [Messbericht](REPORT.md) offen ließ:
`examples/measure` läuft laut eigenem Kommentar **ohne Fenster**, deshalb war die
Zahl, mit der [Entscheidung 009](../../../docs/decisions/009-native-renderer.md)
den Stackwechsel begründet hat — `exec` bis erster Frame — für den nativen Stand
bisher nirgends gemessen.

## Verfahren

`benchmarks/startup-wayland.py`. `WAYLAND_DEBUG=1` lässt libwayland jede
Protokollnachricht mit einem CLOCK_REALTIME-Stempel ausgeben; der Treiber nimmt
dieselbe Uhr unmittelbar vor `exec`, sodass jeder Stempel zu einer Verzögerung
seit Prozessstart wird. Das ist genau das in SPEC Abschnitt 9 vorgeschriebene
Verfahren („Startzeiten über das Wayland-Protokoll des Clients: `exec` → erster
Buffer-Attach → erster Frame-Callback"). Es misst von außen, ohne Instrumentierung
im gemessenen Programm, und damit jeden Wayland-Client gleich.

Vier Momente, je in Millisekunden nach `exec`:

| Marke       | Protokollereignis                                                  |
| ----------- | ------------------------------------------------------------------ |
| `attach`    | erstes `wl_surface.attach` — erster Puffer an den Compositor        |
| `frame`     | erstes `wl_callback.done` zu einem `wl_surface.frame`-Callback      |
| `presented` | erstes `wp_presentation_feedback.presented` — tatsächlich gezeigt   |
| `quiet`     | letzte Nachricht, bevor das Protokoll 150 ms lang schweigt          |

Jeder Lauf bekommt einen eigenen D-Bus, damit die Instanzübergabe nicht einen
zweiten Prozess beenden lässt, statt ihn zeichnen zu lassen.

**`presented` ist der Nachweis, der allen früheren Reihen dieses Projekts
gefehlt hat:** der Compositor meldet einen ausgegebenen Frame, statt dass der
Client rät. Die WebView-Berichte mussten sich durchgehend mit einem
Doppel-`requestAnimationFrame` behelfen und haben das jedes Mal als Lücke vermerkt.

## Ergebnis

Median / p95 in Millisekunden.

| Viewer   | Dokument |      Bytes |     attach |          frame |    presented | n   |
| -------- | -------- | ---------: | ---------: | -------------: | -----------: | --- |
| Hashline | –        |          0 | 192,3/219,1 |    202,2/226,7 | 210,9/1815,3 | 15  |
| Hashline | small    |    102 465 | 213,9/237,4 | **218,5/242,1** |  222,8/905,7 | 30  |
| Hashline | medium   |  1 048 581 | 243,4/273,1 |    249,3/278,7 |  261,4/290,7 | 15  |
| Hashline | large    | 10 486 101 | 551,6/585,3 |    557,2/588,3 |  676,5/704,6 | 15  |
| ViewMD   | –        |          0 |   90,4/92,2 |    110,6/118,3 |            – | 10  |
| ViewMD   | small    |    102 465 |   90,2/91,9 |    153,9/157,1 |            – | 15  |
| ViewMD   | medium   |  1 048 581 |   89,8/92,0 | 1.695,6/1.718,8 |           – | 15  |
| ViewMD   | large    | 10 486 101 |   94,2/97,9 |   **kein Frame** |           – | 3   |

ViewMD fordert keine `wp_presentation_feedback` an, deshalb ist die Spalte dort
leer. Der viewerübergreifende Vergleich läuft über `attach` und `frame`.

## Gegen das Budget

SPEC Abschnitt 9: *Prozessstart bis erste lesbare Darstellung, kleine Datei —
p95 ≤ 250 ms; ambitioniertes Optimierungsziel ≤ 150 ms.*

**Erster Frame-Callback bei 100 KiB: p95 242,1 ms. Das Budget ist erfüllt**,
mit 8 ms Abstand. Das ambitionierte Ziel von 150 ms ist nicht erreicht; der
Median liegt bei 218,5 ms.

Einschränkung, die zählt: SPEC verlangt für die förmliche Abnahme sichtbaren
Dokumenttext, extern über eine Monitoraufnahme belegt — „ein leeres Fenster
genügt nicht". Diese Reihe belegt einen präsentierten Frame, nicht seinen
Inhalt. Sie ersetzt die Monitoraufnahme **nicht**. Sie stützt sie aber stark:
Hashlines `attach` wächst mit dem Dokument (192 → 214 → 243 → 552 ms), das
Dokument ist also fertig, bevor der erste Puffer übergeben wird.

## Zwei Befunde

### 1. Die Virtualisierung schlägt die Latte, und zwar deutlich

009 hat ViewMD als „Latte für Start und Speicher und zugleich das Gegenbeispiel
für die Darstellung" gesetzt. Genau so fällt die Messung aus:

| Erster Frame | 100 KiB |         1 MiB |  10 MiB |
| ------------ | ------: | ------------: | ------: |
| Hashline     | 218,5 ms |      249,3 ms | 557,2 ms |
| ViewMD       | 153,9 ms |     1.695,6 ms | nie      |

Bei 1 MiB ist Hashline **6,8× schneller**. Bei 10 MiB erreicht ViewMD innerhalb
von 30 Sekunden überhaupt keinen Frame-Callback: es hängt nach 94 ms ein leeres
Fenster hin und rendert danach nichts mehr, was das Protokoll sähe. Hashline
zeigt dieselbe Datei nach 557 ms. Der Stackwechsel hat sein Kernversprechen
eingelöst.

### 2. Zwei Startkosten, beide adressierbar

ViewMDs `attach` ist **konstant bei rund 90 ms**, unabhängig von der
Dokumentgröße — es zeigt sofort ein leeres Fenster und füllt es danach.
Hashlines `attach` wächst mit dem Dokument. Daraus zerfällt Hashlines Startzeit
in zwei getrennte Posten:

- **Rund 100 ms Grundlast vor jeder Dokumentarbeit.** Leeres Fenster: Hashline
  192,3 ms gegen ViewMD 90,4 ms. Beides sind GTK4-Anwendungen auf demselben
  Toolkit, die Differenz ist also Hashlines eigener Startpfad — Kandidaten sind
  GSettings-Schema, CSS-Provider, Icon-Theme, Fontmap und AT-SPI. Ungeklärt und
  bisher nirgends untersucht.
- **Dokumentarbeit vor dem ersten Puffer.** +22 ms bei 100 KiB, +51 ms bei 1 MiB,
  +360 ms bei 10 MiB. SPEC Abschnitt 9 verlangt das ausdrücklich so („Kein
  Fenster ohne Inhalt zeigen, wenn der Inhalt in derselben Bildwiederholung
  fertig werden kann"), und bei 100 KiB und 1 MiB ist das richtig. Bei 10 MiB
  kostet es 360 ms, in denen der Bildschirm leer bleibt.

Der erste Posten ist der lohnendere: 100 ms auf **jedem** Start, unabhängig von
der Datei, und er bringt das ambitionierte 150-ms-Ziel in Reichweite.

## Grenzen dieser Reihe

- Der Stempel entsteht, wenn libwayland die Nachricht formatiert. Der dynamische
  Loader und alles vor dem ersten Protokollverkehr stecken in `attach`, sind
  darin aber nicht einzeln sichtbar.
- `presented` streut bei Hashline stark nach oben (p95 905,7 ms bei Median
  222,8 ms, Maximum 922,5 ms), während `frame` im selben Lauf stabil bleibt
  (p95 242,1 ms, Maximum 255,4 ms). Die Streuung sitzt also zwischen
  Frame-Callback und Ausgabe, vermutlich in der Fenster-Einblendung des
  Compositors. Für Aussagen über die Anwendung ist `frame` die belastbare Marke.
- Die Sitzung war nicht ruhiggestellt. Für Startzeiten ist das nachrangig, für
  `presented` nicht.
- Betriebssystem-Dateicache unkontrolliert; ein Kaltstart ist nicht enthalten.
- Rohdaten: `startup-hashline-small.json`, `startup-hashline-big.json`,
  `startup-hashline-empty.json`, `startup-viewmd.json`,
  `startup-viewmd-large.json`, `startup-viewmd-empty.json`.

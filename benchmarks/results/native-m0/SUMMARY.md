# Scroll-Frametimes, nativer Renderer (M0, Risiko 4)

Referenz: GIGA-BYTE G27FC (DP-3, primär), 1920×1080, Ubuntu 26.04, GNOME/Mutter,
Wayland. Release-Build, GSK-Standardrenderer (`vulkan`). Fixture `large.md`
(10 486 101 Bytes). Reiz: echte Mutter-Zeigereingabe, in der Anwendung ist nichts
instrumentiert. Drei Läufe je Rate, je ein 10-Sekunden-Innenfenster.

| Rate | Budget | Lauf | Frames | im Budget | größte Lücke |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 60 Hz | 16.70 ms | 1 | 598 | **98.16 %** | 33.4 ms |
| 60 Hz | 16.70 ms | 2 | 601 | **97.17 %** | 16.8 ms |
| 60 Hz | 16.70 ms | 3 | 601 | **98.34 %** | 16.8 ms |
| 120 Hz | 8.37 ms | 1 | 1184 | **96.11 %** | 25.0 ms |
| 120 Hz | 8.37 ms | 2 | 1194 | **96.57 %** | 16.7 ms |
| 120 Hz | 8.37 ms | 3 | 1165 | **95.02 %** | 16.7 ms |

## Befund

SPEC.md Abschnitt 9 verlangt **mindestens 99 %** der Frames im Refresh-Budget
über 10 Sekunden und keinen Stillstand über 50 ms.

- **Der Frame-Anteil wird verfehlt**, bei 120 Hz deutlicher als bei 60 Hz.
- **Kein Stillstand über 50 ms**: die größte Lücke liegt bei 33,4 ms.

Einschränkung, die das Werkzeug selbst in jede Ausgabe schreibt: gemessen sind
Mutters Präsentationsmarken für den **Monitor**, nicht Frames der Anwendung. Der
Wert schließt alle Clients ein und ist damit eine Untergrenze für die Qualität der
Anwendung, keine Abnahme ihrer Darstellung. Die Sitzung war nicht ruhiggestellt.

Die Rohaufnahmen (`.syscap`, 15–30 MiB je Lauf) und ihre Textausgaben sind nicht
eingecheckt. Der Ablauf zum Wiederholen steht in `docs/testing.md`; er stellt die
Bildwiederholrate des primären Monitors vorübergehend um.

**Offen:** derselbe Lauf mit `GSK_RENDERER=cairo`. Das leere Fenster kostet dort
27 MiB statt 73 MiB PSS, aber cairo zeichnet auf der CPU — ohne diesen Vergleich
bleibt die Renderer-Wahl aus 011 Abschnitt 4 unentschieden.

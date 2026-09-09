# 017 — GtkNotebook für Tabs

Status: beschlossen. Stand: 9. September 2026.
Bezug: [014](014-competitive-targets.md) Abschnitte 2.2 und 3.2,
[SPEC.md](../../SPEC.md) Abschnitte 2, 3 und 9.

## Die Frage

[014](014-competitive-targets.md) holt Tabs in den Umfang und lässt die Wahl
zwischen `GtkNotebook` und `AdwTabView` offen. Beide zeigen mehrere Dokumente
in einem Fenster; sie unterscheiden sich in dem, was sie mitbringen.

## Entscheidung

**`GtkNotebook`.** Die Tableiste bleibt verborgen, solange nur ein Dokument
offen ist, damit eine einzelne Datei genauso aussieht wie vor den Tabs.

## Warum

`AdwTabView` ist die schönere Oberfläche: Übersicht, Anheften, Ziehen zwischen
Fenstern. Zwei der drei sind [014](014-competitive-targets.md) Abschnitt 2.2
ausdrücklich nicht im Umfang, und das dritte braucht niemand für einen
Betrachter, der Dateien nur öffnet und schließt.

Dafür kostet es libadwaita. Das ist eine neue Laufzeitabhängigkeit für ein
Programm, das heute mit GTK 4 auskommt, und sie zieht Stylemanager, eigene
Fensterklassen und ein eigenes Farbschema nach sich — genau die Schicht, die
[SPEC.md](../../SPEC.md) Abschnitt 3 nicht will, weil Hashline seine
Darstellung selbst setzt. Der Speicherboden ist die zweite Rechnung: das leere
Fenster liegt bei 33 MiB gegen ein Ziel von 40 MiB für die kleine Datei
([014](014-competitive-targets.md), Abschnitt 3.2), und dieses Ziel ist heute
schon knapp verfehlt. Eine weitere Bibliothek in den Boden zu legen, um eine
Tableiste hübscher zu machen, ist an dieser Stelle die falsche Reihenfolge.

`GtkNotebook` bringt außerdem mit, was gebraucht wird: eine Seite je Dokument,
`Ctrl+Page_Down`/`Ctrl+Page_Up` von Haus aus, eine scrollbare Leiste und ein
frei bestimmbares Tab-Handle, in dem Name und Schließen-Knopf stehen.

## Was ein Tab besitzt

Das Fenster besitzt Kopfleiste, Suchfeld, Inhaltsverzeichnis und Menü. Ein Tab
besitzt alles, was zum Dokument gehört: Ansicht, Datei, Beobachter, Digest,
Ladeanforderung, Leseposition und die eigene Suche
([014](014-competitive-targets.md), Abschnitt 2.2). Beim Wechsel wird der
Suchtext des verlassenen Tabs vom gemeinsamen Feld genommen und der des
angekommenen dort eingesetzt; genommen wird er erst beim Wechsel, weil
`GtkSearchEntry` eine Änderung mit eigener Verzögerung meldet und diese Meldung
nach dem Wechsel eintreffen kann.

Ein Tab, der nicht vorn ist, hält **keinen Layout-Cache**: gesetzte Blöcke,
Syntaxfarben und dekodierte Bilder werden freigegeben. Op-Buffer, Blockplan mit
seinen gemessenen Höhen und der Leseanker bleiben — das ist es, was das
Zurückkommen zu wenigen Millisekunden macht.

## Gemessen

Ein Prozess je Zeile, `GSK_RENDERER=cairo`, Fixture `medium.md` (1 MiB):

| Offene Tabs | PSS |
| ---: | ---: |
| 1 | 42,8 MiB |
| 2 | 44,7 MiB |
| 10 | 59,9 MiB |

Ein inaktiver Tab kostet damit 1,9 MiB gegen erlaubte 3 MiB, und zehn Tabs
59,9 MiB gegen erlaubte 110 MiB.

## Was sich am Verhalten ändert

- Jede übergebene Datei öffnet einen eigenen Tab; eine bereits offene Datei
  wird nach vorn geholt statt erneut gelesen. Die Meldung „Es kann nur eine
  Datei geöffnet sein“ entfällt.
- Ein weiterer Programmaufruf öffnet einen Tab im bestehenden Fenster.
- `Ctrl+W` schließt den Tab vorn, `Ctrl+Tab` und `Ctrl+Page_Down` wechseln
  vorwärts, `Ctrl+Shift+Tab` und `Ctrl+Page_Up` rückwärts.
- Ein Link auf eine andere Markdown-Datei öffnet ebenfalls einen Tab.
- Der Zoom gilt für das Fenster, nicht für den Tab, damit ein Wechsel nie die
  Schriftgröße ändert.

## Nicht enthalten

Wiederherstellung nach Neustart, Tab-Gruppen, Ziehen zwischen Fenstern und ein
Dateibaum, wie [014](014-competitive-targets.md) Abschnitt 2.2 es festlegt.

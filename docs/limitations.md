# Bekannte Einschränkungen

Der native Reader und die Linux-Paketierung sind implementiert. Der Stand ist
noch keine v1-Freigabe nach SPEC.md. Diese Liste fasst offene Befunde aus
[011](decisions/011-m0-foundation.md),
[012](decisions/012-rendering-and-navigation.md) und dem
[nativen Benchmarkbericht](../benchmarks/results/native-current/REPORT.md)
zusammen; historische Aussagen dort sind keine erneute Messung dieses Pakets.
Was [013](decisions/013-oversized-blocks.md) daran geändert hat, ist unten
jeweils vermerkt.

- **Breite Tabellen** blockieren den Hauptthread weiterhin: 53 ms für
  `wide-table.md` gegen ein 16-ms-Budget. Große Codeblöcke und sehr lange
  Zeilen zerfallen seit 013 in Teilblöcke und tun das nicht mehr — aus 67
  Sekunden werden 1,6 ms und aus 172 ms werden 8 ms. Eine Tabelle zerfällt
  nicht, weil ihre Teile Zeilen sind und der Plan über Textbereiche adressiert.
- **Ein Absatz über 4 KiB wird geschnitten**, und die letzte Zeile jedes Teils
  endet dort, wo der Teil endet, statt an der Spalte. Sichtbar nur bei
  Absätzen, die höher als ein Schirm sind.
- **Die Höhenschätzung von Fließtext liegt rund 32 % zu niedrig**, weil sie
  mehr Zeichen in eine Zeile rechnet, als eine Zeile mit ausgefranstem rechtem
  Rand fasst. Die Bildlaufleiste ist dadurch zu kurz, nie zu lang, und
  korrigiert sich beim Lesen. Für Codeblöcke ist die Schätzung seit 013 auf
  0,2 % genau.
- Das Inhaltsverzeichnis erzeugt alle Überschriftenzeilen. Bei 10 MiB
  überschreitet der gemessene Speicherverbrauch das vorgesehene Budget;
  auch die Scroll- und Klein-Datei-Speicherziele sind noch nicht belegt.
- **Der Standard-GSK-Renderer kostet den größten Einzelposten des
  Speicherbodens.** Im leeren Fenster stehen rund 100 MiB gegen 74 MiB mit
  `GSK_RENDERER=gl` und 33 MiB mit `GSK_RENDERER=cairo`. Umgestellt ist
  nichts: die Frametimes der anderen Renderer sind unbekannt, und der
  Frameanteil beim Scrollen ist selbst ein verfehltes Budget.
- Die Suche wartet bereits 120 ms auf weitere Eingabe; das 100-ms-Ziel für
  mittlere Dateien ist damit noch nicht erfüllbar. Aufweitendes Unicode-Folding
  (`ß`/`ss`, `ﬁ`/`fi`) fehlt.
- Überbreite Tabellen und Codeblöcke sind beschnitten; internes horizontales
  Scrollen fehlt. Fußnoten und Definitionslisten haben keine eigene Gestaltung.
- Bilder im Fließtext zeigen Alternativtext. Remote-Bilder laden nicht;
  rohes HTML einschließlich `details`/`summary` bleibt Quelltext.
- Die Entscheidung für einen sandboxenden Bilddecoder ist offen. Der jetzige
  Decoder verwendet gdk-pixbuf mit Verzeichnis- und Pixelgrenzen.
- Vollständiger visueller Vergleich, Orca-Bedienung und alle regulären
  Performancebudgets mit n=30 auf der vorgesehenen Referenzhardware sind
  noch nicht abgenommen. Bisherige Messungen stammen von Ubuntu 26.04;
  sie ersetzen keine Bestätigung der geforderten integrierten Grafik.

Der [M3-Bericht](acceptance/M3.md) unterscheidet Paketprüfungen von diesen
noch offenen Freigabekriterien.

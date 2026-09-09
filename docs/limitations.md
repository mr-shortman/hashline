# Bekannte Einschränkungen

Der native Reader und die Linux-Paketierung sind implementiert. Der Stand ist
noch keine v1-Freigabe nach SPEC.md. Diese Liste fasst offene Befunde aus
[011](decisions/011-m0-foundation.md),
[012](decisions/012-rendering-and-navigation.md) und dem
[nativen Benchmarkbericht](../benchmarks/results/native-current/REPORT.md)
zusammen; historische Aussagen dort sind keine erneute Messung dieses Pakets.

- Große einzelne Codeblöcke, extrem lange Zeilen und breite Tabellen können
  den Hauptthread erheblich blockieren. Die letzte native Messreihe zeigt
  bis zu 67 Sekunden bei einem Codeblock mit rund 70.000 Zeilen.
- Das Inhaltsverzeichnis erzeugt alle Überschriftenzeilen. Bei 10 MiB
  überschreitet der gemessene Speicherverbrauch das vorgesehene Budget;
  auch die Scroll- und Klein-Datei-Speicherziele sind noch nicht belegt.
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

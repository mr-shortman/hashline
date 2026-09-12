# Bekannte Einschränkungen

Der native Reader und die Linux-Paketierung sind implementiert. Der Stand ist
noch keine v1-Freigabe nach SPEC.md. Diese Liste fasst offene Befunde aus
[011](decisions/011-m0-foundation.md),
[012](decisions/012-rendering-and-navigation.md) und dem
[nativen Benchmarkbericht](../benchmarks/results/native-2026-09-09/REPORT.md)
zusammen; historische Aussagen im
[älteren Bericht](../benchmarks/results/native-current/REPORT.md) sind keine
erneute Messung dieses Pakets.

- **Ein Absatz über 2 KiB wird geschnitten**, und die letzte Zeile jedes Teils
  endet dort, wo der Teil endet, statt an der Spalte. Sichtbar nur bei
  Absätzen, die höher als ein Schirm sind.
- **Überbreite Tabellen setzen nur die Spalten, die auf den Schirm passen.**
  Die abgeschnittenen Zellen werden nicht gesetzt und sind deshalb auch nicht
  anklickbar; ihr Text bleibt im Dokument, wird also mitkopiert und mitgesucht.
  Sobald es waagerechtes Scrollen gibt, muss das zurück.
- **Die Höhenschätzung von Fließtext liegt rund 32 % zu niedrig**, weil sie
  mehr Zeichen in eine Zeile rechnet, als eine Zeile mit ausgefranstem rechtem
  Rand fasst. Die Bildlaufleiste ist dadurch zu kurz, nie zu lang, und
  korrigiert sich beim Lesen. Für Codeblöcke ist die Schätzung seit 013 auf
  0,2 % genau.
- **Der Speicher der kleinen Datei liegt bei 40,8 MiB gegen ein Ziel von
  40 MiB.** 1,5 MiB davon sind die CJK-Schrift, die `small.md` mit „Grüße
  日本語“ in jedem Abschnitt anfordert; ersetzt man dieses eine Wort, sind es
  38,9 MiB. Der Rest des Abstands ist nicht lokalisiert.
- **Der Frameanteil beim Scrollen ist unter `cairo` nicht gemessen.** Die
  einzige Stichprobe aus [015](decisions/015-renderer-choice.md) nennt 93,2 %
  gegen 98,3 % unter Vulkan, beides unter einem Ziel von 99 %. Die Reihe mit
  n = 30 bei 60 und 120 Hz steht aus und braucht eine ruhige Sitzung.
- Aufweitendes Unicode-Folding (`ß`/`ss`, `ﬁ`/`fi`) fehlt in der Suche.
- Überbreite Tabellen und Codeblöcke sind beschnitten; internes horizontales
  Scrollen fehlt. Fußnoten und Definitionslisten haben keine eigene Gestaltung.
- Bilder im Fließtext zeigen Alternativtext. Remote-Bilder laden nicht;
  rohes HTML einschließlich `details`/`summary` bleibt Quelltext.
- Die Entscheidung für einen sandboxenden Bilddecoder ist offen. Der jetzige
  Decoder verwendet gdk-pixbuf mit Verzeichnis- und Pixelgrenzen.
- **Start bis lesbarer Text verfehlt bei 100 KiB das Ziel**: p95 172,0 ms gegen
  120 ms, gemessen in `nested-headless` unter `cairo`, n = 30. 1 MiB (139,2 ms)
  und 10 MiB (205,9 ms) halten ihr Ziel. Die kleinste Datei ist dabei die
  langsamste, weil ihr Dokument vor dem ersten Schlag des Frame-Takts fertig ist
  und das erste Bild deshalb 28 ms Schriftschnitte am Stück zahlt; bei 1 MiB
  liegt es knapp dahinter und der Wert fällt um rund 34 ms
  ([014 §9](decisions/014-competitive-targets.md#9-start-bis-lesbarer-text-wo-die-zeit-wirklich-lag)).
  Der Wert bei 1 MiB steht damit dicht an dieser Klippe.
- **Das erste GTK-4-Programm einer Sitzung wartet auf das Einstellungsportal.**
  GTK fragt es beim Öffnen des Displays synchron nach seiner Version; läuft der
  Dienst noch nicht, sind das 163 ms, die der Betrachter nicht abkürzen kann.
- **Die übrigen Budgets, deren Nachweis ein Einzelbild des Monitors ist, fehlen
  weiterhin**: Öffnen in laufender Instanz, Suche und Menü öffnen, Tabwechsel.
  Der Nachweis verlangt eine unbeaufsichtigte Sitzung, in der das Fenster des
  Betrachters allein auf dem Schirm steht; auf einer Arbeitssitzung misst er
  das, was sonst noch offen ist.
- Vollständiger visueller Vergleich, Orca-Bedienung und alle regulären
  Performancebudgets mit n=30 auf der vorgesehenen Referenzhardware sind
  noch nicht abgenommen. Bisherige Messungen stammen von Ubuntu 26.04;
  sie ersetzen keine Bestätigung der geforderten integrierten Grafik.
- Tabs stellen sich nach einem Neustart nicht wieder her, lassen sich nicht
  gruppieren und nicht zwischen Fenstern ziehen; einen Dateibaum gibt es
  nicht ([017](decisions/017-tabs.md)).

Der [M3-Bericht](acceptance/M3.md) unterscheidet Paketprüfungen von diesen
noch offenen Freigabekriterien.

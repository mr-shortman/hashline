# Architektur und Rendering-Vertrag

Ein einziger nativer Prozess. Die verbindliche Beschreibung steht in
[SPEC.md](../SPEC.md), Abschnitt 5; die Modulkarte in
[development.md](development.md). Dieses Dokument hält den Datenfluss fest, weil
er die eine Sache ist, die man beim Lesen des Codes zuerst braucht.

Die frühere Fassung dieses Dokuments beschrieb den WebView-Aufbau mit
`DocumentGateway`, WASM-Parser und DOM-Replay. Sie ist mit
[009](decisions/009-native-renderer.md) gegenstandslos.

## Vom Dateipfad zum Pixel

1. **Lesen und parsen** geschehen auf einem Arbeitsthread. Jeder Auftrag erhält
   eine steigende `RequestId`; nur die neueste darf das Dokument ersetzen, und
   ein Digest des Quelltexts verhindert, dass eine Änderung ohne Inhalt ein
   Neurendern kostet.
2. **`crates/markdown`** liest das Dokument einmal und erzeugt den Op-Buffer:
   Operationen, zwei Blobs (`strings` und `text`), Blöcke, Abschnitte und
   Überschriften. Es entsteht **kein HTML** — rohes HTML wird als Quelltext
   dargestellt (SPEC.md, Abschnitt 6).
3. **Der Blockplan** entsteht in einem Durchgang aus `blocks` und schätzt jede
   Blockhöhe. Er ist die einzige Struktur, die über das ganze Dokument
   existiert. Ein Block, der viel höher als ein Schirm ist — ein Codeblock mit
   70.000 Zeilen, ein Absatz aus einer Million Zeichen — zerfällt dabei in
   Teile, damit die Virtualisierung in ihn hineinreicht
   ([013](decisions/013-oversized-blocks.md)).
4. **Gesetzt** wird nur der Sichtbereich plus ein Bildschirmpuffer. Eine
   gemessene Höhe ersetzt die Schätzung; liegt der Block über der Leseposition,
   wandert der Scrolloffset um denselben Betrag mit, damit der sichtbare Text
   stillsteht.
5. **Gezeichnet** wird in einem zweiten Durchgang als GSK-Knoten: Dekoration,
   dann Suchtreffer, dann die Auswahl, dann der Text.

## Die eine Zusage, an der alles hängt

Ein Block wird als mehrere *Pieces* gesetzt — eines je Listeneintrag, eines je
Tabellenzelle. Jedes Piece trägt eine `TextMap`: die Läufe, die es aus dem
Dokument übernommen hat. Eingefügte Dekoration — ein Aufzählungszeichen, ein
Bedienelement — hat keinen Lauf und damit keine Dokumentposition.

Deshalb bedeutet jede Position in gesetztem Text weiterhin genau ein Byte im
Dokument. Auswahl über Blockgrenzen, Treffertest, Suchmarkierung,
Linkzuordnung, Syntaxfarben und die Textschnittstelle für AT-SPI gehen alle
durch diese eine Abbildung. Wer sie umgeht, bricht sie —
[012](decisions/012-rendering-and-navigation.md), Abschnitt 1.

## Grenzen

`crates/markdown` kennt weder GTK noch Dateisystem. `layout` kennt keine
Dateien und keinen Anwendungszustand — es fragt über `ImageSource` nach der
Größe eines Bildes, statt selbst zu lesen. `view` ist der einzige Eigentümer des
Layout-Caches. Kein Plugin-System, kein globaler Event-Bus, keine Datenbank.

# 013 — Übergroße Blöcke, Schätzung, Cachegrenzen und der Speicherboden

Status: Umgesetzt. Stand: 9. September 2026.
Vorgänger: [011-m0-foundation.md](011-m0-foundation.md),
[012-rendering-and-navigation.md](012-rendering-and-navigation.md).
Bezug: [SPEC.md](../../SPEC.md), Abschnitte 5, 9 und 10.
Messgrundlage: [nativer Benchmarkbericht](../../benchmarks/results/native-current/REPORT.md),
Ergebnisse dieser Arbeit unter
[`native-oversized/`](../../benchmarks/results/native-oversized/REPORT.md).

Vier Befunde aus der letzten Messreihe. Sie hängen zusammen: drei davon haben
dieselbe Ursache, und der vierte ist der Grund, warum der zweitgrößte
Speicherposten bisher nicht nachgeprüft werden konnte.

## 1. Der Blockplan reicht jetzt in große Blöcke hinein

Der Plan setzt einen Block in einem Stück. Das trägt genau so lange, wie ein
Block ungefähr schirmgroß ist. Zwei Fixtures halten sich nicht daran, und beide
sind keine Kuriosität, sondern die Sorte Datei, die ein Leser tatsächlich
geschickt bekommt:

- `large-code.md` ist **ein** eingezäunter Codeblock mit 70.004 Zeilen. Ein
  Codeblock war genau ein Block des Plans, also half Virtualisierung nicht: um
  irgendetwas zu zeichnen, musste er ganz gesetzt werden. Am laufenden Programm
  gemessen: 66,85 Sekunden bei 100 % CPU, mit dem Vorher-Build dieser Arbeit
  reproduziert als 67.140 ms. Jetzt: **1,5 ms**, weil der erste Schirm zwei von
  275 Teilblöcken setzt.
- `long-line.md` ist **ein** Absatz aus einer Million Zeichen ohne ein einziges
  Leerzeichen. 172 ms Setzzeit — und, was der Bericht in seiner Zeittabelle
  nicht zeigt, 171 MiB PSS für eine 1-MiB-Datei, mehr als die 1-MiB-Datei
  `medium.md` mit 104 MiB. Ein einzelnes Pango-Layout über eine Million Zeichen
  hält Glyphenläufe und Log-Attribute für den gesamten Text. Jetzt: **8,3 ms**
  und **91 MiB**, also 3 MiB über dem leeren Fenster statt 88 MiB darüber.

Der Preis ist derselbe, ob man ihn in Millisekunden oder in Megabyte abliest.
011 hat für die lange Zeile drei Auswege genannt — Umbruch erzwingen, in
synthetische Teilblöcke schneiden, oder abschneiden. Abschneiden verliert
Dokumentinhalt, und Umbruch erzwingen hilft dem Codeblock nicht. Geschnitten
wird also.

**Ein zu großer Block zerfällt beim Bau des Plans in Teile, und ein Teil ist
ein gewöhnlicher Block des Plans**: er wird geschätzt, gemessen, zwischengespeichert
und verdrängt wie jeder andere. Ein Teil kennt seine Nummer und die Zahl der
Teile seines Quellblocks; alles, was den ganzen Block meint statt eines Teils —
das Kopieren eines Codeblocks, der Dreifachklick, die Abstände darum, das
Inhaltsverzeichnis, die Vorlesereihenfolge — geht über diese beiden Zahlen.

Die Teile teilen sich die Operationen ihres Quellblocks und **beschneiden jeden
Textlauf auf den eigenen Bereich**. Das ist die ganze Mechanik: für einen nicht
geschnittenen Block ist das Beschneiden wirkungslos, weil sein Bereich ohnehin
jeden Lauf umfasst.

### Wo geschnitten wird, und warum dort

Die Grenzen sind gemessen, nicht geraten. Setzzeit eines Blocks, Release-Build,
Entwicklungsmaschine, Median aus fünf Läufen:

| Codezeilen | 64 | 128 | 256 | 512 | 1024 | 2048 | 4096 | 8192 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ms | 0,2 | 0,5 | 1,5 | 4,8 | 17,1 | 62,3 | 236,2 | 928,4 |

| Text ohne Leerzeichen, Byte | 2048 | 4096 | 8192 | 16384 | 32768 | 65536 | 131072 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| ms | 2,6 | 8,2 | 28,5 | 102,0 | 390,0 | 1554,1 | 6119,6 |

Beide Kurven vervierfachen sich pro Verdopplung: der Aufwand wächst mit dem
**Quadrat** der Länge. Bei 70.004 Zeilen ergibt die erste Kurve 68 Sekunden,
was die 66,85 gemessenen erklärt. Fließtext *mit* Leerzeichen ist dagegen
annähernd linear (2 ms bei 8 KiB, 56 ms bei 128 KiB): teuer ist die
Umbruchsuche in einem Lauf ohne jede Umbruchgelegenheit.

Daraus die Grenzen, jede mit reichlich Abstand zum 16-ms-Budget aus SPEC
Abschnitt 9, und jede so hoch wie möglich, damit kein gewöhnliches Dokument
überhaupt geschnitten wird:

- **Text: 4096 Byte.** Das ist bereits mehr als ein Schirm der Lesespalte.
  Kostet rund 1 ms Fließtext und 8 ms im Extremfall ohne Leerzeichen.
- **Code: 256 Zeilen oder 32 KiB**, was zuerst eintritt. Rund 1 ms für die
  Zeilenzahl, rund 10 ms für die Bytes; die Bytegrenze fängt den Block ab,
  dessen wenige Zeilen sehr lang sind.

Geschnitten wird **zwischen Zeilen** bei Code und **an einer Wortgrenze** bei
Text, sofern es im letzten Viertel vor der Grenze eine gibt. Code setzt danach
Zeile für Zeile exakt so wie im ganzen Block; das ist geprüft, indem ein
geschnittener Block gegen denselben ungeschnittenen gesetzt und Text und Höhe
verglichen werden. Bei Text bleibt **ein sichtbarer Unterschied**: die letzte
Zeile eines Teils endet dort, wo der Teil endet, nicht dort, wo die Spalte
endet. Das betrifft nur Absätze über 4 KiB und ist der Preis dafür, überhaupt
in sie hineinzureichen.

Die Grenzen sind **Bytezahlen, keine geschätzten Zeilen**. Damit hängen die
Schnittstellen nicht an Spaltenbreite, Zoom oder Schriftart, und die
Blocknummern bleiben über einen Neuumbruch hinweg dieselben — worauf sich alles
verlässt, was einen Block über seine Nummer merkt, von den zwischengespeicherten
Syntaxfarben bis zur Leseposition.

**Listen und Tabellen werden nicht geschnitten.** Ihre Teile sind Einträge und
Zeilen, keine Textstrecken, und der Plan adressiert über Textbereiche. Die
breite Tabelle bleibt damit bei 53 ms über dem Budget; siehe
[Einschränkungen](../limitations.md).

## 2. Die Höhenschätzung eines Codeblocks

Direkt daneben lag ein Fehler, der dieselbe Fixture betrifft. Der Kommentar
über der Schätzung sagte, die Höhe eines Codeblocks folge der Zahl seiner
Zeilenumbrüche — der Code nahm konstant **eine** Zeile an. `BlockPlan::new`
hatte das Dokument und damit den Text; `estimate` bekam ihn nur nicht.

Der Plan zählt die Zeilen eines Codeblocks jetzt beim Bau und legt sie im Block
ab, damit auch ein Neuumbruch ohne das Dokument auskommt. Die Schätzung
verwendet außerdem die tatsächliche Codezeilenhöhe und die Polsterung der
Fläche statt der Fließtextzeilenhöhe, und Abstand über und unter einem Block
kommt für Schätzung wie fertiges Layout aus **derselben** Funktion — vorher
waren es zwei Listen, die sich bei Code und Tabelle widersprachen.

Geschätzte gegen gemessene Gesamthöhe, alle Blöcke einmal gesetzt
(`examples/measure --geometry`):

| Fixture | vorher | nachher | nur Codeblöcke, vorher | nachher |
| --- | ---: | ---: | ---: | ---: |
| `small.md` | −46,6 % | −32,0 % | −62,6 % | −0,2 % |
| `medium.md` | −46,6 % | −32,0 % | −62,6 % | −0,2 % |
| `large-code.md` | −99,98 % | −0,0 % | −100,0 % | −0,0 % |

Die Scrollgeometrie von `large-code.md` war um den Faktor 28.000 daneben und
ließ sich nur durch das Setzen korrigieren, das 67 Sekunden dauerte — die
beiden Befunde verstärkten sich gegenseitig, und beide sind weg.

**Offen bleibt die Schätzung von Fließtext**: −32 % über alle Blöcke. Die
Schätzung teilt die Bytezahl durch die mittlere Zeichenbreite und rechnet
dadurch mehr Zeichen in eine Zeile, als eine Zeile mit ausgefranstem rechten
Rand fasst. Das ist eine eigene, zu belegende Kalibrierung und nicht Teil
dieser Arbeit; die Richtung ist konservativ — die Bildlaufleiste ist zu kurz,
nie zu lang.

## 3. Beide Blockcaches sind jetzt begrenzt

Die gesetzten Blöcke wurden bei 240 Einträgen verdrängt. Die Syntaxfarben
daneben nicht: `highlights` und `requested` wurden **ausschließlich** beim
Dokumentwechsel geleert. Wer in `large.md` durch 28.930 Codeblöcke scrollt,
sammelte deren Spans dauerhaft an. Der 50-Wechsel-Stabilitätstest sieht das
nicht, weil jeder Wechsel leert; eine lange Lesesitzung schon.

Aus den zwei Sammlungen ist eine geworden: `highlights` bildet den Block auf
`Option<Vec<Span>>` ab, wobei `None` „angefragt, Worker läuft noch" heißt. Ein
Eintrag zu verdrängen und das Anfragen weiterhin zu unterdrücken wäre der
schlimmste Fall gewesen — ein Codeblock, der nie wieder Farbe bekommt.
Verdrängt wird bei 480 Einträgen; ein währenddessen eintreffendes Ergebnis
wird verworfen statt zurückgelegt, und der Block fragt beim nächsten Zeichnen
erneut.

Ein geschnittener Codeblock wird **teilweise** eingefärbt, Teil für Teil. Das
ist der Sinn der Sache: ihn als Ganzes einzufärben kostet, was ihn als Ganzes
zu setzen kostet. Eine Zeichenkette oder ein Kommentar über eine Schnittstelle
hinweg wird eingefärbt, als begänne er dort.

## 4. Der GSK-Renderer, und warum er nicht nachprüfbar war

Der Bericht erwähnt am Ende, dass `GSK_RENDERER=cairo` im leeren Fenster 27
statt 75 MiB kostet. Das sind 48 MiB auf dem Boden, gegen ein 80-MiB-Budget für
kleine Dateien, das derzeit mit 84 MiB knapp verfehlt wird — der zweitgrößte
Speicherhebel nach dem Inhaltsverzeichnis, als Nebensatz vermerkt.

Nachprüfen ließ er sich nicht, und daran war die Werkzeugkiste schuld:

- Hashline ist Einzelinstanz. Die installierte laufende Instanz hält den
  Busnamen, also übergibt ein erneuter Start die Datei an *jenes* Fenster und
  misst nichts.
- `dbus-run-session` löst das und schafft ein schlimmeres Problem: der private
  Bus ist Elternprozess von allem, was er aktiviert, also landen Portal-Stack,
  gvfs und dconf in der gemessenen Prozessgruppe und bringen rund 60 MiB mit,
  die nichts mit dem Reader zu tun haben.

`benchmarks/memory.py` startet den privaten Bus deshalb **neben** der Anwendung
statt um sie herum. Was der Bus aktiviert, ist Kind des Busses; die Prozessgruppe
der Anwendung enthält die Anwendung. Portale, gvfs und dconf werden zusätzlich
über die Umgebung ganz vom Bus ferngehalten, damit sie nicht erst gestartet und
dann übersehen werden. Jede Ausgabezeile führt die Prozessgruppe des Busses
mit und ein Feld, das bestätigt, dass die beiden Gruppen disjunkt sind — die
Isolierung ist damit prüfbar statt geglaubt.

Gemessen, leeres Fenster, derselbe Build und dieselbe Sitzung: **88 MiB** mit
dem Standardrenderer (`vulkan`), **74 MiB** mit `gl`, **33 MiB** mit `cairo`.
Der Nebensatz aus dem alten Bericht — 27 statt 75 MiB — ist damit der Richtung
nach bestätigt und der Höhe nach genauer bekannt: es geht um 55 MiB gegen ein
Budget, das mit 96,6 MiB um 16,6 MiB verfehlt wird. Hinzu kommt, dass die beiden
GPU-Renderer nach oben streuen — `gl` misst mit `small.md` in drei Durchgängen
80,9 / 134,8 / 137,4 MiB —, während `cairo` über alle Fixtures um weniger als
0,5 MiB schwankt.

Der Standardrenderer wird hier trotzdem **nicht** umgestellt: `cairo` rendert auf der CPU,
und der Frameanteil beim Scrollen ist ohnehin das zweite verfehlte Budget.
Eine Umstellung braucht Frametimes beider Renderer auf einer ruhiggestellten
Sitzung, und die hat noch niemand.

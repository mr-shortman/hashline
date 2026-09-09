# 014 — Wettbewerbsziel, erweiterter Umfang und zweiteilige Messsuite

Status: Umfang beschlossen und umgesetzt, Zielwerte teils belegt. Stand:
9. September 2026; siehe [Abschnitt 8](#8-stand-der-umsetzung).
Bezug: [SPEC.md](../../SPEC.md) Abschnitte 2, 9 und 12,
[009-native-renderer.md](009-native-renderer.md),
[013-oversized-blocks.md](013-oversized-blocks.md),
[Startmessung](../../benchmarks/results/native-current/STARTUP.md).

**Dieses Dokument ändert `SPEC.md` nicht.** Es erweitert den Umfang um zwei
Funktionen und verschärft die Budgets aus Abschnitt 9. Wird es beschlossen,
wandern Umfang und Zielwerte in einem eigenen Schritt nach `SPEC.md`; bis dahin
gilt dieses Dokument als der ehrgeizigere Maßstab, nicht als Ersatz.

## 1. Die neue Zielsetzung

Bisher lautete das Ziel, gut auszusehen **und** schnell zu sein. Neu ist der
Zusatz: **unter den grafischen Markdown-Viewern für Linux der schnellste und
sparsamste zu sein**, nachgewiesen gegen echte Konkurrenz statt gegen eigene
Budgets.

Das ist keine Prahlerei, sondern eine Messvorschrift. Ein Ziel, das nur gegen
selbst gesetzte Zahlen geprüft wird, kann durch Absenken der Zahlen erreicht
werden. Ein Ziel, das gegen fremde Programme auf derselben Maschine geprüft
wird, kann das nicht.

### Was dabei ehrlich bleiben muss

Hashline wird den Wettlauf um das **leere Fenster** nicht gewinnen. ViewMD ist
ein 520-KiB-C-Programm; Hashline ist GTK4 mit Pango, syntect und AT-SPI.
Gemessen: ein leeres Hashline-Fenster braucht 60 ms bis zum ersten Frame,
ViewMD 111 ms — mit `GSK_RENDERER=cairo` liegt Hashline hier bereits vorn, aber
der Abstand ist schmal und hängt am Renderer, nicht am Können.

Der Vorsprung liegt woanders und ist strukturell: **bei wachsenden Dokumenten**.
Gemessen am 9. September 2026:

| Erster Frame nach `exec` | 100 KiB | 1 MiB | 10 MiB |
| ------------------------ | ------: | ----: | -----: |
| Hashline (`cairo`)       | 93 ms   | 76 ms | 69 ms  |
| Hashline (Vulkan)        | 219 ms  | 249 ms | 557 ms |
| ViewMD                   | 154 ms  | 1.696 ms | **nie** |

| Eingeschwungener PSS | 100 KiB | 1 MiB |
| -------------------- | ------: | ----: |
| Hashline (`cairo`)   | 41,5 MiB | 61,8 MiB |
| Hashline (Vulkan)    | 96,6 MiB | 117,7 MiB |
| ViewMD               | 45,1 MiB | 158,9 MiB |

ViewMD erreicht bei 10 MiB innerhalb von 30 Sekunden überhaupt keinen
Frame-Callback. Das ist der Punkt: Virtualisierung macht Öffnungszeit und
Speicher von der Dokumentgröße unabhängig, und genau das hält kein Konkurrent.

**Die Zielformel lautet deshalb: bei kleinen Dokumenten nie schlechter, bei
großen Dokumenten um Größenordnungen besser.**

### Ein Vorbehalt, der vor jeder Zielsetzung steht

Die `cairo`-Startzahlen sind **noch nicht belastbar**. Unter `cairo` wird
`attach` dokumentunabhängig (48 ms leer, 82 ms bei 100 KiB, 49 ms bei 1 MiB,
49 ms bei 10 MiB). Genau dieses konstante Muster ist ViewMDs Signatur für „erst
leeres Fenster zeigen, dann füllen" — und SPEC Abschnitt 9 schließt das
ausdrücklich aus: „ein leeres Fenster genügt nicht". Unter dem Standardrenderer
skaliert `attach` sauber mit dem Dokument (192 → 214 → 243 → 552 ms), dort ist
der erste Frame also nachweislich gefüllt.

Solange der Inhaltsnachweis fehlt, ist keine der `cairo`-Zeiten als „erste
lesbare Darstellung" verwendbar. Der Nachweis ist Voraussetzung dieses
Dokuments, nicht seine Folge.

## 2. Erweiterter Umfang

### 2.1 Live-Vorschau — bereits vorhanden, ungeprüft

Datei-Beobachtung ist **implementiert**: `document/mod.rs:22` nutzt `notify`,
SPEC Abschnitt 7 beschreibt 150-ms-Bündelung, Inhaltsvergleich und das
Überleben eines Dateiersetzens, und `app/mod.rs:588` hält beim Nachladen den
Leseanker.

Was fehlt, ist die Absicherung: in `crates/hashline/tests/` kommt weder
`reload` noch `watch` vor. Zu ergänzen sind deshalb Tests und ein Messwert, kein
neuer Code:

- Schreibvorgang bis sichtbarer neuer Text als eigene Metrik (`reload`).
- Vertragstests: Speichern über Rename, Löschen und Wiederanlegen,
  Editor-Zwischenzustände mit halb geschriebener Datei, schnelle Folgen von
  Schreibvorgängen, Leseanker bleibt erhalten, kein Sprung an den Anfang.

### 2.2 Tabs — echte Erweiterung

SPEC Abschnitt 2 listet Tabs heute unter „Spätere Erweiterungen". Diese
Entscheidung holt sie in den Umfang. Der Grund ist nicht Komfort, sondern
Wettbewerb: `md-viewer` (aydiler) hat Tabs, Live-Reload und Virtualisierung und
ist damit der einzige Kandidat mit demselben Anspruch.

Umfang bewusst klein gehalten:

- Mehrere Dokumente in **einem** Fenster, `GtkNotebook` oder `AdwTabView`.
- Öffnen in neuem Tab, schließen, wechseln; Tastatur `Ctrl+T` ist **kein**
  Ziel, weil es nichts zu erzeugen gibt — ein Tab entsteht durch Öffnen einer
  Datei. `Ctrl+W` schließt, `Ctrl+Tab` wechselt.
- Jeder Tab hält eigene Leseposition, eigene Suche und eigenen Watcher.
- Die Instanzübergabe öffnet einen neuen Tab statt die Datei zu ersetzen. Die
  bisherige Meldung „Es kann nur eine Datei geöffnet sein" entfällt; mehrere
  übergebene Dateien öffnen mehrere Tabs.
- **Nicht enthalten:** Tabs über Fenster ziehen, Tab-Wiederherstellung nach
  Neustart, Tab-Gruppen, Dateibaum.

Der teure Teil ist nicht die Oberfläche, sondern das Speichermodell: ein
inaktiver Tab darf keinen Layout-Cache halten. Er behält Op-Buffer, Blockplan
und Leseanker — das ist alles, was ein Wiederherstellen in wenigen
Millisekunden braucht.

## 3. Zielwerte

Alle Werte auf der Referenzmaschine, Release-Build, n ≥ 30 für Zeitreihen.
Die Spalte „bester Konkurrent" nennt den besten **gemessenen** Wert; Angaben aus
Projektbeschreibungen sind als solche gekennzeichnet und zählen nicht als
Nachweis.

### 3.1 Start und Öffnen

| Metrik | Ziel | bester Konkurrent | Faktor |
| --- | --- | --- | --- |
| Start bis lesbarer Text, 100 KiB | p95 ≤ 120 ms | ViewMD 157 ms | 1,3× |
| Start bis lesbarer Text, 1 MiB | p95 ≤ 150 ms | ViewMD 1.719 ms | 11× |
| Start bis lesbarer Text, 10 MiB | p95 ≤ 300 ms | ViewMD kein Frame | — |
| Öffnen in laufender Instanz, klein | p95 ≤ 40 ms | nicht erhoben | — |
| Öffnen in laufender Instanz, mittel | p95 ≤ 80 ms | nicht erhoben | — |
| Öffnen in laufender Instanz, groß | p95 ≤ 250 ms | nicht erhoben | — |

Die 1,3× bei 100 KiB sind bewusst bescheiden. Ein größerer Abstand ist gegen ein
520-KiB-C-Programm nicht seriös zu versprechen, siehe Abschnitt 1.

### 3.2 Speicher

| Metrik | Ziel | bester Konkurrent | Faktor |
| --- | --- | --- | --- |
| PSS, 100 KiB | ≤ 40 MiB | ViewMD 45,1 MiB (mdview nennt 34–67 MB, ungeprüft) | 1,1× |
| PSS, 1 MiB | ≤ 70 MiB | ViewMD 158,9 MiB | 2,3× |
| PSS, 10 MiB | ≤ 150 MiB | ViewMD nicht darstellbar | — |
| Speicherzuwachs über Dokumentgröße | PSS(10 MiB) ≤ PSS(100 KiB) + 2 × Dateigröße | — | — |
| Zusatz je inaktivem Tab | ≤ 3 × Dateigröße | — | — |
| 10 Tabs à 1 MiB | ≤ 110 MiB | — | — |
| Leerlauf-CPU | < 0,3 % eines Kerns über 30 s | — | — |

Der Speicherzuwachs verschärft sich von SPEC Abschnitt 9 (Vierfaches) auf das
**Zweifache** der Dateigröße. Das ist die Zeile, die das Alleinstellungsmerkmal
trägt, und sie war beim Schreiben dieses Dokuments mit 402 MiB bei 10 MiB weit
verfehlt — das Inhaltsverzeichnis allein kostete 153 MiB. Sie ist inzwischen
eingehalten; der Stand steht in [Abschnitt 8](#8-stand-der-umsetzung).

### 3.3 Laufzeitverhalten

| Metrik | Ziel |
| --- | --- |
| Scrollen 60 und 120 Hz, alle Fixtures | ≥ 99 % der Frames im Refresh-Budget über 10 s |
| Längster Stillstand beim Scrollen | ≤ 33 ms |
| Hauptthread während Interaktion | keine Aufgabe > 16 ms, **einschließlich Sonderfällen** |
| Suche, große Datei | p95 ≤ 120 ms ab letzter Eingabe |
| Suche/Menü öffnen | p95 ≤ 25 ms |
| Tabwechsel | p95 ≤ 30 ms |
| Live-Reload: Schreibvorgang bis sichtbarer Text | p95 ≤ 250 ms inklusive 150 ms Bündelung |
| Live-Reload: Leseanker | bleibt erhalten, kein Sprung an den Dokumentanfang |
| Installierte Größe | ≤ 20 MiB |

Der Zusatz „einschließlich Sonderfällen" ist wesentlich: Nach
[013](013-oversized-blocks.md) erfüllen Codeblöcke und lange Zeilen das Budget,
breite Tabellen mit 53 ms nicht. Ein Budget mit einer stillschweigenden Ausnahme
ist kein Budget.

## 4. Vergleichskandidaten

Auswahlkriterium des Auftrags: grafische Viewer mit ähnlichem Umfang. Terminal-
Programme und Editoren mit Vorschau fallen damit heraus.

### Aufgenommen: vier Konkurrenten

| Viewer | Technik | Warum |
| --- | --- | --- |
| **ViewMD** | C, md4c, GTK | Die aktuelle Latte. Installiert, mehrfach gemessen, in 009 als Maßstab gesetzt. |
| **mdview** (beleon) | C++, md4c, litehtml, Cairo/Pango | Der architektonisch nächste Verwandte: dieselbe Textmaschinerie, ohne WebView. Eigenangabe 267–426 ms und 34–67 MB RSS. |
| **md-viewer** (aydiler) | Rust, egui | Der einzige Kandidat mit demselben Anspruch: Tabs, Live-Reload, Viewport-Virtualisierung, Eigenangabe 60 fps bei über 100 000 Zeilen. Der eigentliche Wettbewerber. |
| **Okular** | Qt, Discount-Backend | Kein Peer, sondern Referenz: das Programm, das auf vielen Rechnern ohnehin installiert ist. Als Obergrenze ausgewiesen, nicht als Ziel. |

### Nicht aufgenommen

- **Kate, ghostwriter** — Editoren mit Vorschau. Anderer Umfang; ihre Startzeit
  misst einen Editor, nicht einen Viewer.
- **Glow, mdcat, Frogmouth** — Terminal. Kein Fenster, kein vergleichbarer
  Grafikstapel, keine gemeinsame Messbasis.
- **render-markdown.nvim, Emacs markdown-mode** — Editor-Erweiterungen. Ihre
  Zusatzkosten sind nur sinnvoll, wenn der Editor ohnehin läuft; das ist eine
  andere Frage als die nach einem Viewer.

### Das praktische Problem

**Installiert ist nur ViewMD.** Die übrigen drei müssen beschafft oder gebaut
werden. Ohne einen reproduzierbaren Beschaffungsschritt gibt es keinen
Vergleichslauf, und ohne festgehaltene Version ist ein Vergleich in drei Monaten
wertlos. Die Suite braucht deshalb `benchmarks/competitors.toml` mit Quelle,
festgenagelter Version oder Commit, Bauanleitung und Startbefehl je Kandidat,
sowie einen `--provision`-Schritt. Ergebnisse führen mit, welche Kandidaten
tatsächlich vorhanden waren und in welcher Version.

## 5. Die Messsuite

### 5.1 Zwei Kommandos, eine Grundlage

```
benchmarks/run.py bench   [--only GRUPPEN] [--fixtures NAMEN] [--quick] [--out DIR]
benchmarks/run.py compare [--only GRUPPEN] [--viewers NAMEN] [--fixtures NAMEN] [--quick]
```

Der tragende Gedanke: **`compare` ist die externe Teilmenge von `bench`.**
Metriken, die von außen über Wayland, `/proc` und den Compositor entstehen,
funktionieren für jedes grafische Programm gleich. Metriken, die Instrumentierung
im Programm brauchen, gibt es nur für Hashline. Beide Kommandos rufen dieselbe
Implementierung je Gruppe auf — es gibt keine zweite Messlogik, die auseinander
laufen könnte.

| Gruppe | `bench` | `compare` | Werkzeug | voll, je Programm |
| --- | :-: | :-: | --- | ---: |
| `stages` | ✓ | – | `examples/measure` | 5 s |
| `startup` | ✓ | ✓ | `startup-wayland.py` | 5 min |
| `content` | ✓ | ✓ | **neu**, Mutter-ScreenCast | 3 min |
| `memory` | ✓ | ✓ | `memory.py`, kurzes Fenster | 10 min |
| `idle` | ✓ | ✓ | `memory.py`, 30-s-Fenster, n = 5 | 4 min |
| `scroll` | ✓ | ✓ | `scroll-native.py` + `compositor.py` | 5 min |
| `interaction` | ✓ | – | **neu** | 3 min |
| `tabs` | ✓ | ✓ wo unterstützt | **neu** | 2 min |
| `reload` | ✓ | ✓ wo unterstützt | **neu** | 2 min |
| `stability` | ✓ | – | `stability.py` | 3 min |

### 5.2 Laufzeiten

Gemessene Kosten je Lauf auf der Entwicklungsmaschine, kleine Fixture:
`stages` 1 s, `startup` 4 s, `content` 6 s, `memory` 13 s, `scroll` 21 s,
`idle` 38 s. Daraus die Laufzeiten für drei Fixtures und n = 30 (`idle` n = 5):

| Aufruf | Dauer |
| --- | ---: |
| `bench --only content,memory,idle,scroll`, ein Renderer | ~1,25 h |
| dieselbe Auswahl, `cairo` und `vulkan` | ~2,5 h |
| `bench --quick` (n=5, eine Fixture) | ~3 min |
| `bench --only stages` | 5 s |
| `compare` dieselbe Auswahl, 5 Programme | ~6 h |
| `compare` dieselbe Auswahl, n = 10 | ~2 h |
| `compare --quick` | ~12 min |
| `compare --only memory --viewers viewmd` | ~3 min |

Das 30-Sekunden-Fenster für die Leerlauf-CPU ist der Grund, warum `memory` und
`idle` getrennt sind. In einer Reihe mit n = 30 wären es 93 Minuten reines
Warten für einen einzigen Zielwert, den ein einziges Fenster belegt.

Der vollständige Vergleich ist zu lang für den Alltag — deshalb ist die
Teilausführung keine Bequemlichkeit, sondern die Voraussetzung dafür, dass die
Suite überhaupt benutzt wird. `--quick` senkt Wiederholungen auf n=5 und die
Fixtures auf `small`; das taugt zur Regressionsprüfung nach einer Änderung, nicht
zur Abnahme. Jede Ausgabe trägt `acceptance: false`, solange n < 30.

### 5.3 Fairnessregeln, ohne die der Vergleich wertlos ist

1. **Inhaltsnachweis ist Pflicht.** Ohne ihn schmeichelt der Vergleich jedem
   Programm, das ein leeres Fenster früh zeigt — nachweislich ViewMD, und unter
   `cairo` womöglich Hashline selbst. Gemessen wird der erste Frame, **der
   Dokumenttext enthält**.
2. **Verschränkte Durchgänge.** Nicht erst alle Läufe von A, dann alle von B.
   Der erste Speicherlauf einer Sitzung fiel bei mir um 52 MiB zu hoch aus, weil
   der GPU-Treiber noch kalt war; `memory.py --repeat` fängt genau das ab.
3. **Renderer und Backend werden je Programm protokolliert.** Ein Vergleich
   zwischen Hashline unter Vulkan und einem GTK3-Programm ohne GSK vergleicht
   Treiberstapel, nicht Viewer.
4. **Kein stilles Auslassen.** Ein Programm, das eine Fixture nicht darstellt —
   ViewMD bei 10 MiB — erscheint als „kein Ergebnis", nicht als fehlende Zeile.
   Das ist das aussagekräftigste Einzelergebnis der bisherigen Reihe.
5. **Budgetauswertung im Ergebnis.** Jede Ausgabe stellt die Werte den Zielen aus
   Abschnitt 3 gegenüber und markiert Verfehlungen. Ein Benchmark, dessen
   Bewertung im Kopf des Lesers stattfindet, wird nicht gelesen.

## 6. Voraussetzungen, die vorher zu klären sind

Diese Punkte blockieren die Suite und sind vor ihrem Bau zu erledigen:

1. **Die Fixtures sind nicht reproduzierbar.** `benchmarks/generated/` steht in
   `.gitignore`, null Dateien sind eingecheckt, und `generate.mjs` braucht
   `marked` und `jsdom` aus npm — die Node-Toolchain ist mit dem WebView-Stack
   entfernt worden. Ein frischer Klon kann heute **keine einzige Messung dieses
   Projekts wiederholen.** Der Generator muss nach Rust, oder die Fixtures ins
   Repository. Ohne das ist jede Zielzahl unbelegbar. Das ist die einzige
   Voraussetzung, die alle anderen blockiert.

   **Erledigt.** Der Generator ist als `benchmarks/generate.rs` nach Rust
   portiert; `rustc` übersetzt ihn ohne Cargo und ohne Fremdkiste. Eingecheckt
   ist `benchmarks/fixtures/metadata.json` mit Größe und SHA-256 aller 114
   historischen Dateien, und `benchmarks/fixtures.py` prüft jede erzeugte Datei
   dagegen, bevor sie eine vorhandene ersetzt. Die Inhalte der bisherigen Reihen
   sind damit byteweise erhalten. Die Werkzeugkette prüft das bei jedem Lauf
   mit.
2. **Der Inhaltsnachweis fehlt** als Werkzeug, obwohl SPEC Abschnitt 9 ihn
   verlangt. Ohne ihn ist keine `cairo`-Startzeit als „erste lesbare
   Darstellung" verwendbar, siehe Abschnitt 1.

   **Erledigt.** `benchmarks/content.py` nimmt den Monitor über
   `org.gnome.Mutter.ScreenCast` auf und weist den Dokumenttext im Einzelbild
   per OCR nach; die Startmessung meldet nur noch `readableUpperMs`, eine
   Obergrenze mit Nachweis, und ohne Nachweis kein Ergebnis. Der Nachweis trägt
   auch `reload`, `interaction` und `tabs`. Gemessen wurde er auf allen fünf
   Programmen des Vergleichs.
3. **Die Renderer-Entscheidung steht aus.** `cairo` gegen Vulkan entscheidet über
   rund 53 MiB Speicher und mehr als 100 ms Startzeit, und die Frametimes von
   `cairo` kennt niemand. Bis das gemessen ist, sind die Zielwerte aus
   Abschnitt 3 nicht abnehmbar.

   **Entschieden für `cairo`**, siehe [015](015-renderer-choice.md): Start und
   Speicher trennen beide deutlich — 515 gegen 939 ms bis zum lesbaren Text,
   43,0 gegen 156,5 MiB PSS —, die Scrollqualität spricht mit 93,2 gegen 98,3 %
   für Vulkan. Der Abstand beim Speicher wiegt schwerer. Die Reihe mit n = 30
   bei 60 und 120 Hz steht noch aus; sie bestätigt die Wahl oder kehrt sie um.

Zwei Punkte der ursprünglichen Liste sind inzwischen erledigt: `memory.py` und
`startup-wayland.py` sind committet, und `benchmarks/REFERENCE.md` ist mit den
übrigen WebView-Unterlagen aus dem Baum entfernt — die laufenden Messbefehle
stehen in [Prüfungen](../testing.md).

## 8. Stand der Umsetzung

Stand: 9. September 2026, nach den Arbeiten an Speicher, Tabs, Live-Reload und
Hauptthread. Gemessen auf der Entwicklungsmaschine, Release-Build,
`GSK_RENDERER=cairo`, ein Prozess je Messung. **Keine dieser Zahlen ist eine
Abnahme:** die Reihen haben n < 30 und die Sitzung war nicht ruhiggestellt.

| Ziel | Wert | |
| --- | ---: | --- |
| PSS, 100 KiB ≤ 40 MiB | 40,7–40,9 MiB | knapp verfehlt |
| PSS, 1 MiB ≤ 70 MiB | 42,2–42,3 MiB | erfüllt |
| PSS, 10 MiB ≤ 150 MiB | 58,1–58,4 MiB | erfüllt |
| Zuwachs ≤ 2 × Dateigröße (20 MiB) | 17,4–17,5 MiB | erfüllt |
| Zusatz je inaktivem Tab ≤ 3 × Dateigröße | 1,9 MiB bei 1 MiB | erfüllt |
| 10 Tabs à 1 MiB ≤ 110 MiB | 59,9 MiB | erfüllt |
| Leerlauf-CPU < 0,3 % über 30 s | 0,000–0,033 % | erfüllt |
| Hauptthread ≤ 16 ms, Sonderfälle eingeschlossen | 11,0 ms (`large.md`) | erfüllt |
| Installierte Größe ≤ 20 MiB | 3,72 MiB Binary | erfüllt |
| Live-Reload: Leseanker bleibt erhalten | geprüft | erfüllt |

Der Abstand bei 100 KiB ist zu 1,5 MiB die CJK-Schrift, die `small.md` selbst
anfordert; ohne dieses eine Wort sind es 38,9 MiB
([limitations.md](../limitations.md)).

Nicht gemessen sind alle Ziele, deren Nachweis ein Einzelbild des Monitors ist:
Start bis lesbarer Text, Öffnen in laufender Instanz, Suche und Menü öffnen,
Tabwechsel und der Frameanteil beim Scrollen. Die Werkzeuge dafür stehen —
`benchmarks/run.py bench --only content,interaction,tabs,reload,scroll` misst
sie — aber sie brauchen eine unbeaufsichtigte Sitzung, in der das Fenster des
Betrachters allein auf dem Schirm steht. Auf einer Arbeitssitzung liest die
Texterkennung das, was sonst noch offen ist.

Zwei Zahlen, die keine Bildschirmaufnahme brauchen und den Rahmen abstecken:
das Setzen des ersten Schirms kostet bei 10 MiB 0,4 ms und der Blockplan
1,3 ms, und ein Suchlauf über die 7,9 MiB Text der Datei 5–6 ms. Was die
Budgets für Start, Öffnen und Suche im Wesentlichen enthält, ist damit weder
das Parsen noch das Suchen, sondern das Warten des Fensters.

## 7. Was diese Entscheidung nicht tut

- Sie ändert `SPEC.md` nicht. Die Budgets aus Abschnitt 9 bleiben gültig, bis ein
  eigener Schritt sie ersetzt.
- Sie erklärt keine Zahl für erreicht. Alle Werte in Abschnitt 1 stammen aus
  Stichproben mit n ≤ 30 auf einer nicht ruhiggestellten Entwicklungsmaschine
  mit diskreter GPU, nicht von der Referenzmaschine.
- Sie senkt kein bestehendes Ziel. Alle Änderungen an Budgets sind
  Verschärfungen.

# 015 — Cairo als GSK-Renderer für v1

Status: Entschieden. Stand: 9. September 2026.
Vorgänger: [009-native-renderer.md](009-native-renderer.md),
[014-competitive-targets.md](014-competitive-targets.md).
Bezug: [SPEC.md](../../SPEC.md), Abschnitt 9.
Messgrundlage: `benchmarks/results/local-014-fixed-rate/`,
`local-014-ocr-check/`, `local-014-baseline-check/`. Alle Werte von der
Entwicklungsmaschine bei 120 Hz, **nicht** von der Referenzmaschine.

Entscheidung 014, Abschnitt 6, führt die Renderer-Wahl als eine von drei
Voraussetzungen: ohne sie ist kein Zielwert aus Abschnitt 3 abnehmbar. Die
beiden anderen — reproduzierbare Fixtures und der Inhaltsnachweis — sind
erledigt, und erst damit ist diese Frage überhaupt messbar geworden.

## 1. Die Entscheidung

**Hashline läuft in v1 unter `GSK_RENDERER=cairo`.** Vulkan bleibt zulässig, wird
aber nicht voreingestellt und nicht als Zielumgebung gemessen.

## 2. Was gemessen ist

Kleine Fixture (100 KiB), 120 Hz, Release-Build, private Sitzung je Lauf.
Zeiten mit Inhaltsnachweis sind konservative Obergrenzen einschließlich
Aufnahmeweg, keine Scanout-Zeitstempel.

| Messung | cairo | vulkan | Verhältnis |
| --- | ---: | ---: | ---: |
| Erster ausgegebener Frame (n=2) | 417 ms | 854 ms | 2,0× |
| Erster Frame mit Dokumenttext (n=2) | 515 ms | 939 ms | 1,8× |
| PSS im Leerlauf (n=1) | 43,0 MiB | 156,5 MiB | 3,6× |
| Leerlauf-CPU über 30 s (n=1) | 0,00 % | 0,33 % | — |
| Frames im Refreshbudget (n=1) | 93,2 % | 98,3 % | **umgekehrt** |
| Längster Stillstand beim Scrollen (n=1) | 16,7 ms | 16,7 ms | gleich |

Zwei Messreihen tragen die Entscheidung, eine spricht dagegen.

**Start und Speicher sind eindeutig.** Der Abstand ist kein Rauschen: 437 ms und
113 MiB liegen um Größenordnungen über der Streuung der Wiederholungen. Die
113 MiB sind der Vulkan-Treiberstapel, nicht das Dokument — bei 100 KiB Text
verfehlt Vulkan das 40-MiB-Ziel aus 014 um das Vierfache, bevor Hashline
überhaupt etwas hält. Auch die Leerlauf-CPU trennt beide sauber: 0,33 % gegen
0,00 % bei einem Ziel von unter 0,3 %.

**Die Scrollqualität spricht für Vulkan.** 98,3 % gegen 93,2 % der Frames im
Refreshbudget, aus je einem Lauf. Beide verfehlen das Ziel von 99 %. Dieser Wert
ist die offene Flanke dieser Entscheidung, und er ist mit n=1 nicht belastbar.

## 3. Warum trotzdem cairo

Der Speicherwert entscheidet. Das Alleinstellungsmerkmal aus 014 ist der
Speicherzuwachs über die Dokumentgröße; ein Renderer, der schon beim leeren
Dokument 156 MiB hält, macht jedes Ziel aus Abschnitt 3.2 unerreichbar,
unabhängig davon, wie gut Hashline selbst arbeitet. Startzeit und Leerlauf-CPU
zeigen in dieselbe Richtung. Gegen drei Messdimensionen steht eine, und die
fehlenden fünf Prozentpunkte beim Scrollen sind ein Rückstand, den Arbeit an der
Anwendung schließen kann — 113 MiB Treiberspeicher nicht.

Hinzu kommt die Zielumgebung: cairo läuft ohne Vulkan-Treiber überall, Vulkan
nicht. Ein Standardpfad, der auf einem Teil der Zielrechner gar nicht verfügbar
ist, wäre auch bei besseren Zahlen die falsche Voreinstellung.

## 4. Was diese Entscheidung nicht behauptet

- **Kein Wert hier ist abnahmefähig.** n liegt bei 1 bis 2, gemessen auf einer
  benutzten Entwicklungsmaschine mit diskreter GPU. 014 verlangt n ≥ 30 auf der
  Referenzmaschine für Zeitreihen.
- **Die Scrollqualität ist offen.** Weder cairo noch Vulkan erreichen die 99 %.
  Ob der Abstand zwischen beiden Bestand hat, entscheidet erst die Reihe mit
  n = 30 bei 60 und 120 Hz.
- **Die Startzeit ist weit vom Ziel.** 515 ms gegen 120 ms Ziel bei 100 KiB. Die
  Wahl des Renderers ändert daran wenig; sie halbiert nur den Rückstand.

## 5. Was sie umkehren würde

Eine Reihe mit n = 30, in der cairo das Scrollbudget deutlich verfehlt und
Vulkan es hält, während der Speicherabstand schrumpft. Sinkt Vulkans PSS nicht
unter das Ziel aus 3.2, bleibt es bei cairo, auch wenn cairo beim Scrollen
zurückliegt: dann ist nicht der Renderer zu wechseln, sondern die Darstellung zu
reparieren.

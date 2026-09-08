# 006 — Vorerst eine eigene Fensterleiste verwenden

Status: Produktentscheidung vom 8. September 2026; in SPEC §3 übernommen.

Problem: Die ursprüngliche Spezifikation sah native Fensterdekoration und eine
darunterliegende Werkzeugleiste vor. Die aktuelle Umsetzung vereint Werkzeuge
und Fenstersteuerung in einer selbst gestalteten Leiste. Diese Gestaltung soll
auf ausdrücklichen Nutzerwunsch vorerst beibehalten werden.

Entscheidung: Die bestehende Custom Titlebar ist die Grundlage für v1. Die native
Fensterdekoration bleibt deaktiviert. Verschieben, Größenänderung, Minimieren,
Maximieren/Wiederherstellen und Schließen verwenden weiterhin native
Fensteraktionen über den Tauri-Adapter. Die Leiste folgt Theme und Fensterfokus;
ihre Bedienelemente bleiben per Tastatur erreichbar und zugänglich beschriftet.

Die vorhandenen Darstellungsvarianten für Windows und macOS erweitern die
Linux-Zielplattform von v1 nicht. Die Browser-Vorschau zeigt die Werkzeuge ohne
native Fenstersteuerung.

Auswirkung: Die eigene Fensterleiste ist keine offene SPEC-Abweichung mehr.
Ihre Desktop-Abnahme für Fensteraktionen, Fokus, Tastaturbedienung und Skalierung
ist für den identifizierten Linux-Build im [Abnahmebericht](../acceptance/REPORT.md)
abgeschlossen. Diese Produktentscheidung enthält keinen neuen
Performance-Nachweis und ändert die offenen Budgets nicht.

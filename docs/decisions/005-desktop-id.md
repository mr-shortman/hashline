# 005 — Desktop-Dateiname an GTK-Anwendungs-ID angleichen

Tauri 2.11 erzeugt den Desktop-Dateinamen aus `productName` (`Hashline.desktop`).
Wayland ordnet Fenster anhand der GTK-Anwendungs-ID zu. Die App aktiviert deshalb
`enableGTKAppId` mit `de.kalendium.Hashline`.

`npm run bundle` normalisiert anschließend ausschließlich den Desktop-Dateinamen
im erzeugten Debian-Paket zu `de.kalendium.Hashline.desktop` und validiert ihn mit
`desktop-file-validate`. `Name=Hashline`, `GenericName=Markdown Viewer`,
`Exec=hashline %f`, `StartupWMClass=de.kalendium.Hashline` und das Paket `hashline`
bleiben identisch zu den Spezifikationswerten. Es wird weder ein zweiter sichtbarer
Menüeintrag erzeugt noch die Standardanwendung ungefragt umgestellt.

Der Nachbearbeitungsschritt benötigt keine Rootrechte und schreibt nur das
Buildartefakt; er installiert nichts. Die saubere Installation und Zuordnung auf
einer frischen Desktop-Umgebung bleiben eine eigene Abnahme.

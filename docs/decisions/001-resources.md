# 001 — Ressourcen, native Rechte und Größenlimits

Status: implementiert; Linux-Funktionstests erfolgreich, vollständige Budgetabnahme offen.

Problem: Pauschale Tauri-Dateifreigaben würden ungewollt private Dateien für
eingebettete Bildpfade zugänglich machen. IPC-Base64 würde große zusätzliche
Speicherkopien verursachen. Reine vorherige Pfadkanonisierung schützt nicht beim
tatsächlichen Lesen gegen einen ausgetauschten Symlink.

Entscheidung: Ein expliziter Öffnungsvorgang (Dialog, native Drop-Nachricht, CLI oder
geklickter Markdown-Link) autorisiert einen Pfad. Native Commands prüfen diese
Autorisierung selbst. Jede erfolgreiche Revision erhält ein Dokumenthandle mit
einem `cap_std::fs::Dir`. Leseoperationen erfolgen relativ zu diesem Handle und
bleiben einschließlich Symlinks innerhalb des Verzeichnisses. `O_NONBLOCK` und die
Prüfung des geöffneten Dateityps weisen auch FIFOs und Geräte ab.

Ein asynchrones `hashline-image://localhost/<handle>/<quelle>`-Protokoll liest
Bildbytes über dieselbe Grenze. Die äußere Protokollkodierung und die ursprüngliche
URL-Kodierung werden jeweils genau einmal dekodiert. Kein Home-Verzeichnis wird
als Tauri-Asset-Scope freigegeben. Es gibt keine FS-/Shell-/Opener-Pluginrechte im
Frontend. Externe URLs laufen durch einen nativen Allowlist-Command; die WebView
verweigert Navigation zu fremden Ursprüngen zusätzlich zur Inhaltsbereinigung.

Limits: 20 MiB Markdown; 16 MiB komprimierte Bilddatei; 24 Millionen Pixel. Der
Header wird vor der WebView-Dekodierung geprüft. Eine maximale RGBA-Fläche von
24 MP benötigt ungefähr 96 MB, weshalb das Limit ein Schutz vor Einzelressourcen
und keine Garantie des gesamten 200-MiB-App-Budgets ist. GIF/WebP können Animationen
enthalten; deren Gesamtspeicher gehört in die noch offene Bildbelastungsmessung.

Remote-Freigabe: HTTP(S)-Bilder bleiben zunächst ohne `src` im Dokument. Eine
kompakte, per Tastatur erreichbare Aktion erklärt die Übertragung der IP-Adresse
und gibt die aktuell angezeigte Dokumentrevision frei. Das native Dokumenthandle
speichert diese Freigabe ausschließlich im Arbeitsspeicher. Eine geänderte
Revision, Dateiwechsel oder erneutes Öffnen nach einem Wechsel erfordern eine
neue Freigabe. Ein Reload ohne Inhaltsänderung behält dieselbe Revision und damit
die Freigabe. Fehlgeschlagenes Öffnen lässt das bisherige Dokument samt Freigabe
bestehen. Es gibt keine globale oder dauerhaft gespeicherte Erlaubnis.

Downloads erfolgen nativ über dasselbe `hashline-image:`-Protokoll. Die CSP bleibt
unverändert: Die WebView bekommt keinen direkten HTTP(S)-Bildzugriff und keine
zusätzlichen nativen API-Rechte. Vor Freigabe prüft auch das Protokoll den Zugriff.
Der Download sendet keine Cookies, Zugangsdaten oder Referrer und verwendet keinen
Systemproxy. URL-Zugangsdaten und andere Protokolle werden abgewiesen. Maximal drei
Weiterleitungen sind erlaubt; HTTPS darf dabei nicht auf HTTP zurückfallen.

Zusätzliche Remote-Limits pro Dokumentrevision: 64 Bildanfragen, vier gleichzeitige
Downloads, insgesamt 64 MiB empfangene Bilddaten (einschließlich fehlgeschlagener
Bildprüfungen), 15 Sekunden pro laufendem Download einschließlich Weiterleitungen
und fünf Sekunden für den Verbindungsaufbau. Wartende Downloads belegen keine
zusätzlichen Downloadslots. Beim Schließen der Revision werden wartende und
laufende Downloads abgebrochen. Byte-Signatur und Pixelmaße werden vor Übergabe
an die WebView geprüft; der HTTP-Content-Type gewährt keine Formatfreigabe.
Die bestehenden Grenzen von 16 MiB und 24 MP gelten auch für Remote-Bilder.

Bewusste Einschränkung: SVG bleibt lokal, remote und als eingebettetes HTML
blockiert, bis eine geprüfte passive Ressourcenstrategie vorliegt. PNG, JPEG,
GIF und WebP sind unterstützt. Fehlende, abgewiesene oder beschädigte Bilder
behalten einen Alternativtext-Platzhalter. Die Remote-Freigabe ist ausschließlich
in der Desktop-App verfügbar; die Browser-Vorschau erklärt diese Grenze.
Die Freigabe ersetzt keine Bildlast-/Speicherbudgetabnahme für Animationen.

Nachweis: Rust-Vertragstest für `..`, entweichenden Symlink und Leselimit; nativer
WebKitGTK-Smoke-Test für lokale Bild-URL und abgewiesenen unautorisierten Command.
Native Remote-Vertragstests und `tests/desktop/remote_images.py` prüfen Freigabe,
Revisionswechsel, Weiterleitungen, fehlende Cookies/Referrer, gefälschten MIME-Typ,
SVG sowie Byte-/Pixelgrenzen. Darstellungsabnahme siehe
[Desktop-Abnahme](../acceptance/REPORT.md); vollständige Performanceabnahme siehe Benchmarkbericht.

Referenzen: [Tauri-Capabilities](https://v2.tauri.app/security/capabilities/),
[cap-std Dir](https://docs.rs/cap-std/latest/cap_std/fs/struct.Dir.html).

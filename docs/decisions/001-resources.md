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

Abweichung: Remote-Bilder bleiben ohne Freigabeaktion blockiert. Die in SPEC §7
optionale Aktion wird erst mit einem begrenzten Remote-Downloadpfad ergänzt.
SVG ist bis zu einer geprüften passiven Ressourcenstrategie ebenfalls blockiert.
PNG, JPEG, GIF und WebP sind unterstützt. Fehlende, abgewiesene oder beschädigte
Bilder erhalten Alternativtext statt einer unkontrollierten Netzwerkanfrage.

Nachweis: Rust-Vertragstest für `..`, entweichenden Symlink und Leselimit; nativer
WebKitGTK-Smoke-Test für lokale Bild-URL und abgewiesenen unautorisierten Command.
Weitere Paket-/Hardwareabnahme siehe Benchmarkbericht.

Referenzen: [Tauri-Capabilities](https://v2.tauri.app/security/capabilities/),
[cap-std Dir](https://docs.rs/cap-std/latest/cap_std/fs/struct.Dir.html).

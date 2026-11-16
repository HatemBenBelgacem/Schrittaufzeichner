# Schrittaufzeichner

Ein kleines Programm in Rust, das deine Arbeitsschritte aufzeichnet.
Bei **jedem Mausklick** macht es einen Screenshot, markiert die Klick-Stelle mit einem
roten Kreis und erstellt am Ende einen **HTML-Bericht** (Schritt-für-Schritt-Anleitung).

## So funktioniert es

1. Programm starten.
2. Optional: Titel eingeben und Speicherort wählen
   (Standard: `Dokumente/Schrittaufzeichner`).
3. **⏺ Aufnahme starten** klicken.
4. Ganz normal arbeiten. Jeder Klick wird ein Schritt.
   Klicks auf das Fenster des Schrittaufzeichners selbst werden nicht aufgezeichnet.
5. Bei Bedarf **⏸ Pause** und **▶ Fortsetzen**.
6. Zu jedem Schritt kann man eine Notiz schreiben oder den Schritt löschen.
7. **⏹ Stopp & Bericht speichern** – der Bericht öffnet sich im Browser.

Jede Aufnahme bekommt einen eigenen Ordner:

```
Aufnahme_2026-11-16_14-30-00/
├── schritt_001.jpg
├── schritt_002.jpg
└── Bericht.html     ← eine Datei, Bilder sind eingebettet (gut zum Weitergeben/Drucken)
```

## Selbst bauen

Voraussetzung: [Rust](https://rustup.rs) (stabile Version).

```bash
cargo run --release
```

Unter Linux braucht es zusätzlich einige Systembibliotheken (siehe
`.github/workflows/build.yml`) und eine **X11**-Sitzung (unter Wayland können globale
Mausklicks nicht abgehört werden).

## Installer erstellen

Mit [cargo-packager](https://github.com/crabnebula-dev/cargo-packager):

```bash
cargo install cargo-packager --locked
```

```bash
cargo packager --release
```

Die Installer landen im Ordner `dist/`:

| System  | Datei                                   |
|---------|-----------------------------------------|
| Windows | `.exe` (NSIS-Installer) und `.msi`      |
| macOS   | `.dmg`                                  |
| Linux   | `.deb` und `.AppImage`                  |

Ein Installer kann nur auf dem jeweiligen System gebaut werden. Darum baut die
GitHub Action (`.github/workflows/build.yml`) bei jedem Push alle drei automatisch.
Die Dateien findest du danach im Reiter **Actions** unter **Artifacts**.

## Hinweise pro Betriebssystem

- **Windows:** läuft ohne besondere Rechte.
  Klicks in Programmen, die als Administrator laufen, werden nur erkannt,
  wenn auch der Schrittaufzeichner als Administrator läuft.
- **macOS:** beim ersten Start unter *Systemeinstellungen → Datenschutz & Sicherheit*
  die Rechte **Bedienungshilfen** und **Bildschirmaufnahme** erteilen.
- **Linux:** nur X11, nicht Wayland.

## Aufbau des Codes

| Datei             | Aufgabe                                                        |
|-------------------|----------------------------------------------------------------|
| `src/main.rs`     | Fenster und Bedienung (egui/eframe)                            |
| `src/recorder.rs` | Mausklicks abhören (rdev), Screenshot (xcap), Klick markieren  |
| `src/report.rs`   | HTML-Bericht erstellen                                         |
| `build.rs`        | Icon in die Windows-`.exe` einbetten                           |

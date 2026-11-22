# Selbst bauen und veröffentlichen

Diese Hinweise sind für Entwickler. Die normale Nutzung steht in der [README](README.md).

## Aufbau

| Ordner | Inhalt |
| --- | --- |
| `app/` | Rust-Anwendung (Oberfläche, Discovery, PSN-Login, Audio, Eingabe, Updater) |
| `native/` | Dünne C-Schnittstelle (`overlime_core`) um die chiaki-Bibliothek |
| `patches/` | Kleine Änderung an `gf-complete` (fehlender `#include <intrin.h>`) für clang-cl |

Der Stream-Kern (Protokoll, Takion, FEC, Video-/Audio-Empfang) stammt aus
[chiaki-ng](https://github.com/streetpea/chiaki-ng) und wird beim Bauen separat geholt.

## Selbst bauen (Windows)

1. Visual Studio Build Tools (C++), Rust (`rustup`), LLVM (clang-cl), Python 3 mit `protobuf grpcio-tools`
2. vcpkg an einem Pfad **ohne Leerzeichen** (z. B. `C:\vcpkg`):
   `vcpkg install openssl opus ffmpeg sdl2 "curl[openssl,websockets]" json-c libevent miniupnpc --triplet x64-windows-static-md`
3. chiaki-ng klonen (mit nanopb, jerasure, gf-complete), `patches/gf_cpu.c` über
   `third-party/gf-complete/src/gf_cpu.c` kopieren und mit `native/build_chiaki.bat` bauen
   (clang-cl + Ninja, `vcpkg.json` im Quellordner entfernen, GUI/CLI/Tests aus)
4. `native/build_shim.bat` baut `overlime_core.lib`
5. `cd app && cargo build --release` (Pfade über `OVERLIME_NATIVE_BUILD` und `OVERLIME_VCPKG_LIB` anpassbar)

## Installer bauen (nur selten nötig)

Es gibt **ein** Setup für alle. Es enthält die feste Basisversion 0.12, die App aktualisiert sich beim ersten Start selbst. Das Setup muss deshalb nicht bei jeder Version neu gebaut werden, nur wenn sich der Installer selbst ändern soll.

1. Inno Setup 6 installieren (`winget install JRSoftware.InnoSetup`) und Pillow (`pip install pillow`)
2. `installer\build.ps1` lädt die Basisversion von GitHub (Prüfsumme wird kontrolliert) und baut `installer\Output\OVERLIME-Setup.exe`. Symbol und Bilder erzeugt `installer\make_assets.py`, sie liegen nicht im Repository.
3. Die Datei als Asset an ein Release mit dem festen Tag `installer` hängen (als Vorab-Version markiert, damit es nicht als „neuestes Release“ gilt). Der Link bleibt immer gleich: `https://github.com/Overtime09991/overlime/releases/download/installer/OVERLIME-Setup.exe`

## Updates veröffentlichen

Die App prüft beim Start das neueste GitHub-Release (`app/src/update.rs`, Konstante `UPDATE_REPO`).

1. Version in `app/src/main.rs` (`VERSION`) erhöhen und bauen.
2. SHA-256 der exe in eine Datei `overlime.exe.sha256` schreiben (Format: `<hash>  overlime.exe`).
3. Die exe mit dem Entwickler-Schlüssel signieren (Ed25519, Signatur als `overlime.exe.sig`). Der öffentliche Schlüssel steht in `app/src/update.rs` (`UPDATE_PUBKEY`), der private liegt nur beim Entwickler. Ohne gültige Signatur installiert die App kein Update.
4. Release mit Tag `vX.Y` erstellen und `overlime.exe`, `overlime.exe.sha256` und `overlime.exe.sig` anhängen. In die Release-Notizen gehören die Abschnitte `### Neu in X.Y` (Deutsch) und `### New in X.Y` (Englisch), jeweils mit `- `-Stichpunkten, sie erscheinen in der Neuigkeiten-Seite.

Die App lädt die exe, prüft Prüfsumme und Signatur, ersetzt sich selbst und startet neu.

## Sprachen

Die Oberfläche gibt es auf Deutsch und Englisch. Die deutschen Texte stehen im Quelltext. Für Englisch werden sie beim Zeichnen übersetzt: ein kleiner Haken im mitgelieferten `epaint` (`app/vendor/epaint`, in `Cargo.toml` per `[patch.crates-io]` eingebunden) ruft `app/src/lang.rs` auf, das mit dem Wörterbuch `app/src/lang_en.rs` arbeitet. Texte mit Werten stehen dort als Vorlage mit `{}` (z. B. `Update {} verfügbar`). Neue Texte also im Wörterbuch ergänzen. Mit der Umgebungsvariable `OL_LANG_DEBUG=1` schreibt die App nicht übersetzte Texte nach `%TEMP%\ol_lang_miss.txt`.

## Lizenz

AGPL-3.0 (siehe `LICENSE`), weil chiaki-ng-Code eingebunden wird.

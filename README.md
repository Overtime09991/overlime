# OVERLIME Remote Play

Eigener PS4/PS5-Remote-Play-Client für Windows mit moderner Oberfläche (Rust + egui).
Bis 1080p60 (Sony-Limit), HEVC, D3D11-Hardware-Decoding, Controller- und Tastatursteuerung, Ton.

> Status: frühe Testversion. Das Koppeln und Streamen ist noch nicht auf breiter Basis getestet.

## Funktionen
- Konsolen im Heimnetz werden automatisch gefunden (Discovery) und lassen sich aufwecken
- Einmalige PlayStation-Anmeldung (es wird nur die Konto-Nummer gespeichert, kein Passwort)
- Koppeln per PIN, Streamfenster mit Ton, Controller (XInput/gilrs) und Tastatur
- Einstellungen: Auflösung, FPS, Bitrate, Codec, Hochskalierung
- Automatische Updates über GitHub Releases (`app/src/update.rs`)

## Aufbau
| Ordner | Inhalt |
| --- | --- |
| `app/` | Rust-Anwendung (Oberfläche, Netzwerk-Discovery, PSN-Login, Audio, Eingabe) |
| `native/` | Dünne C-Schnittstelle (`overlime_core`) um die chiaki-Bibliothek |
| `patches/` | Kleine Änderung an `gf-complete` (fehlender `#include <intrin.h>`) für clang-cl |

Der Stream-Kern (Protokoll, Takion, FEC, Video-/Audio-Empfang) stammt aus
**[chiaki-ng](https://github.com/streetpea/chiaki-ng)** und wird beim Bauen separat geholt.

## Selbst bauen (Windows)
1. Visual Studio Build Tools (C++), Rust (`rustup`), LLVM (clang-cl), Python 3 mit `protobuf grpcio-tools`
2. vcpkg an einem Pfad **ohne Leerzeichen** (z. B. `C:\vcpkg`):
   `vcpkg install openssl opus ffmpeg "curl[openssl,websockets]" json-c libevent miniupnpc --triplet x64-windows-static-md`
3. chiaki-ng klonen (mit nanopb, jerasure, gf-complete), `patches/gf_cpu.c` über
   `third-party/gf-complete/src/gf_cpu.c` kopieren und mit `native/build_chiaki.bat` bauen
   (clang-cl + Ninja, `vcpkg.json` im Quellordner entfernen, GUI/CLI/Tests aus)
4. `native/build_shim.bat` baut `overlime_core.lib`
5. `cd app && cargo build --release` (Pfade über `OVERLIME_NATIVE_BUILD` und `OVERLIME_VCPKG_LIB` anpassbar)

## Updates veröffentlichen
Im GitHub-Release `overlime.exe` und `overlime.exe.sha256` (SHA-256 der exe) hochladen, Tag z. B. `v0.7`.

## Lizenz
AGPL-3.0 (siehe `LICENSE`), weil chiaki-ng-Code eingebunden wird. Quellcode: dieses Repository.

OVERLIME ist kein offizielles Sony-Produkt. „PlayStation“ ist eine Marke von Sony Interactive Entertainment.

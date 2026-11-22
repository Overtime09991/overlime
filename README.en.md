<div align="center">

# OVERLIME

### Your PS4 and PS5 Remote Play for the PC. Beautiful, fast, simple.

Play your PlayStation games on a Windows PC at up to **1080p and 60 FPS**,
with a modern interface and a start in under a minute.

[**Download**](https://github.com/Overtime09991/overlime/releases/download/installer/OVERLIME-Setup.exe) &nbsp;•&nbsp; Windows 10/11 &nbsp;•&nbsp; PS4 & PS5 &nbsp;•&nbsp; free & open source

[Deutsch](README.md) · English

</div>

> **Notice:** OVERLIME is an independent project and not affiliated with Sony. “PlayStation” is a trademark of Sony Interactive Entertainment. No Sony logos or images are used.

> **Early test version.** The interface, sign-in and console search are done. Pairing and streaming are being tried out with real consoles. Feedback is welcome.

## Why OVERLIME?

- **Easy to start.** Sign in to PlayStation once, pair your console, play. The app remembers everything.
- **Your console is found automatically.** No IP addresses, no settings, no expert knowledge needed.
- **Wake from rest mode.** One click and the console starts, without getting up.
- **The best picture Sony allows.** Up to 1080p60 at up to 50 Mbit/s and HEVC (H.265) where the console supports it, hardware decoding on the graphics card for low latency.
- **Controller or keyboard.** PS5, PS4, Xbox and other controllers are detected automatically, also via Bluetooth, including vibration. Without a controller the keyboard works.
- **Record, customize, set up.** Video recording with F9, new in-stream menus with live charts, freely selectable status display, a setup assistant for your own controllers and optional interpolated frames for 120 Hz screens.
- **Per console and on a schedule.** Own quality for every console, network test with a recommendation, sound controls and optionally waking the console at a fixed time.
- **Security.** Updates are signed (Ed25519): OVERLIME only installs versions that match the built-in key. The console pairing keys and your settings are stored encrypted (Windows user account). Problems can be reported directly from the app.
- **Clearly laid out.** An assistant guides you through the first sign-in, settings have search and categories, help and diagnostics are one page with tabs. OVERLIME opens only once at a time.
- **Tray icon.** A small background icon (starts with Windows if you like): one click wakes the console and OVERLIME connects automatically.
- **News in the app.** A page shows what is new in the current version, older ones on click.
- **Updates at a glance.** A dedicated update area in the settings shows new versions with their changes, install with one click, plus a notice in the top bar.
- **Controller diagnostics.** Live test of all buttons, sticks, triggers, touchpad and motion, plus a list of detected devices.
- **PS5 controller fully used.** Touchpad, motion sensor, adaptive triggers, light bar, vibration as in the game and battery level (DualSense, also via Bluetooth).
- **Latency at a glance.** The top bar shows ping, estimated total latency and data rate, colored by connection quality.
- **Quick menu in the stream.** Change sharpness instantly, adjust resolution, frame rate and bitrate with a short reconnect.
- **Smooth picture.** The graphics card converts and upscales the image, with a real sharpening filter (CAS) for “Sharp” and “Ultra”.
- **Sound included.** Output device and volume freely selectable, mute with the M key, best audio format automatically (32-bit, 48 kHz).
- **Diagnostics and help.** A diagnostics page checks network, console, account and controller and explains what is wrong. “Report a problem” sends a report without IP addresses.
- **Convenience.** OVERLIME can connect to your console automatically at start (and wake it up). Screenshots in the stream with F12.
- **Your look.** Five color schemes (from ocean to pure black) and your own themes as a simple text file, plus gentle animations.
- **Discord status (optional).** If you like, Discord shows “Playing OVERLIME” with game and play time. Off by default, without IP addresses or console names.
- **Nothing by accident.** Esc only opens a pause menu, and closing the window in the middle of a stream asks first.
- **Language.** German and English, selectable in the settings (the language of Windows is used at first start).

## Get started in three steps

1. **Download:** get [OVERLIME-Setup.exe](https://github.com/Overtime09991/overlime/releases/download/installer/OVERLIME-Setup.exe) and double-click it. This is the only download you will ever need.
2. **Sign in and pair:** sign in with your PlayStation account (only once), click **Pair** next to the console and enter the 8-digit code the console shows under *Settings › System › Remote Play › Link Device*.
3. **Play:** click **Connect**.

**On the console first:** enable Remote Play (*Settings › System › Remote Play*). If the console should wake from rest mode, switch on “Stay Connected to the Internet” and “Enable Turning On PS5 from Network” there.

> Windows may show “Windows protected your PC” for new programs. Then click *More info › Run anyway*. If a file `VCRUNTIME140.dll` is missing at start, install the [Microsoft Visual C++ Redistributable](https://aka.ms/vs/17/release/vc_redist.x64.exe) once.

## Installation (one time)

There is **one single setup for everyone**: [OVERLIME-Setup.exe](https://github.com/Overtime09991/overlime/releases/download/installer/OVERLIME-Setup.exe). It never has to be downloaded again.

- The setup installs OVERLIME in your user folder, without administrator rights, with a Start menu entry and optionally a desktop icon and start with Windows.
- At the **first start** OVERLIME fetches the **latest version** by itself. After that all updates come automatically, the app checks and installs them on its own.
- There is an entry under *Settings › Apps*, from where OVERLIME can be uninstalled.

Alternatively the [releases page](https://github.com/Overtime09991/overlime/releases) offers the single `overlime.exe` without installation.

## Quality

| Setting | Choice |
| --- | --- |
| Resolution | 540p · 720p · 1080p (Sony limit), also switchable in the stream |
| Frame rate | 30 or 60 FPS |
| Bitrate | 2 to 50 Mbit/s |
| Codec | Automatic · H.264 · HEVC |
| Display | Upscaling on the graphics card (Off, Soft, Sharp, Ultra), fullscreen (F11) |

With **⚡ Maximum quality** you set everything to the best at once.
*Tip for the best result:* connect the console with a LAN cable, the PC with a cable or 5 GHz Wi-Fi.

## Controls

Controllers are detected automatically. Without a controller the keyboard works, all keys can be freely assigned in the settings. Default layout:

| Key | Function |
| --- | --- |
| W A S D | Left stick |
| Arrow keys | D-pad |
| Space / E / Q / R | Cross / Circle / Square / Triangle |
| Z / X | L1 / R1 |
| Shift / Ctrl | L2 / R2 |
| Tab / Backspace / F2 | Options / Share / PS button |
| F9 / F12 | Record video / screenshot |
| F11 | Fullscreen |
| Esc | Pause menu (does not quit anything by accident) |
| F1 / F10 | Key layout / status display |

## Privacy

- You enter your password **only on Sony’s website**. OVERLIME never sees it.
- Only your PlayStation account number and the pairing with your console are stored, locally on your PC.
- There are no ads, no tracking and no user accounts with us.
- The Discord status is only active if you switch it on in the settings.
- If you send a problem report, it contains the OVERLIME version, quality settings, controller, consoles found and the log, without IP addresses, passwords or your account number.

## Frequently asked questions

**Why only 1080p, not 4K?** The PlayStation sends at most 1080p over Remote Play. Sony sets that on the console.

**My console is not found.** PC and console must be on the same network, the console on or in rest mode with a network connection. Confirm the Windows firewall prompt at the first start with *Allow access*.

**Where do I find the log if something goes wrong?** In the app under *Help › Open log folder*.

## About the project

OVERLIME is open source (AGPL-3.0). The streaming core is based on the project [chiaki-ng](https://github.com/streetpea/chiaki-ng), to which we are grateful for the great work.
OVERLIME is not an official Sony product. “PlayStation” is a trademark of Sony Interactive Entertainment.

For developers: notes on building and publishing are in [BUILDING.md](BUILDING.md).

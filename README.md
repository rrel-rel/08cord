<h1 align="center">'08cord</h1>
<p align="center">
  A lightweight native Discord client for Windows, written in Rust and dressed as a late-2000s Source engine menu.
</p>

'08cord is one program of about 23 MB with no Electron and no browser engine inside it. It draws its own UI with
[egui](https://github.com/emilk/egui) on Direct3D 11, and uses Windows itself for everything else: images, TLS,
audio, video encoding, the clipboard, notifications and fonts.

It started as a Windows port inside a fork of [Swiftcord](https://github.com/SwiftcordApp/Swiftcord), the native
macOS client. None of Swiftcord's code is left, so it lives here now. The history came along, so everyone who
worked on it is still credited.

## Compared with the official Discord app

Both apps were measured on the same machine, signed in to the same account, then left idle on the same screen.

| | Official Discord (1.0.9268) | '08cord (0.1.0) | |
| --- | ---: | ---: | ---: |
| Download | 153.4 MB | 13.0 MB (x64 and ARM64) | 12× smaller |
| On disk once installed | 620.9 MB | 26.9 MB | 23× smaller |
| Processes | 6 | 1 | |
| Window on screen | 9.0 s (splash), 23.6 s (app) | 0.2–0.3 s (1.8 s on a first launch) | |
| Signed in, with your servers | after the app window | 2.2 s | |
| Memory (working set) | 1,350 MB | 94 MB | 14× less |
| Memory (private) | 1,000 MB | 55 MB | 18× less |
| Threads | 192 | 15 | |
| Handles | 4,239 | 659 | |
| CPU while idle | 45.5 % | 0.44 % | ~100× less |

How they were measured: memory, threads and handles are summed over all of an app's processes after a minute of
settling, and idle CPU is all of their processor time over the next minute, as a share of all 4 cores. Startup is
the time from launch until the app has a window. The test machine is a virtual machine with 4 cores, 8 GB of
memory and no GPU, running Windows 11 (build 26200). With no GPU, Discord (Chromium) draws in software, which
inflates its idle CPU; on a PC with a graphics card expect it to be much lower, though still well above '08cord's.
Figures are from 2026-11-17.

## Features

- **Signing in:** a QR code scanned with the Discord phone app, Discord's own login page (in a separate process, so
  the client never loads a browser engine), or a token. The token is kept in Windows Credential Manager.
- **Messaging:**
  - servers and folders, channels filtered by your permissions, DMs and group DMs, unread and mention badges;
  - a history that loads as you scroll up;
  - Discord's markdown, with embeds, Components V2, attachments, voice messages, videos, YouTube embeds that play
    in place, GIFs, stickers (Lottie and APNG) and an image viewer;
  - reactions, replies, editing, deleting, forwarding, pins, Mark Unread, threads and forum channels;
  - message search, a mentions inbox, and mention, channel, emoji and slash-command autocomplete;
  - apps' buttons, select menus and forms;
  - uploads from the picker, drag and drop or the clipboard.
- **Voice and video:**
  - server voice channels and DM calls, end-to-end encrypted with DAVE when the call is;
  - cameras, Go Live screen and window sharing (with stream audio), and watching others' streams;
  - noise suppression, voice-activity sensitivity, a mic test and per-user volume.
- **People:** profile cards with banners, colours and decorations; server profiles; editing your own profiles; the
  member list; real statuses, including mobile; a status picker.
- **Windows:**
  - toast notifications that follow your Discord notification settings;
  - a tray icon, starting with Windows, a single instance and remembered window placement;
  - `Alt+Shift+↑/↓` jumps between unread channels.
- **Look and sound:**
  - a VGUI skin with tabbed, Source-style option windows and Source UI sounds;
  - colour themes and an accent colour;
  - optional shader transitions.
- **Servers:** a server menu with invites, notification settings, privacy, and moderation (members, bans, audit
  log, AutoMod) where your permissions allow.

## Installing

There are no releases yet. Build the installer yourself (see below); it installs per user, needs no administrator
rights, and holds x64 and ARM64 builds.

## Building

You need Rust (stable) and the MSVC build tools with the Windows SDK. There are no other native dependencies: Opus is
the transpiled [unsafe-libopus](https://crates.io/crates/unsafe-libopus), and the rest is Windows' own.

```powershell
cd rust
cargo run -p swiftcord                # debug
cargo build --release -p swiftcord    # target\release\swiftcord.exe
cargo test --workspace
.\tools\build-installer.ps1           # target\installer\08cord-Setup-<version>.exe (needs Inno Setup 6)
```

| Crate | What it does |
| --- | --- |
| `rust/crates/core` | The client without a UI: gateway (zlib-stream, heartbeats, resume), REST with rate limits, the state cache, Discord markdown, permissions, QR login. |
| `rust/crates/voice` | Voice gateway v8 with DAVE ([davey](https://crates.io/crates/davey)), AES-GCM and XChaCha20 RTP encryption, Opus, WASAPI audio, Media Foundation video. |
| `rust/crates/app` | The app: the D3D11 renderer and everything you see. |

The crates and the data folder still use the Swiftcord name: settings, logs and cache live in
`%LOCALAPPDATA%\Swiftcord\data` (`SWIFTCORD_LOG=debug` logs more).

## Contributors

- [rrel-rel](https://github.com/rrel-rel) (Ron Chrysler Amistad): the Rust client.
- [FunOrange](https://github.com/FunOrange): the Direct3D 11 renderer, the VGUI skin and '08cord branding, UI
  sounds, the tray, notifications, startup and offline mode, among much else.
- [cryptoAlgorithm](https://github.com/cryptoAlgorithm) (Vincent Kwok): Swiftcord, where this began.

## License and notice

[MIT](LICENSE). '08cord is an unofficial client and is not affiliated with Discord or Valve. It signs in with your
user token, which is against Discord's Terms of Service, so use it at your own risk.

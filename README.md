# Open GHub

> Open GHub is an independent project. It is not affiliated with, endorsed by
> or supported by Logitech. Logitech, Logitech G, G HUB, LIGHTSPEED and product
> names are trademarks of Logitech, used here only to identify compatible hardware.

A small Tauri 2 + Rust app that configures Logitech gaming mice directly over
HID++ 2.0, the same protocol G HUB's `lghub_agent.exe` uses. No G HUB, no
background service: the app opens the mouse's vendor HID collection and sends
feature requests itself.

## What it does

| Setting | HID++ feature | Notes |
|---|---|---|
| Device discovery | Root `0x0000`, FeatureSet `0x0001` | Direct USB devices and receiver slots 1–6 |
| Name and type | `0x0005` | |
| Battery | `0x1004`, `0x1000`, `0x1001` | Whichever the device has |
| DPI | `0x2201` AdjustableDPI, `0x2202` ExtendedAdjustableDPI | Separate X/Y and lift-off distance on `0x2202` mice |
| Report rate | `0x8060`, `0x8061` | Up to 8000 Hz on supported mice |
| Onboard / software mode | `0x8100` OnboardProfiles | Switches between profile memory and live control |

## Tray and background mode

- Closing the window keeps Open GHub running in the system tray, so
  auto-switching and battery warnings keep working. Quit from the tray menu.
- The window is destroyed when closed and recreated when opened, so in the tray
  no WebView2 processes run: about 27 MB of RAM in one process (release build).
  Started with Windows (`--minimized`), the window is never created at all.
  WebView2 runs with `--disable-gpu`, which avoids a ~100 MB GPU process.
- **Tray menu:** battery per device, a profile switcher, the auto-switch toggle,
  Show Open GHub, Start with Windows, and Quit. Left-click opens the window.
- **Start with Windows** (tray menu) launches Open GHub minimized to the tray
  (`--minimized`).
- **Low-battery notifications** at 15% and 5%, once each per discharge.
- **Event-driven, no polling:** profile auto-switch uses a Windows foreground
  hook; battery, DPI and profile changes come from the mouse's HID++
  notifications; a mouse waking up or reconnecting comes from the receiver's
  connection notification (Open GHub enables these on the receiver); plugging
  devices in or out comes from Windows device notifications. A safety battery
  check runs every 15 minutes. Idle CPU is effectively zero.
- Set `OPEN_GHUB_LOG=1` to print every event to the console (diagnostics).
- Only one instance runs. Launching it again opens the existing window.
- Windows may not show notifications from a build that hasn't been installed;
  the installer (`npm run app:build`) registers the app so they appear.

## Updates

Open GHub updates itself from this repository's GitHub releases using the
Tauri updater. It checks 30 seconds after start and then every 6 hours, even
while it's in the tray. When a newer version exists it shows a notification,
an "Install update" item in the tray menu, and an Install button in
**App settings** (gear icon). Turning on **Install updates automatically**
installs new versions without asking. Each installer is signed, and Open GHub
verifies the signature before installing.

The endpoint is `latest.json` on the latest release:
`https://github.com/abidr/open-ghub/releases/latest/download/latest.json`.

### Publishing a release

```sh
npm run release -- 0.1.1 --notes path/to/notes.md            # publish
npm run release -- 0.1.1 --notes path/to/notes.md --dry-run  # build only
```

`scripts/release.mjs` checks that `main` is clean and pushed, bumps the version
everywhere, builds the signed installer, commits and tags `v0.1.1`, pushes, and
creates the GitHub release with the installer, its `.sig` and `latest.json`.
It needs the GitHub CLI signed in (`gh auth login`).

**Signing key:** the private key is at `~/.tauri/open-ghub.key` (or set
`OPEN_GHUB_SIGNING_KEY`). Never commit it, and keep a backup. If it's lost,
installed copies can't verify new releases and users must reinstall manually.
The matching public key is in `src-tauri/tauri.conf.json`. Because the bundle
creates updater artifacts, `npm run app:build` needs the key too
(`TAURI_SIGNING_PRIVATE_KEY`); use `npm run release -- <version> --dry-run`
for local installer builds.

## Device names

Firmware often reports an internal name (a PRO X SUPERLIGHT reports
"PRO X Wireless"). Open GHub reads the device's product ids with HID++
feature `0x0003` and looks them up in its built-in table,
`src-tauri/src/device_db.rs` (110 Logitech G products). Unknown devices keep
their firmware name. No G HUB installation is needed. To add a device, append
its product id and name to the table, keeping the ids sorted.

## Profiles

Profiles work like G HUB's software profiles: each one stores DPI levels (with
one active level) and a report rate, and is linked to an application.

- **Desktop** is the fallback profile, used when no other profile matches.
- **Auto-switch** (off by default) polls the foreground window every 750 ms.
  When its executable matches a profile's app path (the exact `.exe`, or any
  program inside a linked folder; the longest match wins), that profile is
  applied to every connected mouse. Open GHub's own window never triggers a switch.
- **Import from G HUB** (optional; only works where G HUB is or was installed) reads `%LOCALAPPDATA%\LGHUB\settings.db` read-only and
  converts each application's profile, using the DPI table and report rate
  from its `MOUSE_SETTINGS` card. Re-importing updates earlier imports instead
  of duplicating them. Only games G HUB found installed get an app path.
- Profiles are stored in `%APPDATA%\dev.local.openghub\profiles.json`.
- Values are snapped to what each mouse supports when applied. Profiles are
  software-side only and are not written into the mouse's onboard memory.

Diagnostics (read-only): `open-ghub.exe --ghub-profiles` prints what an import
would produce, and `--probe` also prints the current foreground app.

## How it maps to G HUB

```
G HUB:      Electron UI ──named pipe──► lghub_agent.exe ──HID++──► mouse
Open GHub: Web UI ──Tauri invoke──► Rust (src-tauri) ──HID++──► mouse
```

- `src-tauri/src/hidpp.rs`: request/response transport over 20-byte long reports
- `src-tauri/src/device.rs`: feature discovery plus DPI, report rate, battery and onboard mode
- `src-tauri/src/manager.rs`: enumeration, receiver slot probing, rescans
- `src/`: React + TypeScript front end built with Vite, Tailwind and
  [shadcn/ui](https://ui.shadcn.com) components (`src/components/ui`), themed
  with G HUB's colour palette in `src/index.css`
  - `src/lib/api.ts`: typed wrappers around the Tauri commands
  - `src/components/`: header, home (device cards), device page, DPI and settings panels

## Build

Prerequisites on Windows:

1. Rust stable (MSVC toolchain): `rustup default stable`
2. Visual Studio C++ build tools: the "Desktop development with C++" workload, including a Windows SDK
3. Node.js 20+
4. WebView2 runtime (preinstalled on Windows 11)

```sh
npm install
npm run app:dev     # run the app with hot reload (starts Vite on :1420)
npm run release -- <version> --notes <file> [--dry-run]   # signed installer + GitHub release
npm run build       # type-check and build only the front end into dist/
npx shadcn@latest add <component>   # add more shadcn/ui components
cd src-tauri && cargo test   # DPI-list decoder tests
src-tauri/target/debug/open-ghub.exe --probe   # read-only: print detected devices and settings as JSON
```

## Notes

- **G HUB can fight you.** If G HUB is running it may reapply its own profile.
  Quit it from the system tray while using Open GHub.
- **Onboard memory mode.** In this mode the mouse runs the profile stored on it.
  A DPI change applies until you press a DPI button or switch profiles. Some
  mice only accept report-rate changes in software mode.
- **Software mode** settings last until the mouse loses power. They are not
  saved to the mouse.
- Requests are tagged with software id `0xB`, so replies meant for other
  software that has the device open are ignored.

## License

Copyright (C) 2026 Abidur Rahman

Open GHub is free software: you can redistribute it and/or modify it under the
terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. It is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE. See [LICENSE](LICENSE) for the full text.

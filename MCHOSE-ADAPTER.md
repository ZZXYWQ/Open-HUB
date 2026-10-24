# Open GHub MCHOSE 0.2.0

Personal Windows adaptation of abidr/open-ghub for the physically verified MCHOSE A7 V3 Ultra+ (USB 3837:1014, model 3837:4026) and MagDock (3837:1012). Logitech HID++ stays on its existing transport and UI.

## Implemented and checked
- Official A7 V3 Ultra+ pictures in white, black, blue, red and pink; official MagDock picture.
- Six X/Y DPI stages, native polling rates, LOD, debounce, sleep, sensor options, button assignments, 18 onboard macro slots and three board profiles.
- Six factory MagDock modes, switch, RGB color, brightness, speed, direction and reset. Firmware expresses music mode using its separate sync flag; the adapter normalizes this on reads.
- Software static, rainbow, gradient, breath, comet, twinkle, six amplitude-reactive music styles, and primary-display sampling. An independent lighting page works without a mouse attached.
- Only selected OpenRGB devices are controlled. Expected device identity (name/vendor/version/serial/location) is checked again before any streaming writes. Hotplug notifications stop the stream; stale numeric IDs are not silently reused. Unselected devices are left alone.
- Read-only M HUB import from `%APPDATA%/MCHOSEHUB/files/config/mc_main_store_key.json` or a selected JSON file. Only supported device and board fields are consumed. Three named board references are imported; no fabricated DPI is displayed, and device settings remain in onboard memory. Cloud and unpublished software-only plans are outside this import.
- Profiles have optional vendor/model scopes. Legacy MCHOSE board references are implicitly MCHOSE/model scoped; G HUB imports are Logitech scoped. Incompatible devices are skipped before any protocol call. Device page profile pickers also filter by scope.

## Protocol
Mouse V3 packet details are implemented in `src-tauri/src/mchose.rs` and `mchose_config.rs`. Original byte fields are preserved, writes are checked by re-reading, and backups are taken before changes.

MagDock uses report ID 0, a 65-byte HID output with leading report byte, and payload header `[AA, command, type, sequence, total_frames, payload_length]`. Read command 7 returns 30 state bytes; command 39 sets the ten lighting bytes; command 41 supplies twelve RGB LEDs; command 42 resets factory lights. Only exact VID/PID/product/usage matches are opened. No firmware flashing or bootloader commands are exposed.

The packet layouts were independently implemented using the public M HUB bundle at https://www.mchose.com.cn/assets/purify.es-BufdR6vO.js and actual device replies. Official product images are included for local device identification; attribution is in the bundled notices.

## RGB and capture
Unmodified OpenRGB 1.1 is bundled with its GPL license and corresponding source archive. Its SDK server is bound only to 127.0.0.1:6742. The bounded SDK v3 client talks to supported Direct-mode controllers, snapshots original mode/colors, and restores them on stop. A ViewSonic XG270QG was physically exercised. Other models depend on OpenRGB compatibility and driver conditions; no kernel driver is installed by this package.

Music uses the Windows default render endpoint's peak meter (IAudioMeterInformation), not microphone capture or frequency-band FFT. Screen mode samples a 4x3 grid of locally averaged pixels on the primary monitor and smooths transitions; protected content/HDR/fullscreen can affect sampling. Audio/screenshots are not saved or transmitted.

## Installation
The Chinese NSIS installer includes the full Microsoft-signed offline WebView2 runtime, application, WebView2 loader, OpenRGB and notices. Default installation is per-user under `%LOCALAPPDATA%/Programs/Open GHub MCHOSE`, separate from backups. Install/uninstall hooks ask the running app to stop lighting, restore state and quit via `--prepare-update` before replacement. User data is retained by default. The personal build is unsigned.

## Limits
Firmware updates, device pairing and M HUB cloud/account services stay in the official application, accessible via the maintenance entry. A connected Logitech mouse was not available for physical regression tests; its existing protocol path is retained and cross-brand dispatch guards have automated tests. This is not a claim of universal support for every mouse or PC RGB controller.

## Rebuilding
Install Node.js, Rust and the platform toolchain, run `npm ci`, `npm run build`, then Tauri build/bundle. The supplied source includes the NSIS template, upgrade hooks, product images and unmodified OpenRGB runtime resources. GNU Windows builds need WebView2Loader.dll, which Tauri includes in the installer automatically. This session used Rust 1.99.1 GNU, Node 22.23.4 and Tauri 2.12.4.

0.2.1 UI update: G HUB and M HUB now share the same Import Configuration menu, with equal-level brand submenus. The previous standalone M HUB banner has been removed. Existing import commands and vendor/model isolation are unchanged.

0.2.1 is now branded **Open HUB**. Window, tray, executable (`open-hub.exe`), install directory, shortcuts, uninstall entry and installer filename use the new name. The old internal app-data identifier and backup format are deliberately retained for compatibility. The installer migrates the previous local fork; it does not uninstall the original Logitech application or M HUB.

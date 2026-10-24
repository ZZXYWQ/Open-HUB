# Adding another mouse brand

Existing device support is separated by hardware identity and transport. Adding a brand to a label list does not enable its hardware.

1. Add its vendor label/import-source metadata in `src/lib/brands.ts`.
2. Add explicit VID/PID, usage and model identity checks to device discovery. Add a native protocol adapter and route its commands through that adapter; do not send MCHOSE or Logitech packets as an unknown-device fallback.
3. Register an appropriate device control page and product imagery. `device-page.tsx` routes MCHOSE, the exact-model native bridge, SinoWealth and the original Logitech path separately; do not infer native control support from an OpenRGB-only device.
4. Add a read-only importer for that brand's configuration, with file size/range checks and a whitelist of supported fields. Imported profiles must carry `target_vendor` and, when appropriate, `target_model`. Preserve app paths on re-import; do not import tokens/accounts.
5. Keep `Profile::targets` and device-page selection guards consistent. Add mismatch tests proving no control command can run against a different vendor/model. Existing generic profiles retain their explicitly generic behavior.
6. For cross-device RGB, use the separate, opt-in OpenRGB target list. Automatically discover and select compatible mice while idle when the user enables automatic selection. Remember explicit opt-outs; never start writes automatically or change the targets of a running stream. Check full expected device identity again before writes, and stop on topology changes.
7. Verify real read/write/restore behavior with that exact model. Do not advertise untested firmware flashing, RGB modes or universal compatibility. Keep hardware backups separate from installation files.

Current importers include Logitech G HUB settings.db (DPI/report-rate), MCHOSE M HUB local JSON (the three existing A7 V3 Ultra+ board references), supported Naga V2 HyperSpeed Synapse exports, M65 PRO RGB iCUE cereal XML, Rival310 GG SQLite, and Open HUB profile packs. Source model/format limits are enforced by the converters; unsupported semantics block conversion instead of silently dropping actions. The new native brands are simulated, not hardware-verified. Importing writes only this application's software profile store; applying to a matching physical mouse is a separate action.

The iCUE and GG import UIs share `vendor-profile-import.tsx`; source-specific native converters retain their own protocol and field validation. The six source buttons occupy one menu. Backend and browser regression evidence is recorded in `MULTIBRAND-PROTOCOL-WORK.md`; a passing simulated import is not universal brand compatibility.

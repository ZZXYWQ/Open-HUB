# Open HUB 0.3.1 ATK validation — 2026-10-24

ATK adds A9 Plus NK USB 373B:1115 and older NearLink 373B:10C9 (CID/MID 2,83), and X1 Pro Max USB 373B:1017 (2,39). These are two mouse models / three exact transport identities, not whole-brand compatibility. ATK has no physical-device validation. Scope and protocol provenance: ATK-ADAPTER.md.

239 native regressions passed, including 19 ATK tests. ATK tests exercise the production driver over synthetic HID, native macro upload/readback/re-import, all three playback modes, no-free-slot preservation, interrupted-upload restoration, corrupt/truncated/status/identity replies, left-click protection, 8-stage DPI, exact rate ceilings, static indicator colors, and bridge serialization/execution. All 15 browser UI suites passed; ATK UI checks read-only import, stale-preview clearing, exact-device mismatch rejection, macro contents/mode, no auto-application, and native macro editing/simulation. Installation evidence is recorded separately in the delivered verification JSON.

The optional Device native-macro methods have default implementations; prior drivers do not receive ATK macro definitions. No new runtime dependency or official control center is bundled. App identifier and user configuration locations are unchanged.

Appearance adds three native schema/export tests and a browser suite covering color presets, validated 8 KB skin imports/exports, persistence, background alpha without fading text, minimum-size reachability and device-write isolation. The new abstract Z-motion icon is used by the header and all app/installer/tray assets. See APPEARANCE.md and docs/BRANDING.md.

## Previous 0.3.0 release evidence

# Open HUB 0.3.0 validation — 2026-10-24

## Scope and limits

The completed integration scope is three exact models: Razer Naga V2 HyperSpeed
1532:00B4, Corsair M65 PRO RGB 1B1C:1B2E, and SteelSeries Rival 310 1038:1720.
It includes supported-source conversion, on-disk full-profile persistence,
DPI/rate, supported button actions, host macro execution and corresponding UI.
Unsupported source actions/layouts are blocked as a whole candidate. This is not
whole-brand support. MOUSE-BRAND-SUPPORT.md is the model/capability matrix.

The new models use real protocol drivers with synthetic HID/input sinks. No
physical verification is claimed for them. Existing MCHOSE A7 V3 Ultra+/MagDock
are the available mouse/dock hardware. Browser suites mock Tauri and consume
native DTO fixtures where applicable; they are separate evidence from native
source-to-output tests, not a claim that browser mocks execute native code.

## Requirement audit

| Requirement | Authoritative implementation and validation |
|---|---|
| Naga source import/save and executable controls | `synapse_profiles.rs`, Naga production driver, `import_execution_tests::naga_synapse_file_disk_reload_native_trigger_and_macro_output`; actual Synapse 4 JSON/base64 file entry -> disk store -> HID driver -> physical-source dispatch -> complete A/B hold output |
| M65 source import/save and executable controls | `icue_profiles.rs`, Corsair NXP driver, `m65_icue_file_disk_reload_vendor_button_and_macro_output`; iCUE XML entry -> disk -> vendor-button dispatch -> full macro output. An absent source polling rate stays absent; the session view may reflect the target's known rate |
| Rival source import/save and executable controls | `gg_profiles.rs`, SteelSeries driver, `rival_gg_database_disk_reload_native_trigger_and_mixed_macro_output`; SQLite source -> disk -> native trigger -> mixed keyboard/mouse output |
| Profile persistence and editing | Production ProfileStore and merge validation are used by all three cross-layer tests. `session_plan.rs` verifies full desired plan across read/edit/reapply, temporary DPI handling, removed sources, serial/model isolation and scoped stop |
| UI and configuration imports | All 13 rendered browser suites passed. Brand chooser has 20 checks, 13 individual supported-brand entries, one toolbar entry, bounded narrow-window layout and source-specific dialogs. Import suites cover edit/save, macro modes, invalid sources and foreign models |
| Logitech/MCHOSE isolation | Backend filters/exact VID/PID validation; tests reject plans before unrelated cleanup or writes. Cross-layer stores include both legacy brands and assert byte-equivalent serialized profiles. Foreign source/model/serial and scoped-stop regressions pass |
| Fault handling | Native suites cover malformed/unbalanced macros, held keys, cancellation, disconnect, concurrent ownership, write-ahead failure, stale/foreign recovery, changed polling re-enumeration, unknown send results and reviewed retries. `manual_recovery.rs`, `rate_reconnect.rs`, `macro_recovery.rs`, `macro_runtime.rs` and driver tests implement these paths |
| Installer and migration | Production NSIS is extracted and all 35 configured executable/loader/resource payload files compared by SHA256. Actual 0.2.3 upgrade, same-version overwrite, uninstall retaining data, absent-directory/registration reinstall and restart pass. Original user files remain byte-identical. Shared production NSIS macro also passes nested/Unicode/repeat/copy-sharing-failure/destination-failure/missing-source fixtures |
| Compact distribution and licensing | Normal installer is about 39 MiB with system/downloaded WebView2; offline runtime is opt-in. Shared OpenRGB and source/license resources remain. Research installers, vendor applications, build outputs and tools are excluded. Corresponding source archive is verified entry by entry |

## Final regression evidence

- `../final-native-import-execution-regression.log`: **217 passed, 0 failed, 0 ignored**.
- `../brand-chooser-final-vendor-tests.log`: **289 unit + 1 documentation test passed**.
- `../final-synapse-parser-regression.log`: **148 unit + 3 integration tests passed**, including explicit public-source smoke input.
- `../browser-ui-regression-report.json`: all **13 suites passed**; source/UI code is unchanged after the last UI run.
- `../source-import-execution-tests.log`: the three new source-file/disk/native-trigger/output tests passed.
- `../installer-migration-qa-23e42286/report.json`: compiled production NSIS macro runtime fixtures passed.
- `../final-installer-lifecycle-verification.json`: actual installer lifecycle and data preservation.

External GG/iCUE research probes are opt-in tests. Their explicit input variables
are `OPEN_HUB_GG_RIVAL310_SQL` and `OPEN_HUB_ICUE_SAMPLE`; the release run set both
to existing inert research sources. Parser smoke uses
`SIDEARM_SYNAPSE_SMOKE_FILE`, set to the bundled public fixture. No vendor SQL or
proprietary executable is run. Source fixtures in `tests/fixtures` and generated
M65/GG test data are synthetic, not physical-device captures.

## Installation verification boundary

The clean installation test starts with no Open HUB program directory or uninstall
registration, while retaining the user's configuration as required. It reuses the
current Windows user and existing WebView2. A pristine Windows VM/new-user first
run and the missing-WebView2 network bootstrap were not exercised. This is an
explicit limitation, separate from the verified repeat-install requirement.

Final user artifacts and SHA256 values are recorded next to the installer in
`../../outputs/Open-HUB-0.3.0-brand-imports-verification.json`. The source archive
includes the final protocol code, tests, synthetic fixture, resources and licenses.

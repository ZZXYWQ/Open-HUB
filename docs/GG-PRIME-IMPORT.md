# Original wired Prime GG profile import

2026-10-30 development increment. Only original wired1038:182e with exact GG name `prime` is admitted. No model count increase and no claim of full OEM parity.

## Evidence

Retained official GG122 inert migration3.17.4/20200214130818_squash.sql declares device186, product272111662, name prime, bootloader272111663. The source hash and extracted configuration facts are in development/gg122-prime-import-source.json. The default settings JSON is configuration data, not executed SQL. No renderer factory for Prime was found in the retained extracted chunks or the inspected app.asar; no such factory verification is claimed. The previously hash-verified native descriptor/include chain defines CPI codes, two key tables and conversion into libsteel Feature62. Source, not a different mouse model, determines this adapter.

## Implemented path

The existing read-only, query-only SQLite preview routes this exact identity to a dedicated converter. It checks complete source fields and does not expose a partial converted candidate on unsupported or unknown data.

- batch_resolution uses1–5 consecutive CPI slots, zero-based active_level_index and uint16 codes multiplied by50; range50–18000. Unused stored slots are range checked. Ordering is preserved.
- button_and_wheel_mappings has exactly five physical and two wheel records with four key-code bytes and no_live_deploy1. Native disabled/mouse/CPI/vertical-scroll/media and single-key-plus-modifier actions convert through the exact Prime validator. Primary click stays protected; bottom CPI is fixed and is not invented as an extra source record.
- Existing strict macro reference/event conversion handles physical inputs1–5, including balanced keyboard/mouse events and supported playback rules. They remain source-scoped software macros, not native firmware macros. Wheel macro bindings are withheld until their original trigger format is verified.
- led_brightness0–256 and lighting_enable0/1 remain independent of the raw colors.
- This increment admits only lighting_config type1/settings_mask0 colorshift from the evidenced14-record colors/positions layout. Positions must be ordered percentages starting at0. It generates each RGB/relative-length record and appends the initial color as the closing record. Native initial color follows the first gradient stop, rather than the inactive stored initial_color field. Duration follows max(speed,(count+1)*33)*10. Weights follow the descriptor's truncated2.55 scaling; physical phase/quantization parity remains unverified.
- All14 original records are validated. At most14 active colors yield15 native sections; there is no truncation. Other source layouts, breathe/steady/reactive masks, movement fields and nondefault driver acceleration/aim/straightening are refused explicitly.

The brand import list and GG description identify Prime. The production editor receives the converted native gradient, supports editing, and quick save retains the edited result.

## Verification and remaining gaps

Focused Rust tests convert the actual extracted original default, check exact native gradient bytes/weights0,84,84,86 and10000 duration, simulate a physical-key macro, and withhold malformed/unknown/wrong-model candidates. Headless Edge uses the real converter DTO with mocked IPC, edits CPI400 to600 and initial color, saves and quick-saves identical data. A separate Rust test imports that actual saved profile through production storage/export/import and replays native validation/simulation. Rival3 steady and dynamic import UI regressions pass.

No physical mouse, real input or live vendor database is touched. Actual user database variants, broader OEM mode conversion, wheel trigger identities, multi-normal-key shortcut conversion, native macro library, full hardware readback, original PC synchronization and physical timing remain open. The original source's has_onboard_macros flag does not prove that Open HUB implements those macros.

Reproduce: cargo test gg_profiles::prime with OPEN_HUB_GG_PRIME_PREVIEW pointing to gg-prime-native-preview.json; npm run build; node scripts/test-gg-prime-import-ui.cjs; cargo test prime_gg_import_browser -- --ignored with OPEN_HUB_GG_PRIME_UI_PROFILES pointing to gg-prime-ui-profiles.json.

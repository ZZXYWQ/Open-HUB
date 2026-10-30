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

## Steady and breathe follow-up

Common descriptor constants independently confirm effect0, steady1, host-stream2, override4 and reactive8 masks, and disabled0/colorshift1/breathe2 types. The adapter now also admits type0/mask1 with zero active colors as an exact steady initial_color, retaining raw brightness and enable state. Authored source-schema records exercise this combination; it is not described as an observed user database.

Type2/mask0 expands each source color into color/black transitions and appends the initial color. Integer midpoint interval splitting and truncated2.55 weights follow the inspected descriptor expression. The default three stop positions0/33/66 expand to weights0/40/43/40/43/43/43; the native program has seven sections. At most seven source colors fit; eight would require17 sections and the entire candidate is withheld. Seven colors are explicitly tested to generate all15 records without truncation. The effect minimum duration retains the source mask-effect rule max(speed,(count+1)*33)*10.

Host-stream, override, reactive and invalid type/mask pairs remain refused. The adapter does not guess a trigger response from an ordinary gradient. Physical timing, integer-midpoint/quantization phase parity and actual OEM database variations are still unverified.

Six focused tests include exact steady RGB/raw37 brightness/disabled state, odd interval breathing weights, capacity boundaries, rejected masks and the existing default/macro/malformed cases. Three separately identified native converter previews enter the production React importer. Browser editing changes CPI and initial/static RGB; ordinary save and quick save are identical. All three actual saved profiles then pass independent production storage/export/import and native simulation replay. Test source IDs are distinct, matching the source database identity requirement; the UI intentionally keeps edits per profile ID. Existing single-default Prime and Rival3 import regressions remain covered.

Use OPEN_HUB_GG_PRIME_MODES_PREVIEW with cargo test gg_profiles::prime to emit the three native previews; OPEN_HUB_GG_PRIME_MODES_UI=1 node scripts/test-gg-prime-import-ui.cjs saves all three. Replay using OPEN_HUB_GG_PRIME_MODES_UI_PROFILES and cargo test prime_gg_three_modes_browser -- --ignored.

## Numeric semantics correction

Official SteelSeries GoLisp language documentation establishes integer-only truncated division and float32 arithmetic when a float participates. The retained engine contains named GoLisp quotientInts, quotientFloats, multiplyFloats and ToIntImpl symbols. The public documentation plus these symbol identities support the runtime-language interpretation; they are not claimed as a complete native-code/version proof. REA reports the Ghidra provider unconfigured, so no native decompilation result is asserted.

Corrected the percent-weight multiplication from float64 to float32. At a full100-percent interval, float64 calculation truncated to254; GoLisp float32 rounds to255 before integer conversion. All101 distances0–100 are checked against the independent rational integer result floor(255*p/100); one-color original-schema colorshift and breathe go through actual conversion and native simulation. Previously tested odd position midpoints remain integer divisions, and the default0/33/66 weight vectors do not change. Physical firmware timing remains unverified.

Sources: [official language reference](https://techblog.steelseries.com/golisp/language-ref.html), [official GoLisp source](https://github.com/SteelSeries/golisp). Local symbol locations and evidence limits are recorded in development/gg122-prime-numeric-semantics.json.

## Multi-usage keyboard chord follow-up

Original function81 four-byte records now share the existing GG keyboard compiler. One ordinary usage plus modifiers remains a native mapping. Multiple ordinary usages or modifier-only combinations become source-scoped WhileHeld software macros: original nonzero usage order, explicit WaitForRelease, reverse release order. Empty/duplicate/invalid key records still withhold the whole import. The fixed bottom CPI row and protected primary left click remain unchanged. Firmware-side macro support is not inferred from this conversion.

All six editable inputs2/3/4/5/7/8 are exercised with Ctrl+A+B, four ordinary keys, and modifier-only combinations:18 complete production import/compile/native-simulation cases. Existing original macro bindings override a direct chord without leaving an unused generated macro. A native converter preview enters the actual built importer UI; ordinary and quick save preserve all seven Ctrl/A/B hold/release steps. The actual saved JSON passes the independent production store/export/import and driver replay path.

A separate Service/actual driver/memory input runtime test configures two serial-less same-model Prime instances with different three-key held chords. Each source starts its own keys, waits for its own release, emits reverse key-ups and restores separately. All HID and input are memory-backed; this does not prove the physical device keyboard collection association. No OS input or global mouse mapping is used.

Legacy GG import UI and Rival3 import UI regressions pass after correcting a stale M HUB button name in the old test. Full main suite:1118 passed,0 failed,105 ignored; these ignored artifact/physical tests are not counted as passed. Physical held-key behavior, exact wheel pulse/hold timing, actual identity association and native onboard macros remain open.

Reproduce: OPEN_HUB_GG_PRIME_CHORD_PREVIEW plus cargo test gg_profiles emits the chord preview. OPEN_HUB_GG_PRIME_CHORD_UI=1 node scripts/test-gg-prime-import-ui.cjs saves the profile. OPEN_HUB_GG_PRIME_CHORD_UI_PROFILES plus cargo test prime_gg_chord_browser -- --ignored replays it. The source/runtime isolation test is prime_two_scoped_held_chords.

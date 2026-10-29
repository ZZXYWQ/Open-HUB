# Original wired SteelSeries Rival 3

Development only, 2026-10-29. Counts as one partially integrated model, not fully accepted. USB 1038:1824 and 1038:184C (firmware v0.37.0.0), control interface 3. Rival 3 Wireless and Gen 2 are excluded.

## Implemented

- Native CPI 200–8500, step 100, 1–5 stages with selected active stage; polling 125/250/500/1000 Hz.
- Complete native eight-slot assignment table: six physical buttons plus wheel up/down. Primary left click retained. Disabled, mouse outputs, one keyboard usage without modifiers, supported media, DPI cycle and vertical scroll outputs use exact two-byte fields.
- Per-mouse software macros on non-primary inputs use existing source identity verification and recovery journal. They require Open HUB running and a verified keyboard collection belonging to the same device. Offline simulation does not establish a physical keyboard/control association.
- Four independent native RGB/brightness values, one shared seven-mode firmware selector (rainbow shift, three breathing speeds, steady, rainbow breathing, disco). No invented millisecond period or per-zone independent pattern.
- Complete desired palette is sent before the shared mode. Static screen/music sampling explicitly switches to steady and preserves the other desired zone values. A failed or short write at any of the five packets clears lighting cache; no automatic retry. A fresh complete plan can be explicitly applied.
- Exact model drawing, Chinese lighting editor, editable DPI stages, known control rows, host macro editor, Open HUB profile export/import and native codec replay.
- Explicit onboard save only after an applied complete plan; software macro trigger mappings cannot be flashed. Only all-zone steady or all-zone disabled lighting is eligible.

## Evidence

Protocol and model data: [pinned rivalcfg original Rival 3 profile](https://github.com/flozz/rivalcfg/blob/779ea52c68cbf8d21234d9e0d81718225f680819/rivalcfg/devices/rival3.py), its independent legacy reference vectors, and the original [OEM manual](https://downloads.steelseriescdn.com/guides/Rival_3_PIG_10_MP.pdf). Native reports use a zero HID report ID; the upstream test harness report-type prefix is not transmitted as a report ID.

Onboard persistence scope: [SteelSeries official Rival 3 onboard-memory article](https://support.steelseries.com/hc/en-us/articles/16260643559053-What-settings-save-onto-my-Rival-3-s-onboard-memory). Dynamic patterns are retained in Open HUB profiles; they are not claimed as permanently saved mouse state.

Artwork: unmodified original rivalcfg SVG at the same pinned commit, 25,904 bytes, checked for external hrefs, images, script and event handlers. License copied verbatim to RIVALCFG-ARTWORK-LICENSE.txt; source and SHA256 in DEVICE-ARTWORK-SOURCES.json. A light UI backdrop preserves black annotation contrast.

## Verification and gaps

Production memory-transport tests cover exact sensor/rate/table bytes, all seven lighting modes for both firmware identities, invalid foreign schemas and native modifiers, each lighting packet failing short with no retry, cache invalidation and explicit recovery, and all non-primary inputs with independent host macros. Shared wired tests cover two identical serial-less devices, wrong-source refusal, stopping one macro source without stopping the other, and reopened-device journal preservation. Persistence tests cover every mode with enabled/disabled combinations.

Headless Edge runs the actual production built UI using native DTOs, edits five CPI stages and all four zones, selects seven modes and saves each firmware profile. Browser IPC is mocked and does not prove driver success. A separate Rust test imports those saved profiles through the production profile transfer/store path and replays the native compiler. No HID or OS input is used.

Still missing: physical device tests; complete hardware settings readback; verified original GG profile parser/migration for this model; native macro library parity and OEM advanced behaviors not established by the public profile. Initial values are templates/session cache, never observed hardware state. These gaps prevent full acceptance under the 100-model goal.

Reproduce: npm run build; node scripts/test-rival3-ui.cjs after generating ../outputs/100-model-development/rival3-native-views.json with the Rust DTO test using OPEN_HUB_RIVAL3_VIEWS. Replay with OPEN_HUB_RIVAL3_UI_PROFILES pointing to the browser output and cargo test rival3_browser_saved_profiles -- --ignored. Full validation uses the main Cargo manifest and the vendored gearwise workspace manifest.

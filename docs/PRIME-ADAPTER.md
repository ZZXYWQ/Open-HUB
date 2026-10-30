# Original wired SteelSeries Prime

Development adapter, 2026-10-30. Only USB 1038:182e is admitted; Prime+, wireless versions and bootloader182f are excluded. This is partial integration, not complete OEM parity.

## Implemented

- Native 50–18000 CPI in steps of50, one to five stages and selected active stage;125/250/500/1000Hz.
- Separate physical5b and wheel5c assignment tables. Five top physical buttons and wheel up/down are represented; bottom CPI remains its fixed firmware action. Application primary-left protection leaves six editable inputs2,3,4,5,7,8. Keyboard modifiers, supported mouse/media/scroll/DPI actions use the shared native encoder.
- Independent per-mouse software macros on all six editable inputs use the existing verified input-source association. They require Open HUB running; no global mouse hook or claim of native macro playback.
- Native static scroll-wheel RGB and authored1–16-section gradient programs, separate raw brightness0–256 and enable/off. Color remains unchanged when brightness changes. Feature reports are643 bytes; output reports65 bytes; real transport enforces at least30ms command spacing. No implicit flash write.
- Complete desired lighting and both key tables support explicit reapplication after failure. Partial failure invalidates the relevant cache and stops sending; uncertain tables cannot be incrementally edited until a full table is applied.
- Explicit save59/00 requires a complete applied plan for the same current connection. Stale/repeated saves and software macro plans are refused. A sent command is not reported as verified hardware persistence.
- Exact licensed model artwork, five-stage DPI editor, fixed/writable controls, dedicated lighting editor and Open HUB profile transfer/store roundtrip.

## Evidence and validation

[Static OEM findings](development/gg122-prime-native-facts.json) record GG122 descriptor/include hashes, MDC integrity and native layout. OEM Lisp, SQL and executable code were not executed. The independently retained rivalcfg Prime profile provides a second implementation reference. The unmodified29,596-byte SVG has source/hash attribution in DEVICE-ARTWORK-SOURCES.json and the existing WTFPL license in RIVALCFG-ARTWORK-LICENSE.txt.

Seven focused Rust tests, including the actual browser-saved profile replay, passed. They cover exact bytes, all six software-macro controls, same-model device isolation, raw brightness bounds, foreign/malformed schemas before I/O, each lighting/table failure, explicit recovery and one-shot connection-scoped flash. Headless Edge edits and saves two complete production React profiles, including Ctrl+C, wheel macro, static/off and37/256 brightness. Rust then imports them through production storage and replays the native compiler. Browser IPC and transport are simulated, so these checks do not establish physical compatibility.

Reproduce: npm run build; generate native DTO using OPEN_HUB_PRIME_VIEWS and cargo test prime_exact_model_template; node scripts/test-prime-ui.cjs; set OPEN_HUB_PRIME_UI_PROFILES to the resulting prime-ui-profiles.json and cargo test prime_ -- --include-ignored.

## Remaining OEM parity work

Exact OEM dynamic conversion and verified motion/timing, plus complete Prism/PC synchronization; onboard macro memory and playback; full readback and persistence verification; complete original GG private profile import (strict steady/colorshift/breathe subsets are connected); other advanced OEM settings and real-device validation. Initial UI values are templates/session cache, not hardware observations. These gaps keep this model out of complete acceptance.

Rival110 was also investigated, but its OEM ordinary button/wheel mapping relies on a kernel path rather than this native table protocol. [Research gap](development/2026-10-29-rival110-protocol-gap.json) records this; it has no newly admitted device entry and is not counted.

Follow-up: [native gradient codec and source conversion limits](PRIME-NATIVE-GRADIENT-PREPARATION.md). Static lighting shares the reviewed codec; dynamic schema/UI are connected for authored programs, with original GG conversion and physical timing still pending.

[GG original-profile migration follow-up](GG-PRIME-IMPORT.md) connects the evidenced default colorshift and authored steady/breathe schema variants, plus physical/wheel software macro subset; unverified modes are withheld.

[Onboard macro header research](PRIME-ONBOARD-MACRO-RESEARCH.md) pins native encoder/metadata evidence, canonical event/block codecs and an offline transfer planner. Original firmware read/write, binding and scheduler instructions now pass isolated emulation with525 compiled one-shot programs. A bounded offline once compiler is implemented; complete playback modes, ownership/readback, concurrency admission and safe live upload remain incomplete.

[Macro storage/recovery follow-up](PRIME-MACRO-STORAGE.md) adds offline exclusive replacement and complete foreign-byte preservation, including interrupted restoration history. Actual transport readback, persistent journals, allocation and guarded application/UI integration remain incomplete.

[Feature readback follow-up](PRIME-FEATURE-READBACK.md) adds bounded read codecs and original USB-class/cache instruction tests. Untagged stale or intermediate cache data is explicitly unverified and cannot establish current identity, idle state or safe upload.

[Output/interrupt reader follow-up](PRIME-OUTPUT-READBACK.md) adds challenge-fenced candidates, exact data preservation across notification/echo collisions and original-firmware interoperation. Physical transfer mapping, stable identity/images, idle state and safe application/UI integration remain incomplete.

[Scoped macro-image collector](PRIME-IMAGE-SESSION.md) adds two equal full macro/table observations with per-I/O operation checks and original-firmware rejection tests. It is not a complete device profile, physical-identity/atomicity proof or live-upload authorization.

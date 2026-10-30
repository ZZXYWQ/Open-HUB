# Prime native gradient codec preparation

2026-10-30. Existing wired Prime only; no increase in partial or complete model counts.

Static inspection of the same GG122 include chain confirms an80-byte native program: section count, initial RGB, little-endian duration and initial tick, four movement bytes, and sixteen RGB/weight records. The HID Feature report includes report ID0 and command62, padded to643 bytes. Source hashes remain in gg122-prime-native-facts.json. No OEM code is executed.

Implemented a typed native Program/Section/Motion codec with checked section capacity and duration domain, zeroed initial tick and reserved records. Existing production static lighting now uses this codec and preserves its exact bytes. Three additional tests cover static equivalence, all16 records and motion/duration offsets, invalid counts/durations, zero-duration static restrictions and zero-weight handling. This is native encoding groundwork; dynamic UI, application schema and original-profile conversion remain unimplemented.

OEM conversion needs more work before admission: colorshift appends a closing initial-color record; breathe expands stops with black transitions and a closing record. The declared16 original colors can exceed the actual16-record output capacity after expansion. We must reject or explicitly represent that mismatch, never silently truncate. Position quantization and minimum-speed evaluation still need independent vectors. Movement fields are encoded but their physical response is unverified.

Readback structures DD/DF/EC return values after report ID without echoed commands. The current generic command-recognition query cannot establish correlation for these replies. No readback is enabled and template/session values remain labelled accordingly.

Temporary decoded sources were hash-checked against the retained facts and removed after research. No installer or release is produced.

## Application and UI integration follow-up

The existing strict Prime profile now accepts an optional typed `program`; absent fields retain previous static behavior and serialization. Nested program/section/motion fields reject unknown fields and RGB extensions; bounds are checked before I/O. Enabled dynamic programs clear the generic static-lighting projection instead of pretending to report a steady color. An explicit generic static write removes the program; disable preserves it. Full native plans use Feature62 plus independent brightness/enable and keep existing failure invalidation.

Production UI now edits initial RGB,1–16 section colors and weights, native duration and experimental movement bytes. Authored gradient and two-section color/black breathing presets are provided; these do not claim exact GG conversion or physical phase equivalence. Program state is retained by brightness and on/off changes. Dynamic data is saved in Open HUB profiles. Hardware flash explicitly refuses a program until native persistence is verified.

Headless Edge saves two complete dynamic profiles, including three section colors/weights, duration12340 and motion7/8/9, with enabled/disabled and wheel-macro cases. A separate ignored Rust integration test imports their actual saved JSON through production storage and replays the native codec. Native tests assert exact program bytes and two-device isolation, static-switch cache behavior and nested invalid-data refusal. IPC/transport remain simulated; physical timing and motion remain unverified. Original GG conversion, full readback, PC synchronization policies and onboard parity still need implementation.

Reproduce: OPEN_HUB_PRIME_DYNAMIC_UI=1 node scripts/test-prime-ui.cjs after the normal build/DTO setup; OPEN_HUB_PRIME_DYNAMIC_UI_PROFILES points to prime-dynamic-ui-profiles.json for cargo test prime_dynamic_browser -- --ignored. Normal Prime UI tests remain in the same script without the flag.

# Prime direct-media and macro frame capacity

2026-11-07. Exact original wired Prime1038:182e, pinned firmware1.0.15+4244c08. Development capacity analysis only; no application upload or UI capability is enabled. Counts remain44 partial/0 complete.

`prime_macro_concurrency::analyze_complete_tables` now reads the complete physical and wheel mapping tables to compute direct-media headroom, then composes it with the existing macro peak/context bound. The original ordinary binding function at0800675e..080067f2 handles function61 by appending its byte1 consumer usage to frame bytes40..43. It skips a usage already present and stops when the four entries are occupied. The pinned function digest is added to the original firmware verifier's playback regions. Physical switch scanning and the whole main loop are not executed by the new test.

Direct bindings with the same consumer usage are therefore counted once across the supplied tables. Macro contexts still consume separate merged entries even when their bytecode is aliased. The resulting reservation includes direct unique usages plus the caller's explicit budget for other producers; macro peaks are added separately. More than four combined entries refuses capacity admission. The result includes the direct usage list, making that reservation inspectable. A direct61 usage of zero contributes no occupied entry. Its trailing three bytes are ignored by the reviewed native function and are preserved rather than interpreted or rewritten. Keyboard51 and reviewed mouse/DPI/scroll functions contribute no consumer entries; macro71 references are examined by the existing program analyzer. Unknown direct function codes refuse this stronger admission check.

The table-wide budget conservatively assumes the configured direct media actions may overlap. It does not infer physical restrictions that might make some combinations unreachable. Arbitrary additional producers, real current input state, current physical identity, I/O exclusivity and whole-runtime coverage still need caller admission. This is not proof that an actual device is connected or safe for a macro write.

## Original-instruction evidence

The production Rust fixture covers macro consumer peaks0..4 combined with zero through five distinct ordinary media records, plus five cases with repeated ordinary usage records. The actual original ordinary binding function populates a fresh modeled52-byte frame before the original macro scheduler/merger runs over that same frame. The ordinary records use nonzero ignored trailing bytes, so preservation is not assumed from zero-padded defaults. They are held synthetically for30 scheduler calls, then omitted to model release. No physical switch, OS input, USB transfer or full input polling loop is involved.

All35 configurations pass over1,400 scheduler iterations. All19 admitted configurations retain every expected consumer entry. All16 refused configurations actually truncate consumer output. Equal ordinary usages deduplicate as predicted, and the final frame has no consumer output or active macro context. The tests compare captured usage bytes, not just an analyzer success flag. This closes the previously unexecuted direct-media contribution case while retaining the earlier126 macro-overlap tests as separate evidence.

All152 driver tests pass, including complete-table media deduplication, reserved bytes, unknown function rejection and composition with extra reservations. All20 main Prime tests pass; six artifact-dependent tests remain ignored. [Results](development/2026-11-07-prime-direct-consumer-emulation.json).

```text
cargo run --quiet --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries --example prime_direct_consumer_fixture > <fixture.json>
python scripts/test-prime-direct-consumer-emulation.py <pinned-firmware.bin> --fixture <fixture.json> --report <report.json>
```

Remaining: full physical input/polling path and non-table contributors, mode5/OEM conversion, allocation and copy-on-write, guarded per-device application/UI integration, actual Windows transfer behavior, timing/input release and flash persistence. Unicorn, retained vendor firmware and development fixtures are not bundled with the application.

# Prime explicit macro playback policies

2026-11-07. Exact retained wired Prime1038:182e firmware1.0.15+4244c08. Development compiler/storage/journal APIs only; no application upload/UI capability, new model or installer is admitted.

`prime_macro_program::compile` accepts explicit once, counted, while-held and toggle policies, plus press/release direction. Counted playback admits1..255 runs and a raw16-bit inter-cycle scheduler interval. Zero is rejected: original firmware decrements its byte counter and zero wraps to256 plays. While-held playback requires press direction; starting it on release is not equivalent to an ordinary held loop. Once, counted and toggle can use either reviewed direction. Native mode5 is deliberately not mapped to a guessed application meaning; it remains a gap in full OEM parity.

The common once compiler still enforces canonical supported events, a balanced body, bounded transferred-prefix size and at most four held consumer usages per context. A balanced loop body does not prove the shared four-slot merged consumer queue is safe across concurrent contexts. Delays and repeat intervals remain scheduler counts, not verified milliseconds. The original allocator records source-held state separately from the matching-direction toggle flag. The scheduler can continue to merge held outputs while an already executing Delay record counts down. Therefore cancellation on release or the next toggle is not promised to produce immediate physical input release.

`replace_program_in_place` applies these explicit policies to an existing exclusive macro allocation while retaining all previous scope, extent, prefix, protected-control and foreign-byte preservation checks. The original `replace_once_in_place` wrapper remains press-once. No free allocator, first-time binding allocation or alias copy-on-write is introduced. The new journal `create_program` retains playback/direction and reconstructs the exact policy on reopen. Its original `create` remains press-once. Default policy fields are omitted from canonical serialization so the prior checksummed version1 records remain readable without rewriting them. A policy is never silently converted to press-once during recovery.

## Original-firmware validation

The Rust `prime_macro_program_fixture` emits87 production compiler/storage plans targeting an existing allocation across the512-byte transfer boundary. A foreign mode6/opaque-event block beyond the transferred prefix remains exact. The Python harness writes the actual two production Feature payloads through the pinned original macro write routine, then executes original binding, scheduler and merge instructions in isolated RAM. Full5120-byte before/after images match exactly outside the selected existing extent.

Coverage includes six once cases,72 counted cases (counts1,2,3,255; intervals0,1,7; body delays0,1,3; both directions), three held cases and six toggle cases. It checks exact key-down start times and expected held/released frames for all finite runs. Held/toggle tests cover126 cancellation boundaries; opposite-direction toggle events do not stop playback. In total39,982 scheduler iterations pass. Raw contexts and synthetic output buffers are observed; no HID, physical switch input, OS key event, actual wall-clock duration or device firmware boot is validated.

All148 SteelSeries tests pass, including common rejection paths and journal policy persistence/reopen. The five retained original once-journal scenarios also open with the new implementation and preserve their committed/restored/pending phases. All20 main Prime tests pass; six artifact tests remain ignored. A fresh fixture emitted from the final Rust sources is byte-identical to the fixture executed by the firmware harness. [Machine-readable firmware results](development/2026-11-07-prime-program-emulation.json).

```text
cargo run --quiet --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries --example prime_macro_program_fixture > <fixture.json>
python scripts/test-prime-program-emulation.py <pinned-firmware.bin> --fixture <fixture.json> --report <report.json>
cargo run --quiet --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries --example prime_macro_program_fixture -- --check-journals <retained-journal-root>
```

Remaining: native mode5 semantics/OEM conversion, verified physical timing/release, per-device concurrent consumer admission, full allocation/binding ownership, real connection/input/exclusive-I/O integration, guarded application/UI routing and flash persistence. Counts remain44 partial/0 complete.

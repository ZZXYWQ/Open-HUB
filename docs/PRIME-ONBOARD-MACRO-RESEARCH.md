# Prime onboard macro header research

2026-10-30. Structural reader only; native firmware macros are not enabled and this does not increase accepted model counts.

The retained official GG122 engine was inspected without execution using Go1.20 pclntab function metadata, Go reflection type records and GNU objdump. No Ghidra/decompiler availability is claimed. The metadata helper at1404e4800 and encoder at1404e37c0 are explicitly named in pclntab; their complete body digests, six field descriptors and instruction anchors are pinned in development/gg122-prime-macro-header-source.json.

The Go type is8 bytes: magic u8@0, playbackType u8@1, length u16@2, direction u8@4, numRepeats u8@5, repeatDelay u16@6. Machine instructions confirm little-endian stores/loads. The encoder initializes marker77 and branches from source numeric playback0–4 to native1–5. That numeric relationship is not yet claimed to prove every host playback meaning or firmware behavior.

The parser compares marker77, native playback1–5, and offset+8+4*length strictly less than the available memory length. Encoder instructions independently use a four-byte record count. The extra available byte is not asserted to be a terminator: its presence is established, its value is not. Zero-length headers and raw direction/repeat fields are preserved as structural data; this is not approval to execute them.

Implemented a read-only prime_macro::inspect helper. It reports typed header fields and opaque event byte ranges, rejects every header/event truncation and exact-end boundary, unknown markers/codes and overflowing offsets. No event payload decoder, event generator, HID write, automatic device read, schema/UI switch or native-macro capability is enabled. Existing source-scoped software macros remain the application path.

Tests:101 SteelSeries driver unit tests pass, including three new header/bounds tests.20 main Prime regression tests pass with6 artifact/physical tests ignored. The source verifier checks the engine digest, both complete Go function bounds/digests, runtime type size/fields and four machine-code anchors without vendor execution.

Remaining work: reconstruct four-byte event semantics and allocation/binding offsets; verify the1280-byte memory versus512-byte chunk tail; prove preservation/readback of existing slots; implement explicit failure/recovery transactions and device-scoped persistence; connect UI and actual hardware validation. Do not upload fabricated records to bridge these gaps.

Reproduce: python scripts/verify-gg-prime-macro-header-source.py <SteelSeriesEngine.exe>; cargo test --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries; cargo test --manifest-path src-tauri/Cargo.toml prime_.

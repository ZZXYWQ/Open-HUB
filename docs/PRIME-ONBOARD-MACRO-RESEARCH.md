# Prime onboard macro header research

2026-10-30. Structural reader only; native firmware macros are not enabled and this does not increase accepted model counts.

The retained official GG122 engine was inspected without execution using Go1.20 pclntab function metadata, Go reflection type records and GNU objdump. No Ghidra/decompiler availability is claimed. The metadata helper at1404e4800 and encoder at1404e37c0 are explicitly named in pclntab; their complete body digests, six field descriptors and instruction anchors are pinned in development/gg122-prime-macro-header-source.json.

The Go type is8 bytes: magic u8@0, playbackType u8@1, length u16@2, direction u8@4, numRepeats u8@5, repeatDelay u16@6. Machine instructions confirm little-endian stores/loads. The encoder initializes marker77 and branches from source numeric playback0–4 to native1–5. That numeric relationship is not yet claimed to prove every host playback meaning or firmware behavior.

The parser compares marker77, native playback1–5, and offset+8+4*length strictly less than the available memory length. Encoder instructions independently use a four-byte record count. The extra available byte is not asserted to be a terminator: its presence is established, its value is not. Zero-length headers and raw direction/repeat fields are preserved as structural data; this is not approval to execute them.

Implemented a read-only prime_macro::inspect helper. It reports typed header fields and opaque event byte ranges, rejects every header/event truncation and exact-end boundary, unknown markers/codes and overflowing offsets. No event payload decoder, event generator, HID write, automatic device read, schema/UI switch or native-macro capability is enabled. Existing source-scoped software macros remain the application path.

Tests:101 SteelSeries driver unit tests pass, including three new header/bounds tests.20 main Prime regression tests pass with6 artifact/physical tests ignored. The source verifier checks the engine digest, both complete Go function bounds/digests, runtime type size/fields and four machine-code anchors without vendor execution.

Remaining work: reconstruct four-byte event semantics and allocation/binding offsets; verify the1280-byte memory versus512-byte chunk tail; prove preservation/readback of existing slots; implement explicit failure/recovery transactions and device-scoped persistence; connect UI and actual hardware validation. Do not upload fabricated records to bridge these gaps.

Reproduce: python scripts/verify-gg-prime-macro-header-source.py <SteelSeriesEngine.exe>; cargo test --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries; cargo test --manifest-path src-tauri/Cargo.toml prime_.

## Canonical event-record follow-up

Static encoder instructions and reverse-parser instructions now establish a canonical record subset. Keyboard records are [2,usage8,page1-or12,state0-or1]; the source keyboard page7 is normalized to native1 by its encoder. Mouse button1–8 uses [0,single-bit-mask,0,state]. Delay uses [4,0,low8,high8]. Native encoder loops split unsigned long delays at65535 and subtract65535 before continuing. These are observed byte operations, not an assertion about physical delay timing.

Added typed encode/decode for this exact subset plus bounded long-delay splitting. The record budget and uint16 header-count capacity are checked before allocation. The codec refuses unknown pages/types, noncanonical reserved fields, multiple-button masks, invalid state flags and truncated/extra-length records. It retains explicit zero delays and does not merge or rewrite arbitrary records. Existing inspect() still exposes opaque event ranges for unknown data.

Three new tests cover1020 keyboard combinations,16 mouse states, every65536 possible16-bit delay, original instruction-derived golden records, long-delay boundaries and budget failures. SteelSeries driver tests:104 passed/0 failed. Main Prime regressions:20 passed/0 failed,6 ignored. The extended verifier checks three complete Go function ranges/digests, six reflection fields and ten instruction anchors. Reproduce with the existing verifier's --events flag.

Not implemented: whole-macro event/hold balance validation, higher-level source transformations, scroll/pan/system events, conversion into application native-macro capability, library slot allocation and binding-address ownership, actual GET/SET transactions, existing-memory preservation/recovery, or persistence and physical playback. Consumer-page down/up records are a raw protocol representation; they are not wired to the application's atomic MediaTap as if parity were proven. The1280/512 tail question remains open. No device I/O or installer is enabled.

## Offline blocks and native binding offsets

Go reflection independently identifies parsedSettingsMapping as HIDCode u16@0, Function u8@2 and KeyCodes[4]@3. The encoder stores function71, two reserved zero key bytes, and little-endian offset/4 in the final two key bytes. A signed divide-by-four sequence precedes those stores; the typed codec additionally refuses unaligned offsets rather than silently rounding them. The converter's binding value uses this exact reflected type. Source Event fields are also pinned as Function@0, Page@1, Code u16@2, Data@4, Timestamp@8 in a12-byte Go type.

Added prime_macro_block::Block for offline canonical macro images. Decode composes the strict header inspector with canonical event decoding. Encode retains original raw direction/repeat/delay fields and verifies count/playback consistency. An explicit input-balance check separately rejects duplicate presses, unmatched releases, empty bodies and held inputs left at the end; keyboard and consumer usages with the same numeric code have separate ownership. This balance check does not approve undocumented playback/direction combinations or substitute for full firmware semantic validation.

Raw function71 binding encode/decode uses exact five-byte mappings [71,0,0,offset_words_low,offset_words_high]. Every65536 possible uint16 word address roundtrips. This is an address-format check, not admission of a262144-byte device memory: actual memory-size/slot bounds remain a separate future planner requirement. No allocation strategy or device writes are enabled.

Three new tests cover a complete28-byte mixed keyboard/mouse/delay golden macro, offset reads in a padded memory image, neighboring bytes, raw header preservation, count mismatch, every truncation/exact-end boundary, unknown records, input imbalance, separate usage pages and word-address bounds.107 SteelSeries tests and20 main Prime regressions pass;6 artifact tests remain ignored.

The extended source verifier's --blocks flag checks four complete Go function ranges/digests, six header fields plus eight auxiliary mapping/event fields, and13 machine-instruction anchors. The source data is development/gg122-prime-macro-block-source.json. The initial tool approval timeout was resolved; these checks were actually run. Macro library allocation, existing-memory preservation and transactions, native mode semantics,1280/512 tail, real GET/SET/readback and hardware playback remain incomplete.

## Pinned transfer-loop coverage

The engine's DefChunkLoop retains its count expression; ApiCommand.SerializePayload evaluates an integer count, initializes the iteration counter to0, increments by1 and exits when counter>=count. The pinned GoLisp quotientInts contains native signed division. With the reviewed descriptor's1280/512 integer expression this establishes two iterations, rather than an inclusive third iteration. Function ranges/digests and instruction anchors are in gg122-prime-macro-coverage-source.json and verified with --coverage.

The descriptor's two Feature5e commands each declare128 four-byte words at offsets0 and128 words. Thus the currently reviewed command plan covers the first1024 declared data bytes. It does not establish what the last256 bytes represent, nor does this prove the complete physical macro capacity is1024. Firmware interpretation of padding/count and readback remain unverified. Open HUB's capability/UI is not changed to advertise a smaller or complete native macro implementation.

Added an offline coverage planner that accepts complete1280-byte before/desired images and an explicitly supplied macro-reference set. It refuses any tail change and any referenced canonical block extending past1024, checks alignment and input balance, then emits the two exact643-byte Feature frames (including report ID). The images are not mutated. A block ending exactly at1024 is covered; the existing tail byte needed by the structural parser remains unchanged. Malformed or unsupported blocks withhold the plan.

This planner is not an allocator, snapshot identity proof or complete binding ownership check. Its caller must independently prove the reference set is complete, preserve unselected slots, validate native mode/direction semantics and implement explicit failure/readback/recovery before any device write. No caller performs device I/O with this plan yet.

Three additional tests cover full frame headers/word offsets/counts, payload/padding, unchanged source images, tail mutation, valid tail references that are still untransferred, malformed images/blocks and exact boundary cases.110 SteelSeries driver tests and20 main Prime regressions pass;6 artifact tests remain ignored. The extended verifier pins seven function digests and18 machine-code anchors, alongside prior type-layout evidence. No source code from the vendor is executed.

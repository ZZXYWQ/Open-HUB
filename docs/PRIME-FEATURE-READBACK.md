# Prime feature readback and cached-response evidence

2026-10-30. Exact retained wired Prime1038:182e firmware1.0.15+4244c08 only. Read request/reply codec and original-instruction emulation; no live reader, stable snapshot, native macro upload or hardware parity is claimed.

## Read layout established

Original command0x83 reads a byte window based at20000bd8. Raw request+2 holds a little-endian16-bit BYTE offset and+4 a BYTE count; this differs from the four-byte-word fields of the native macro command0xde/0x5e. The read handler clamps the byte count to its supplied reply capacity and copies to the reply buffer. Command0x90 copies the64-byte zero-padded firmware version string from0800c3b3. The application codec emits only these two known read opcodes; it does not expose command0x03's write direction or arbitrary addresses.

`prime_feature_read::Read` supplies bounded reads of physical30 bytes at1cb6 (window offset10de), wheel10 bytes at1cd4 (10fc), macro contexts180 bytes at1c00 (1028), and ten512-byte chunks of the observed macro RAM1e73..3273 (starting offset129b). Together with the version read this is a14-request observation recipe. It does not allocate5120 bytes of macro capacity or widen the existing1024-byte host transfer budget. Zero, oversized and overflowing macro ranges are rejected before any command can be produced.

The generated buffer is643 bytes including HIDAPI's leading report ID0. The modeled USB payload is the remaining642 bytes. Raw read offset/count consequently occupy HIDAPI buffer positions3..4/5..6. The actual Windows/HIDAPI transfer boundary still requires capture/physical validation. `Read::report()` is already a complete feature buffer; it must not be passed unchanged to a wrapper that adds another report ID.

## Original control/cache path

Pinned official initialized callbacks select class init0800884a, setup080086f8, RX completion080086d0 and command staging08008b08. Original initialization installs class context20000fd8; the handle is200009b4, core base20000310. SET_REPORT type3 prepares a buffer of at most642 bytes; RX completion stages the request at200014fc, with pending length20001900. The main loop later dispatches it. Feature responses are written to class+284, cache2000125c. GET_REPORT returns the requested number of bytes from that cache, not an acknowledged response associated with a request identifier.

The whole cache span125c..14de numerically overlaps the1408 global base used by other firmware code. These isolated slices do not establish all full-runtime aliasing/lifetime rules. No inference that this is a firmware bug or safe concurrent live snapshot is made.

The emulator executes these exact original functions, main dispatch/finish, raw/version handlers, memcpy and string padding. Only endpoint-open, control-transmit, control-receive and stall primitives are intercepted to model a bus boundary. No boot, MMIO/peripheral execution, real USB, OS input or device access is supplied. Source/digests, callback pointers and RAM relationships are pinned in development/gg122-prime-feature-read-source.json and checked with verify-prime-firmware-macro-source.py --feature. Base, playback and feature evidence together verify30 pinned region digests using --playback --feature.

## Freshness limitations now reproduced

Every one of the14 SET requests can be followed by a syntactically valid old GET cache before main-loop processing. After the original handler runs, the requested bytes match the seeded version/tables/contexts/macro memory, while all bytes after the returned data remain old cache. The known configuration/context bytes1c00..1e73 and the entire macro range remain unchanged by these emulated reads; other runtime/physical effects are not established.

A rejected macro request, unknown command and zero-length raw read each leave the old cached reply intact. A second staged request overwrites an unprocessed first request because there is a single pending slot. GET supports requested lengths0/1/63/64/512/642 without tying that length to the preceding handler's returned data. SET length643 stalls; GET length643 returns from setup without a transmit call. A successful-looking GET is therefore not a SET acknowledgement.

Pausing the ORIGINAL memcpy after16 completed stores reaches a cache containing16 new bytes followed by old cached bytes. That mixed state has the same syntactic feature format as a complete result. This demonstrates a reachable intermediate RAM cache, not physical USB interrupt/DMA timing. No claim that a real transfer was observed during that copy is made. A first changed byte, fixed delay or two identical reads alone has not been proved to establish complete fresh processing.

The codec intentionally returns `CachedBytes`, whose `freshness_verified()` remains false, including when the cached version exactly matches the pinned build. Matching cached version is not live firmware/device identity proof. Short/oversized buffers and wrong report IDs are rejected. No conversion to a trusted storage image, current idle proof or write authorization is supplied. The application/service has no caller of this module; native macro upload remains disabled.

## Verification and next work

Rust generates the14 requests. Original firmware execution produces33 observations:14 old caches,14 post-handler caches, one mid-copy cache, three rejected/empty request caches and one overwritten pending request. These are fed BACK into the Rust production parser; all33 remain explicitly unverified for freshness. Three codec tests cover exact byte offsets/counts, full observation bounds and cached-version/error semantics. All124 SteelSeries tests and20 main Prime regressions pass;6 artifact tests remain ignored. Report: development/2026-10-30-prime-feature-cache-emulation.json.

```text
cargo run --quiet --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries --example prime_feature_read_fixture > <fixture.json>
python scripts/test-prime-feature-cache-emulation.py <firmware.bin> --fixture <fixture.json> --observations <observations.json> --report <report.json>
cargo run --quiet --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries --example prime_feature_read_fixture -- --check-observations <observations.json>
```

Development-only Unicorn2.1.4 is installed separately, not bundled. No vendor binary, emulator package or generated test data was added to application resources; no new runtime dependency was introduced. Model counts remain44 partial/0 complete. No installer or release was generated.

Remaining: independently establish a completed, ordered, fresh response protocol (including output/interrupt replies, notifications and pending-request races), stable full-image collection, actual firmware/current-device identity, idle/exclusive access, persistent edit/recovery journals, allocator/copy-on-write and whole-plan macro concurrency, then guarded application/store/UI integration and hardware verification. Cached observations must not bypass those requirements.


## Output/interrupt follow-up

[Ordered output read candidates](PRIME-OUTPUT-READBACK.md) adds a production transport reader with challenge completion fences, notification-safe narrow reads and explicit failure bounds. Actual Rust transport calls run against original firmware instructions in eleven modeled FIFO scenarios;130 SteelSeries tests and20 main Prime regressions pass. This does not upgrade cached Feature observations or establish physical identity/atomic images, and no live macro upload is enabled.

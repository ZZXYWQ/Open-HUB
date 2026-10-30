# Prime ordered output/interrupt read candidates

2026-10-30. Exact retained wired Prime1038:182e firmware1.0.15+4244c08. The production Rust reader is now exercised through original ARM firmware with a modeled USB FIFO. Physical Windows HID mapping, current device identity, complete runtime/IRQ behavior and atomic images remain unverified. Native macro upload is still disabled.

## Original protocol facts

The pinned interface0 HID descriptor declares unnumbered64-byte input and output payloads and642-byte feature payloads. Its initialized RAM descriptor at20000294 is traced to the retained firmware data image and parsed independently by the source verifier. Output buffers use HIDAPI's leading ID0 (65 bytes); input payloads in this model are64 bytes without that wrapper. The modeled host output transfer uses original SET_REPORT type2; the real Windows `HidTransport::write` to USB mapping still needs capture/physical confirmation.

Unlike the cached Feature path, a command's interrupt reply is sent after its read handler returns. The main loop first zeroes its64-byte temporary reply. A raw0x83 read therefore yields its requested1..64 bytes followed by zeros. The send wrapper at08007f80 checks configured state and per-endpoint busy state, prepares the original LL descriptor and invokes transmission. When busy it returns2 without sending; when unconfigured it returns0 without sending. The command loop clears its pending request and ignores that send result. Host output-write success cannot therefore establish delivery.

Two reviewed unsolicited payload families share endpoint81: AA55F0 for the DPI notification and AA55FF for button/scroll state. Their ORIGINAL formatting and send slices are executed by the test harness, not constructed as Python replacement reports. Other reviewed host-action reports use endpoint83 and other HID collections. Pinned known direct endpoint81 call sites are recorded; this is not a claim of a complete physical-runtime or arbitrary-firmware notification model.

The raw read window can also read its own staged request payload at20001502, offset092a relative to20000bd8. A read of32 bytes there echoes the request's32 payload bytes into a zero-padded64-byte completed interrupt reply. This is request scratch, not a configuration/macro-memory write. Descriptor, sender, completion, notification and reply-tail evidence is in development/gg122-prime-output-read-source.json. All evidence flags together verify39 pinned region digests plus callback/report-layout facts.

## Reader behavior

`prime_output_read::Reader<T>` owns one `HidTransport`. Every exchange first sends a32-byte challenge consisting of a supplied128-bit session seed and an incrementing128-bit query counter. It drains old replies until the exact challenge plus zero padding is received, then sends the actual read. Counter exhaustion, empty/notification-prefixed seeds, short writes, bad input sizes, timeout and report-budget exhaustion refuse a result. Each phase has a32-report limit and each receive has a200ms timeout. Writes retain the30ms Prime command gap. No automatic device selection, reopen or retry after failure is implemented.

The seed MUST be unpredictable and fresh for each real Reader lifetime, including reconnects/restarts; the module does not generate entropy. Reusing a seed can admit an old matching fence. Fixture seeds are deterministic test data and do not validate caller entropy. Echoing a challenge is not device authentication. FIFO ordering, sole ownership/consumption of the collection and absence of another command writer are assumptions requiring real integration checks. Another app or a second internal reader must not compete for this interface.

Replies matching the two known notification headers are ambiguous for a wide raw read: legitimate stored bytes can start with those exact headers. The reader restarts that segment using independently fenced two-byte reads instead of discarding the data. A canonical two-byte reply has zeros from byte2 onward; both known notification types have a nonzero byte2, so they can be skipped without confusing them with data. A legitimate64-byte value equal to the entire current echo reply also triggers this fallback and is reconstructed exactly. Version replies are checked against the pinned build string. Nonzero padding or an unresolved short/version ambiguity refuses a result.

The bounded `Read::subrange` and `output_report` helpers preserve their parent's approved range, emit complete65-byte buffers and refuse oversized single output reads. Larger requests are split into64-byte segments. The full14-request observation recipe involves86 challenge/data exchanges in the ordinary modeled case. Multiple segments/requests are not atomic, and live contexts/settings could change between them. `Candidate` deliberately does NOT claim physical identity or atomic-image verification. It is not a trusted storage image, an idle-input proof, a hardware-ready flag or macro-write authorization. The application/service has no caller of this reader yet.

## Original-firmware interoperation tests

The actual Rust Reader runs in a separate example executable over a JSON-lines `HidTransport`; Python relays its writes/reads to the ARM emulator. Original class setup/RX staging, main dispatch/read handlers, full send-wrapper/LL descriptor preparation and endpoint-completion callback run. Only peripheral primitives and a FIFO delivery/completion boundary are modeled. Payloads are frozen at that boundary, and completion is modeled after the sender returns. This is not physical DMA, IRQ or host-buffer timing validation. Notification tests invoke the original formatting/send slices; they do not simulate actual hardware button/DPI input conditions.

Eleven scenarios pass: the complete14-read recipe; real data with notification-header bytes; real data identical to the first echo reply; an old queued command reply; both queued notification families; notifications preceding a target reply with an abandoned wide reply drained by the next fence; busy-target drop; unconfigured sender; injected truncated input; injected short write; and32 notifications exhausting the fence budget. Six scenarios return20 exact read candidates in total, and five deliberately fail without a candidate. Native bus-model transmission count is411. Configuration/context and macro bytes1c00..3273 remain exact in all scenarios. Byte comparisons cover the full5120-byte macro observation, not just an invented device name or canned interface response.

Five Reader unit tests plus one output-subrange test pass. All130 SteelSeries driver tests and20 main Prime tests pass;6 artifact tests remain ignored. Source verification covers the unnumbered report lengths, echo scratch address and known transmit call targets. Report: development/2026-10-30-prime-output-read-emulation.json.

```text
cargo build --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries --example prime_output_read_exchange
cargo run --quiet --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries --example prime_feature_read_fixture > <fixture.json>
python scripts/test-prime-output-read-emulation.py <firmware.bin> --reader <built-prime_output_read_exchange.exe> --fixture <fixture.json> --report <report.json>
```

Development-only Unicorn2.1.4 remains separately installed. No emulator/vendor firmware/test data was bundled, no runtime dependency was added, and no actual HID handle, mouse, OS input, desktop UI, installer or release was touched. Counts remain44 partial/0 complete.

Remaining: real Windows output/interrupt mapping and complete runtime/notification validation, caller entropy and connection identity, exclusive/cancellable per-device integration, stable whole-image and idle-state checks, persistent journals/transport recovery, complete allocation/copy-on-write/concurrency and macro modes, guarded storage/UI integration, flash readback and actual hardware verification. These candidates must not bypass those requirements.

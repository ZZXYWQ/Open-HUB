# Prime macro image protection and conditional recovery

2026-10-30. Development-only offline planning. Native macros remain disabled in the application; this is not complete OEM compatibility or verified live recovery.

## Implemented

`prime_macro_storage::Image` requires the full observed5120-byte firmware macro range and both complete mapping tables:30 physical bytes and10 wheel bytes. The firmware's physical/wheel setters establish RAM bases20001cb6/20001cd4 and lengths30/10; their instruction digests and literal pointers are now pinned in gg122-prime-firmware-macro-source.json. The static verifier checks ten base regions plus ten playback regions. This does not implement USB reads of those tables or enlarge the host allocation budget beyond the reviewed1024-byte transferred prefix.

The image census examines all eight five-byte records, including the fixed bottom CPI record. Function0x71 references have little-endian word offsets at+3/+4. Marker0x77 and the raw event count establish protected extents; unknown playback modes, directions and event records are preserved without becoming executable application capabilities. Foreign noncanonical binding reserved bytes are retained and their extents protected, but such a target cannot be edited. Distinct overlapping extents, invalid references, incomplete data and a changed fixed record refuse admission. Exact aliases can be inventoried, but a shared target cannot be individually replaced without future copy-on-write allocation.

`replace_once_in_place` explicitly replaces an existing exclusive target with the admitted one-shot program. It protects primary left and fixed bottom CPI, covers independent physical/wheel roots, requires the replacement to fit both its original extent and the current transferred prefix, and changes no mapping bytes. Every byte outside the replacement remains exact, including shortened trailing content, opaque unreferenced data, foreign macros in the host tail and beyond1280 bytes, and all unknown records. Zero-filled memory alone is not treated as free space. Identical edits produce no packets, because even a zero-byte native macro write would reset running contexts.

This API is not a complete allocator: first-time allocation, growing beyond the current owned extent, alias copy-on-write, recycling and persistent reservation history remain to implement. After shrinking, this helper does not infer ownership of the leftover bytes for a subsequent edit. Selecting this API intentionally requests native one-shot mode; it must not silently convert a user's other selected playback mode in a future UI.

## Conditional restoration

Each edit retains exact before/after images, replacement/original extents and the two native Feature frames. `restore` recognizes the original state or a whole-command prefix of the attempted forward writes. `restore_with_attempts` additionally recognizes bounded prefixes of a previously attempted restoration. This covers an interrupted restore that leaves the old header with a new event body across the512-byte boundary. Without that restoration attempt history, the mixed state is refused.

Recognition compares the ENTIRE supplied5120-byte image, both complete tables and the supplied device/connection stamp. Any unrelated change, third-party target value, changed mapping, different stamp, impossible attempt count or incomplete image refuses restoration. Already-original data produces no commands. Packet plans never imply that a restore has been executed or verified. Unknown torn states are not guessed. Matching a stamp or matching expected bytes is not proof that a real device is still connected or that another writer is excluded.

No persistent journal or real transport runner has been added. A future caller must obtain fresh stable reads, prove actual current identity and idle inputs/contexts, coordinate exclusive transport access, persist the exact plan and attempt counters BEFORE sending, verify every resulting readback, and report restoration failure explicitly. The pinned firmware resets all macro contexts before validating writes. These preconditions cannot be replaced by synthetic images or the constructor's scope string. Repeated writes/retries are not automatically authorized by this offline decision API. Flash persistence, real USB input release and all macro playback modes remain unverified.

## Verification performed

Eight new Rust tests cover full reference census, raw foreign mode6/unknown events, noncanonical reserved-byte preservation, shorter replacements, primary/fixed protection, independent wheel roots, shared aliases, overlap and malformed input, growth/tail refusal, no-op suppression, known forward/restore-prefix histories, connection/table changes and unknown target/foreign memory changes. The unchanged low-level coverage planner now documents that its offset list covers changed/new programs while the caller must separately prove byte preservation of every unchanged foreign reference; it does not supply this census itself.

The Rust `prime_macro_storage_fixture` example emits the production edit and restore frames for three scenarios: physical target at64, a physical macro crossing512 at504, and an independent wheel target. Existing opaque foreign macros at600,1200 and2048 remain exact. The Python harness seeds isolated RAM using the pinned binding-table layout and executes the original write handler, binding wrappers and scheduler. It does not boot the firmware or emulate USB/peripherals. Unchanged command lists keep the original running macro active in all three scenarios.

Thirty actual emulated memory/table observations are fed BACK through the Rust production image/recovery code:15 recognized original/forward/restore-prefix states and15 rejected foreign-memory, foreign-table, changed-connection-stamp, truncated-image or third-party-target states. Seven restoration sequences complete back to the exact original full image, followed by original macro playback. Eighteen original or edited program replays pass. No-op preservation passes three times. The short-read cases truncate captured RAM data deliberately; they do not demonstrate actual USB short-transfer behavior. Mock stamp differences do not establish physical device identity.

All121 SteelSeries driver tests and20 main Prime tests pass;6 artifact tests remain ignored. The firmware source verifier passes all20 pinned regions and mapping pointers. Report: development/2026-10-30-prime-firmware-storage-emulation.json. No firmware/emulator package/test fixtures or new runtime dependency is bundled. No mouse, OS input, desktop UI, installer or release was touched. Counts remain44 partial/0 complete.

Reproduce with the separately installed development-only unicorn2.1.4:

```text
cargo run --quiet --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries --example prime_macro_storage_fixture > <fixture.json>
python scripts/test-prime-firmware-storage-emulation.py <firmware.bin> --fixture <fixture.json> --observations <observations.json> --report <report.json>
cargo run --quiet --manifest-path src-tauri/vendor/gearwise/Cargo.toml -p gearwise-steelseries --example prime_macro_storage_fixture -- --check-observations <observations.json>
```

Remaining work: verified real readback and current hardware identity/idle state, exclusive I/O and persistent journals, full allocation/copy-on-write, whole-plan concurrency limits, all macro modes and real timing, failure-safe application/store/UI integration, flash verification and hardware validation. These gaps keep this model out of complete acceptance.

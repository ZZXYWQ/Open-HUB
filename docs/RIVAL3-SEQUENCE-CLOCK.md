# Rival3: reviewed sequencer transitions and bounded playback state core

Development increment, 2026-10-29. This initial core increment did not enable dynamic playback. Follow-ups connect the player and reviewed GG loops; no OEM runtime parity, fully compatible model or installer is claimed.

## Source evidence

Official GG122 SSEdevice.dll SHA25650284715f971c6daea99bbe714caad9de329a681eb8f54915081777c4ce1264a contains exported Sequencer_NewSequence, StartSequence, ResumeSequence and StopSequence functions. Go SteelSeriesEngine.exe SHA256c1b8e182e62c8bb6667a3c678394c4d6363a532409f3de5a8fd7a31fdf0c0ec1 calls this native sequencer. Local Ghidra12.1.4 statically analyzed both files; no vendor binary or DLL was loaded as executable code. REA's native backend is not configured, so its provider was not used for decompilation.

Native constructor0x180006b80 stores a byte repeat value, original/template packet, rewriting instructions, byte-frame data, frame count and step duration. Scheduler0x18003d420 computes shared elapsed milliseconds; writer0x18003d100 constructs the current frame, advances state and checks running before SendCommand. Ten machine-instruction anchors, addresses, bytes and their meanings are retained in development/gg122-native-sequencer-facts.json. scripts/verify-gg-native-sequencer.py independently checks exact DLL SHA256 and PE-mapped source bytes without executing the vendor artifact.

Observed native rules:

- Raw native repeat255 uses shared phase floor(elapsed_ms/step_ms) modulo frame count. The writer builds the previous indexed frame before installing the published phase. Wrap notification compares new phase with old index.
- Other native repeat bytes advance by accumulated pending ticks, not an unbounded burst of output packets. At a frame boundary the completed-cycle byte increments once and either resets the index or stops. Zero and one both stop at the first boundary.
- The final finite-cycle running flag is cleared before the send gate. The prepared final packet is therefore suppressed in this reviewed build. This is an observed compatibility behavior, not a required policy for newly authored Open HUB animations.
- Start resets index and cycle counter. Resume only reenables running; stop retains the position/counter. The kernel deliberately distinguishes these operations.

## Implemented

New native Rust module rival3_sequence provides strict, bounded color sequence parsing and a deterministic Cursor for those reviewed state transitions. It owns no timer, thread, device or HID transport. RGB records must be exactly three bytes; repeat must be a byte; unknown fields and more than4096 frames are rejected. Empty sequences remain valid file records for static color but cannot start a cursor. Cursor step durations have an explicit software bound1–60000ms rather than allowing division by zero.

The production GG Rival3 converter now uses the same typed Sequence decoder. Constant/empty sequences continue through the previously accepted steady-color path. The initial converter rejected dynamic/finite sequences. The [later GG loop integration](GG-RIVAL3-DYNAMIC-IMPORT.md) admits reviewed nonempty repeat255 loops on1824; unsupported finite sequences still reject the entire candidate.

Four test groups use reviewed finite/infinite transition examples, old-frame/shared-phase ordering, dropped pending ticks, finite last-packet suppression, paused/resumed/restarted state and malformed RGB/clock/size inputs. Full native and main suites, original default-source rejection and saved browser profile replay are also checked.

## Remaining obligations

The follow-up traces the source repeat property through both Go wrappers and the native DLL constructor, and the33ms declaration through the original inherited descriptor chain and final native step field. These loop-specific parameters are now verified. Renderer sampling alone was not used to establish the native step. Finite UI/policy remains incomplete.

The native scheduler notifies the writer before publishing the phase field. The Rust cursor models an observed, published scheduler snapshot; it does not reproduce that native notification/publication race or Windows scheduling jitter. Physical transfer latency and sequence timing have not been measured.

The connection-owned player now has per-frame connection/revision checks, stop/join, initialization, fault cancellation and explicit app shutdown. Full simultaneous PC/dock controller policy still needs application validation. The OEM shutdown path writes a flash fallback; that must not be silently copied into ordinary app exit. Loop sequence UI and reviewed GG loop import/application are connected. Finite UI and physical tests remain unfinished. Counts stay43 partial /0 complete.

Temporary Ghidra projects containing vendor artifact copies and the one-off analysis launchers were removed after evidence extraction. Existing source research files owned by earlier work were not modified. Vendor pseudocode, binary bodies, decrypt keys and a decompiler runtime are not distributed with this change.

Follow-up: [connection-owned device playback layer](RIVAL3-CONNECTION-PLAYER.md) now exercises the actual driver through memory transports, with per-frame connection/revision checks. The [loop profile integration](RIVAL3-LOOP-PROFILES.md) adds UI, profile activation and managed shutdown; the [GG loop follow-up](GG-RIVAL3-DYNAMIC-IMPORT.md) adds source migration.

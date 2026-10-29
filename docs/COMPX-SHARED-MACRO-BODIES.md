# COMPX identical macro body sharing

Development change, 2026-10-29. Applies only to the existing native COMPX catalog (nine distinct mice, ten connection entries); no new model or complete-compatibility claim.

A native macro's program bytes and its binding playback byte are separate. The allocator now first looks for the exact encoded program in the target's current slot, then in deterministically ordered slots referenced by other current-bank buttons. A matching program can be shared with independent Once, Repeat, Toggle or WhileHeld binding modes. An identical body is never rewritten; a binding-only update sends one SET, and an unchanged binding sends none. Host names stay in the profile.

The commit captures all 384 slot bytes and current-bank bindings, validates checksums and identity, then rechecks the program and references. A nonempty slot cannot be newly adopted merely because its body matches: it must still have a live reference. Unbound original macros remain untouched. If an edited program differs from a shared body, a separate free slot is required; exhausted slots reject without changing existing bindings or programs.

The existing conditional transaction verifies before and after writes. A failed binding-only write restores the original binding only when all slot bytes, other bindings and target state still match permitted pre/post states. Foreign changes to program bytes, opaque tail or another binding stop recovery without overwriting that change. This does not prove firmware playback, atomicity, cross-bank ownership or in-flight macro quiescence.

Validation uses production allocation/encoding/transactions over in-memory COMPX reports, without physical HID or OS input. Added cases cover full-slot reuse, independent modes and readback, no-op changes, copy-on-edit with and without free space, stale body/reference refusal, orphan retention, before/after-effect binding faults and foreign program/tail/binding mutations across all ten existing connection entries. OEM file import and real browser-saved profile replay assert that the two reviewed bindings share slot zero and retain their distinct confirmed modes and every event/delay. UI IPC and device inventory remain mocked.

Default tests: 1052 passed, zero failed, 92 ignored for separate artifacts. Explicit browser-save replay: one passed. Native development build passed. No dependency, frontend asset, version, installer or release change. Full acceptance remains 0/100; see [model count](MODEL-COUNT.md).

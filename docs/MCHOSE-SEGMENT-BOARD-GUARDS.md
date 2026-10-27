# MCHOSE standalone multi-report board checks

2026-10-27; development only. This closes an application-path gap found while following up the A7 physical DPI experiment. It does not add an accepted mouse model.

## Behavior

`apply_hardware_for` already pins an action to its original board and persists a full backup. Previously its standalone DPI helper passed an empty callback between X/Y SET reports; standalone name and macro upload paths likewise did not repeat the board check between chunks. A physical or OEM board change during an upload could therefore leave the remaining reports running even though the draft's selected context had changed.

The standalone DPI writer now calls `require_board(original)` before each axis SET, including after the firmware queries that select the linked-axis behavior. The standalone name writer checks before each 20-byte chunk. Macro save and delete actions use the existing checked macro writer with the same check before each 19-byte chunk. They stop on a detected change and never adopt the newly selected board. Full-plan transaction callbacks remain intact.

Action execution errors after the backup now include its path and explicitly state that partial writes may have occurred and no automatic retry was made. Stopping an upload does not undo its first report or claim that the firmware discarded staged bytes. No forced board switch or blind restoration is added.

## Evidence

Production packet-codec tests inject board changes after the first X SET, after the writer's receiver-firmware GET, after the first name chunk, and after the first macro chunk (both save and delete). They assert one X write with untouched original Y, zero DPI writes after the firmware-query switch, one name/macro chunk with no continuation, retained backups, and no writes to the newly selected board. Existing normal name/macro/DPI roundtrips and full-plan behavior remain in the default suite.

- Native default suite: **799 passed, 0 failed, 62 ignored** (three new tests and one strengthened existing test).
- Non-test native development build: passed.
- Built headless UI draft regression: **11 groups passed**, mocked IPC, no physical I/O. This checks existing draft retention behavior; it does not itself prove native device routing.
- No UI production assets, runtime dependencies, installer or release changed. No physical SET or host input was performed in this increment.

## Limits

These checks detect an observed board change; a GET followed by SET is not an atomic firmware transaction. Switching away and back between checks, simultaneous external writers, interrupted firmware staging, macro playback and power-cycle retention remain unproved. Global macro slots still need separate shared-reference protection; the board check is not a claim that macro storage is board-local. The previous real 1600→1650→1600 test remains scoped to mouse firmware 0x17 / receiver 0x06 and does not physically validate this new interruption case. Complete acceptance remains **0/100**.

Machine-readable evidence: [validation](development/2026-10-27-mchose-segment-guards-validation.json).

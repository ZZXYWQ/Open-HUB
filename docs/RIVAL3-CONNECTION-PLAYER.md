# Original Rival3: connection-owned device playback layer

Development increment, 2026-10-29. This layer is implemented and exercised with the production native driver on memory transports. It is not registered as a Tauri command or enabled in the UI yet. Dynamic GG import remains blocked; no fully compatible model, installer or release is added.

## Implemented

rival3_player::Player starts an explicitly authored Open HUB looping RGB timeline on original wired1038:1824. Target includes the exact device ID and ephemeral open-connection key. Config contains four initial RGB values, optional independent per-zone loop sequences, and a nominal frame interval33–1000ms. At least one nonempty sequence is required; each sequence is1–4096 RGB frames with explicit native loop byte255. This does not infer Go/GG finite-repeat options or arbitrary OEM playback equivalence.

Before spawning a worker, the layer validates config and target, stops/joins an earlier worker for that target, rechecks the connection, and explicitly applies a complete full-brightness steady palette. Only a successful initialization starts playback. It withdraws the target's earlier hardware-save session so a static session cannot authorize flashing a moving frame. Saved profile files and other devices are not changed.

The worker uses the reviewed bounded Cursor for each animated zone. A per-connection monotonic clock is shared by that connection's four zones; separately opened devices have independent clocks. Late slots update phase rather than generating an unbounded catch-up burst. Stationary zones retain their explicit initial colors. Repeated identical palettes need no extra device command. New palettes go through the real SteelSeries driver, including its conditional batch path and failure handling; no raw unreviewed commands are emitted by the player.

Every frame locks the manager and rechecks exact product, connection key and process-local lighting revision before writing. Drivers opt in to a new Device::lighting_revision method; only reviewed Rival3 drivers implement it here, and other drivers retain None. Rival3 increments this revision for each valid lighting write attempt, including identical writes and attempts that fail after partial transmission. This is local coordination, never hardware readback or persistent mouse identity. A manually rewritten identical palette therefore revokes the old worker; comparing colors alone would miss that handoff.

Pause, resume and restart use acknowledged messages. Pause confirmation occurs after an in-flight operation completes; no later frame is written until resumed. Restart resets cursor state while resume retains it. Stop removes the owned worker, requests cancellation and joins it. stop_all cancels all workers before joining; Drop also joins owned workers. Stopping leaves the last successful color rather than making an unverified restoration or flash write. Blocking vendor transport calls still depend on that transport returning; there is no claim of forced USB cancellation or measured stop latency.

A stale control target cannot stop a newer connection. Reopening the same device ID produces a different connection key, so the old worker refuses all frames to the replacement. Write failure ends playback once, retains an error and uses the driver's uncertain-cache handling without retries. Status colors describe the last successful desired output, not observed hardware state.

## Verification

Seven application test groups cover real native output bytes/padding and no flash, joined stop without subsequent writes, identical manual-lighting handoff, same-ID reconnect with no replacement writes, two identical devices paused/restarted/stopped independently, first-frame short write without retry/cache success, invalid firmware/target/config rejection, earlier flash-session withdrawal, and Drop joining its worker. A separate shared-driver test checks revision opt-in, new-connection reset, valid-lighting attempts and exclusion of report-rate/invalid-profile actions. Hardware, OS input and the installed app are not accessed.

Full main suite:1090 passed,0 failed,97 ignored. Shared workspace:398 unit plus1 doc test passed. No frontend code changed in this increment. Evidence is in development/2026-10-29-rival3-player-validation.json.

## Still required

Integrate this engine into managed app lifetime and explicit shutdown, exact connection-aware RPC and the lighting/profile editor. Define the persisted authored timeline schema, source-aware desired session state, and coordination with the existing PC/dock effect controls before exposing an application entry point. Connect saved timelines to profile activation and verify actual UI requests against the worker. The current generic per-device host clock is an explicit Open HUB policy, not proof of exact native global-thread scheduling.

Go argument mapping, original GG dynamic migration, finite playback UI/policy, hardware readback, physical timing and actual Rival3 hardware tests remain incomplete. Firmware184C is not enabled for this new player, and other brands retain their own drivers/UI. Counts stay43 partial /0 complete.

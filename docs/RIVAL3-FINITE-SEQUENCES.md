# Original Rival3 finite sequence integration

Development increment, 2026-10-29. Original wired1038:1824 only. This connects the reviewed native finite-repeat rules to import, persisted configuration, UI and connection-owned playback. Counts remain43 partially integrated models /0 fully accepted. No installer, release, installed-app change, visible desktop, physical HID or OS input is involved.

## Native behavior and explicit format

The existing pinned native constructor/worker facts and verified Go ABI establish direct source-byte mapping. Repeat255 is infinite;0–254 are finite, with0 and1 both ending at the first cycle boundary. The reviewed native worker constructs the old frame, updates the cycle/index and checks running before sending. Therefore the final frame of the final cycle is suppressed. A single-frame single-cycle sequence sends no sequence color and leaves the explicit initial palette. These source rules are visible in the UI rather than silently replaced with an invented final-color policy.

A new VendorProfile `openhub-rival3-sequence/1` admits explicit finite byte values as well as infinite255. Four initial colors, four optional independently sized sequences,1–4096 frames per nonempty zone and33–1000ms nominal step remain bounded. The existing `openhub-rival3-loop/1` contract continues to reject finite values, preserving prior validation behavior. Both schemas lower to the same reviewed native steady palette for model/capability validation and initial write; mixed or duplicate lighting schemas still reject.

GG nonempty finite sequences now keep every source frame and raw count, including0, with descriptor-backed33ms step. Empty records remain stationary. Fully static matching infinite palettes retain their previous native steady path. Unknown fields, malformed/out-of-byte counts and unreviewed184C dynamic transport still reject the complete candidate. The new finite-schema fixture uses authored original GG layout, not an observed finite user database. The unmodified official303-frame infinite default remains independently tested.

## Playback lifecycle

The worker gates ticks when paused without changing each cursor's finished state. A completed finite zone stays finished when a peer zone continues looping, pauses or resumes. Restart explicitly resets all configured cursors. If every animated zone finishes, overall phase becomes completed and the worker waits on its control channel; no more frame writes are issued. Restart and stop remain available with the same connection/playback guards. Resume/pause of an already completed whole sequence is rejected with a request to restart. Shutdown/stop still cancel and join the idle worker. Completion retains the last successful color and does not flash.

Status now includes four zone phases (static/running/paused/completed/stopped/failed). Desired session state distinguishes completed from stopped; an optional selected input without a software macro does not hide completion. An armed macro can remain active when lighting has ended. Dynamic plans cannot become hardware-save authorization merely because the timeline completed, paused or stopped.

The sequence editor exposes sustained loop or original finite mode for each zone, authored counts1–254, initial colors and complete frame controls. A source count0 is displayed as once with an explicit legacy-value note and is preserved unless its playback setting changes. Editing colors, adding/removing frames and presets retain that zone's count. Choosing finite in a legacy loop profile explicitly upgrades the profile schema. Playback UI shows completion and per-zone state, offers restart after completion, and retains existing current-connection/playback request scope. Session summary is labeled as state at read time, separate from live lighting status.

## Verification

Main suite1100 passed,0 failed,100 ignored. Shared workspace399 unit plus1 documentation test passed. The production frontend build passes; no runtime dependency or image asset is added.

Four added default application groups cover: native one-frame0/1 end without a terminal write, no later writes after completion, restart and flash rejection; a finished zone surviving pause/resume of an infinite peer; exact nominal two-cycle terminal suppression and legacy schema refusal; all256 source count bytes preserving original frames/plan scope plus mixed finite/static native output. The added shared Cursor test covers every finite byte0–254 with lengths1/2/3/17 through the exact completion boundary and terminal suppression. These are native-state/codec/application memory-transport checks, not physical timing evidence.

Headless Edge uses the built production UI and converter-produced fixture DTOs. It exercises per-zone count editing, preservation across color/add/delete, finite/loop switching, raw0 preservation, quick-save equality, legacy-loop-to-sequence upgrade for a separately authored profile, completed/restart/pause/resume/stop controls, per-zone finished states and disabled flash. Browser IPC replies are mocked.

A separately selected Rust replay loads both actual saved browser profiles through production ProfileStore and configuration pack transfer, compares the complete imported plan to the source fixture with only intended edits, runs native nominal simulation, then applies to the connection-owned real worker on memory HID. Both plans complete and remain free of later writes, preserve their desired plan, reject flash, restart and complete again, and join on shutdown. The four existing explicit saved-profile replay groups (native, static GG, authored loop, official dynamic loop) pass too. Five UI scenarios, including the new finite UI, pass.

Evidence: `development/2026-10-29-rival3-finite-validation.json`, `scripts/test-rival3-finite-ui.cjs`, player/converter/shared tests and retained private logs/screenshot/saved profiles. The existing exact-source anchor verifier is also re-run; no new vendor artifact or decoded source copy is introduced.

Remaining: exact OEM global phase/publication race and physical transfer timing, complete unsupported GG meta/performance/settings options, full PC/dock simultaneous-controller policy, hardware readback and actual1824/184C hardware validation. The native finite policy can skip frames/cycle progress after delayed ticks; nominal durations are not measured device timing. This does not make Rival3 or another model fully accepted.

# Original Rival3: native batch palette and host-sequence research

Development increment, 2026-10-29. No new complete mouse count, release, installer or physical I/O.

## Source and boundaries

Official GG122 original rival-3.edevice was read statically using the existing local OpenPGP decoder with MDC integrity validation. Encrypted SHA2567dc4cbd8bfd778cf374f4c935abe9dccf8607026a4850e330ddd07e12c6cedc6; decoded SHA2561057127f3eeeb30e2215e11e45e346bae2e68103b0307df9422af8637b9c2ab4. It names product1038:1824 and firmware0.81.x.x. No Lisp, vendor binary, SQL, downloaded script or device command was executed. The decoder key and vendor source body are not distributed. Only protocol facts are recorded in development/gg122-rival3-native-lighting-facts.json. Newly generated decoded vendor text and research helper are removed after extracting the facts.

Native set_all_colors is a33-byte report: report ID0, command0A, selector0, zone mask0F, fourRGB triples in z1/z2/z3/logo order,17 reserved zero bytes. It is cache-exempt and has no brightness parameter. Original all_illumination uses this command for color-bearing modes, followed by a mode command. Native mode command06 has an optional speed byte; selector7 is a speed operation in this description. Those speed/query behaviors have not been enabled here.

## Implemented

The production SteelSeries driver now uses this exact batch command for subsequent full-brightness steady palettes on1824, followed by an explicit padded steady mode command. This reduces five command writes to two; it does not promise measured latency or physical refresh rate.

A new connection starts unprimed even though template values are populated. It must first successfully write all four RGB/brightness values and the mode through the existing complete path. Both previous successfully written palette and requested palette must be enabled, steady and100% brightness on all four zones. The new batch command cannot encode brightness, so dimmed/disabled/other-pattern cases retain the complete five-command path. Firmware identity184C and all other models retain their prior path; the original descriptor is not evidence for those new commands.

A short/error write at either batch command invalidates the full lighting cache and priming state. No automatic retry or unverified restoration occurs. A subsequent explicit complete application starts with the five commands again. Priming is held in each open device object, never shared by brand, model name, serial or profile. Generic static-zone edits use the same planning path while retaining the other three desired colors.

## Host animation remains unfinished

The original description defines four independent sequence templates that send command05 zone/RGB/brightness100. Initialization sends mode7 speed0. Shutdown stops all four sequences; if any saved sequence has data, it switches to rainbow and saves Flash. This behavior is recorded rather than automatically imitated; application exit must not silently change persistent device settings.

Renderer factory95552 explicitly generates software gradients at33ms sampling steps. This proves the frontend sampling convention, not the Go backend scheduling, finite-repeat interpretation, pause/resume handling or physical USB timing. The earlier fact field was renamed from host_frame_interval_ms to renderer_simulation_step_ms to keep this distinction explicit. Those backend semantics and connection ownership still need verification and integration before arbitrary GG dynamic profiles can be accepted.

The descriptor also serializes15 button slots while the actual frontend exposes eight inputs; undocumented slots are not counted as extra physical buttons. Debounce make/break and individual effect query commands, the speed operation, and differences from generic selector meta53 remain recorded gaps. No unreviewed command or extra input is opened from those findings.

## Verification

Production memory transports verify independent full batch bytes and padding, ordinary path for184C, first-apply template exclusion, previous/next dimmed/off/dynamic exclusions, both partial batch failures with no retries and complete recovery, separate identical device sessions, reconnect priming reset, and generic static-zone color preservation. The existing shared Rival3 command test is updated to verify the complete initial writes followed by the source-backed batch packet. Full test results are recorded in development/2026-10-29-rival3-batch-validation.json.

Follow-up: [native sequencer rules and bounded state core](RIVAL3-SEQUENCE-CLOCK.md) now have instruction-level evidence. The playback worker and complete GG dynamic application remain unfinished.

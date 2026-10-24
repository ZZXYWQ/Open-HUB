# Open HUB protocol work — verified specific-model integration

## Current checkpoint (2026-10-24)

The executable import/save/DPI/button/host-macro paths for Naga V2 HyperSpeed
(1532:00B4), M65 PRO RGB (1B1C:1B2E) and Rival 310 (1038:1720) are connected.
The actual model-scoped production drivers run over memory HID in tests. The
three new cross-layer tests read source-format files, save/reload the on-disk
profile store, apply through the production Service, dispatch a physical-source
trigger and assert complete keyboard/mouse macro output. Synthetic fixtures are
labeled; their existence does not establish hardware compatibility.

Final native regression: 217 passed, none failed or ignored. Driver workspace:
289 unit tests + 1 doctest. Synapse parser: 148 unit + 3 integration tests, including
an explicitly supplied public-export smoke sample. All 13 rendered UI suites
pass; 20 brand-import checks cover the single toolbar chooser and 13 individually
listed brands. UI uses mocked Tauri and native DTO fixtures, separately from the
production-driver integration tests. Supported source formats and remaining
model restrictions are recorded in MOUSE-BRAND-SUPPORT.md.

Logitech/MCHOSE are not handled by the three-brand backend. Exact-model validation
precedes input-route changes or device restoration. Full desired session plans
survive refresh/edit/save; scoped stop preserves other devices. Write-ahead
recovery, release waits, foreign-device guards and reviewed M65 reconnect/retry
have fault tests. Viper partial features remain bounded and do not imply complete
Synapse conversion or macros for those models.

The compact installer has been built, extracted and byte-verified, upgraded from
0.2.3, overwritten with the same version, uninstalled with data retained, and
reinstalled after its program directory and uninstall registration were absent.
User configuration/backups stayed byte-identical and the installed program
restarted. This is the existing Windows/user/WebView2 environment; a clean OS,
new-user first run and missing-WebView2 network bootstrap are not tested.
The installed application is now 0.3.0. See RELEASE-VALIDATION.md and PACKAGING.md.

No official vendor program is bundled. RGB starts on demand. OpenRGB and its
corresponding source/licenses are retained. The normal installer uses system
WebView2/downloadBootstrapper and LZMA; offline WebView2 is an opt-in build.

This completes the specified three-model integration/installer scope. Wider
whole-brand coverage is not established. AULA, ROG, HyperX, VGN/ATK and other
unimplemented adapters remain research or future work; no generic card claims
otherwise. Only the existing MCHOSE A7 V3 Ultra+/MagDock hardware is available.

Historical checkpoint follows:


## Current checkpoint (2026-10-23; supersedes draft notes below)

Latest: M65 poll-rate USB re-enumeration now uses a durable reviewed continuation.
It saves the full plan before the rate write, stops using the old handle, reopens
the explicitly selected target, verifies fresh IDENT polling, and applies the
remaining DPI/buttons/macros without resending polling. Unknown send results are
retained and never retried automatically. Explicit retries have monotonic attempt
versions, so identical errors cannot reuse stale confirmation. Full plans survive
restart and require fresh input-source binding. The actual device and profile
pages can review, save, continue, explicitly retry or discard pending plans.

Current validation: **202 root tests**, **287 vendor unit tests +1doctest**, fresh
frontend build, **ten rendered UI suites** and final production release check all
pass (`../rate-reconnect-final-tests.log`, `../rate-reconnect-vendor-tests.log`,
`../browser-ui-regression-report.json`,
`../rate-reconnect-final-production-check.log`). Eleven new native fault tests
use actual NXP driver code with scripted enumeration/memory HID; browser mocks
Tauri. New models remain physically untested. This supersedes older polling and
test-count statements below; installed0.2.3 is untouched and no final installer
has been produced.

Remaining audit gate: device-page `fromMulti` still reconstructs only DPI/rate,
so native read/fresh page mount does not round-trip full desired macro definitions
and button/source plans. Add real regression coverage and fix that state flow
before packaging. M65 cached state must also not be described as proof of another
controller's current settings. Final installer/install/upgrade validation remains.

Latest: reviewed restart recovery is connected to the actual device UI and native
commands. It displays original actions/DPI and exact current target identity,
requires explicit confirmation and a matching physical input source, rejects
changed records/target identities and active routes, and preserves failed journals
for retry. M65 waits for observed released vendor buttons, then sends a dedicated
reviewed hardware reset even when a fresh driver has no cached routing ownership.
The ordinary automatic stop path retains its ownership guard. Other models and
brands do not inherit the M65 command.

Current validation: **191 root tests**, **287 vendor unit tests + 1 doctest**, a
production release check, fresh frontend build and **nine rendered UI suites** all
pass (`../manual-recovery-final-tests.log`,
`../manual-recovery-vendor-tests.log`,
`../manual-recovery-production-check.log`,
`../browser-ui-regression-report.json`). Seven root tests and thirteen browser
checks cover the new recovery flow. Browser uses mocked Tauri and all new mouse
models remain hardware untested. This supersedes lower counts/no-manual-UI notes.
Changed-polling reconnect, M65 live/cache ownership handling and final installer
verification remain open; installed 0.2.3 is untouched.

Newest native checkpoint: **184 passed, 0 failed, 0 ignored** in
`../native-release-complete-tests.log`; production release check also passes in
`../native-release-production-check.log`. Physical trigger state is now tracked
per source: stopping cancels software output immediately and waits for release
before restoring native mappings. A background worker completes live recovery
without UI polling. Disconnect revokes live authority, and a reused path or
different serial cannot authorize restoration. Shutdown reentrant cancellation
and concurrently removed journal files have regression coverage. The Windows
key-state guard is compiled but hardware/secure-desktop behavior is unverified.
Manual restart recovery, M65 cached/live state and polling-reset reconnect remain
release gates. Installed 0.2.3 is untouched; there is no final installer.

M65's device panel now explicitly describes session-cached live settings and labels
its refresh action accordingly. This corrects the UI, not the missing live GET
protocol. A fresh frontend build and all eight rendered browser suites passed
after this change (`../browser-ui-regression-report.json`, mocked Tauri backend).

Latest update: exact Rival310 GG SQLite import is connected, editable, simulatable
and saveable. It preserves the supported source CPI/rate/buttons and original
mixed keyboard/mouse macro events, repeat modes/gaps and one application path.
Unsupported layouts/actions/multiple-app conditions block conversion. Read-only
source DB behavior and the official GG122 default JSON seed were verified. iCUE
and GG now share a reusable import component and the same six-source menu.

Latest root run171 passed,0 failed,0 ignored (`../gg-complete-native-tests.log`),
including explicit external source probes. All eight rendered UI suites passed
with a mocked Tauri backend. These supersede lower test counts below. Extra GG
source variants and the native recovery/reconnect/polling-reset gates remain open;
no final installer has been produced.

The real Gearwise driver bridge, exact-model discovery, profile saving and UI are
now connected. Supported simulated targets include Naga V2 HyperSpeed,
M65 PRO RGB and Rival310; DeathAdder V2 has partial capabilities. Software macros
use per-device Raw Input or Corsair vendor reports and a cancellable host runtime.
The Logitech and MCHOSE backends remain isolated. These three new brands have no
available physical test mouse; simulated results do not establish hardware support.

Source imports now include convertible Naga Synapse exports and exact M65 iCUE
profiles: XY DPI, button mappings and supported original macro events can be
reviewed, edited, simulated and saved. iCUE directional DPI actions are supported
only when their flags have known matching semantics. Unsupported actions and model
layouts block conversion; lights, extra playback variants and GG import remain
incomplete. GG122 official installer SQL and inert JavaScript are research sources
in the parent workspace; none of its proprietary program assets ship in Open HUB.

Latest root suite:164 passed,0 failed, including the explicit public iCUE export
probe (`../rea-checkpoint-native-tests.log`). All seven rendered React UI suites
pass with a mocked Tauri backend (`../browser-ui-regression-report.json`). Vendor
and parser suites also passed at their previous checkpoints. These are separate
layers of validation, not a claim of complete brand compatibility.

Release gates still include GG/source variants, reliable Corsair live-state and
USB reset handling, recovery/reconnect fault coverage, then installer validation.
Installed0.2.3 remains untouched; no final0.3.0 installer has been produced.

## Historical draft below

This work is not connected to native discovery, profile activation, auto-switch,
or the UI. The installed application and version remain unchanged. No installer
is produced from this work. Passing these tests does not mean brand support is
complete, and does not constitute hardware verification.

## Implemented in `src-tauri/src/multibrand_protocol.rs`

- Razer DeathAdder Elite: 90-byte XY DPI and polling-rate commands, transaction
  ID, CRC, response status/identity checks. Not a generic Razer adapter.
- Corsair M65 Pro NXP: volatile independent XY DPI-stage encoding. No start-up,
  HID interface selection or colour preservation transaction is implemented.
- SteelSeries Rival 310, USB 1038:1720, protocol v2: two DPI stages, polling
  rate, six mouse/keyboard/shortcut bindings; output plan compilation before
  transport, persistence last, short-write/disconnection rejection. Limited DPI
  settings decode. Timed macros are rejected, never reduced to one shortcut.
- Shared bounded, balanced keyboard macro validation and lossless serialization.

The module is compiled only under `cfg(test)` until its transport, identity,
profile persistence, imports and UI are ready. No HID API is used in these tests.

## Still required before the requested installer

1. Actual Windows HID discovery/transport and exact interface matching for each
   model. Collision handling with the original manufacturer's program.
2. Native get/set integration and clear write-only/partial-readback behavior.
3. Host input routing and a cancellable macro scheduler for devices without a
   documented onboard timed-macro protocol; release held keys on all exits.
4. Verified Synapse, iCUE and SteelSeries GG profile fixtures and parsers.
   ckb-next is an alternative application; its format is not iCUE's format.
5. Typed profile data, strict model scoping, UI, import preview, editing and
   end-to-end simulated scenarios including faults and existing-brand regression.
6. Build an installer only after the above is implemented and tested. Label all
   three new models as simulated, not hardware-tested.

## Primary protocol references

Read on 2026-10-22; local research snapshots are outside the shipping resources.

- https://github.com/openrazer/openrazer/blob/master/driver/razercommon.c
- https://github.com/openrazer/openrazer/blob/master/driver/razerchromacommon.c
- https://github.com/openrazer/openrazer/blob/master/driver/razermouse_driver.c
- https://github.com/ckb-next/ckb-next/blob/master/src/daemon/dpi.c
- https://github.com/ckb-next/ckb-next/blob/master/src/daemon/nxp_proto.h
- https://github.com/ckb-next/ckb-next/blob/master/src/daemon/usb.h
- https://github.com/libratbag/libratbag/blob/master/src/driver-steelseries.c
- https://github.com/libratbag/libratbag/blob/master/data/devices/steelseries-rival-310.device

The tests compare fixed wire bytes from the documented layouts, exercise bounds,
CRC/status/identity failure, macro truncation rejection, complete prevalidation,
short writes, and a disconnect at every transaction step. A mock acknowledges
transport bytes only; it cannot verify how hardware interprets them.

## Independent cross-check on 2026-10-22

Ran flozz/rivalcfg against its FakeDevice with the real HID module replaced by
an error-on-access stub. Generated 14 DPI/rate/button reference packets in
src-tauri/tests/fixtures/rival310-rivalcfg.json. No real device was opened.

Important unresolved disagreement: rivalcfg's Rival 310 DPI command places 0x42
one byte earlier than libratbag, and uses rate codes 4/3 for 125/250 Hz where
libratbag v2 uses 8/4. These are separate draft codecs with explicit comparison
tests. They are NOT interchangeable. The current no-hardware evidence is
insufficient to select a production dialect. Existing golden tests only proved
conformance to their respective source, not correctness on an unknown firmware.
References:
https://github.com/flozz/rivalcfg/blob/master/rivalcfg/devices/rival310.py
https://github.com/flozz/rivalcfg/blob/master/test/devices/old_specs/test_rival310.py

## Synapse parser breakthrough on 2026-10-22

A parser-only MIT adaptation of danscMax/Sidearm now lives in
src-tauri/vendor/sidearm-synapse-parser, with upstream license and provenance.
It is compiled into Open HUB and exposed through preview_synapse_import and
--synapse-import-probe. A read-only configuration-center preview shows mappings,
macros and all conversion warnings. It does not commit or activate a profile.
The real public export was parsed by the compiled binary: 1 profile, 39 bindings,
3 unmappable actions, 1 warning, 0 macros. Separate simulated tests cover macros.
DPI import, model-general mapping, device application and host macros remain
unfinished. See vendor README for exact limits. No new installer was produced.

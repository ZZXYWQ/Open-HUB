# Per-mouse profile ownership (unreleased development)

The old native Logitech/MCHOSE profile activation loop applied a profile to every connected matching device. The generic SinoWealth fallback also iterated over all peers. This contradicted the per-mouse configuration requirement even though direct controls and newer multi-brand plans had exact device IDs.

All profile activations now resolve one matching endpoint before writes or store changes. Two matching mice require an explicit target. The dispatcher calls only that endpoint's service. Native mouse profiles exclude keyboards and other non-mouse devices unless they expose an adjustable-DPI feature.

Imported profiles remain library templates. First use makes a private copy scoped to the selected mouse's vendor/model and local identity. A device has its own active profile, default profile, and template-copy references. Editing one copy or re-importing a source profile does not edit another mouse's copy. The old `active_id` remains only for compatibility/history. Old `profiles.json` files migrate without assigning a global Desktop profile to newly connected mice.

The device header, configuration center and tray all select an exact mouse. DPI/rate edits save into the open mouse's copy. Online multi-brand/SinoWealth complete-plan saves retain buttons, macros, lighting and DPI under that mouse, while saving a draft does not activate it. Selecting another mouse on a private profile exposes an explicit **复制并应用** action; the source copy remains intact. Exported profile packs contain profile values, not the local ownership/quarantine maps.

## Identity and automatic application

- Native Logitech uses feature `0x0003`'s nonblank/non-FF four-byte unit ID from a genuine observed long payload. It refreshes this identity while rescanning known receiver slots and checks it again before profile writes. Direct-USB model fallback is also included in profile matching; receiver PIDs are not paired-mouse identities.
- Model drivers and SinoWealth use a nonblank, non-placeholder model-scoped serial when available. An identity snapshot is checked again inside the device service's write lock, including before model-specific cleanup/restore operations.
- Duplicate unit IDs/serials are quarantined into separate connection identities. The quarantine is saved locally and seeded on restart; one duplicate disappearing does not re-enable automatic replay under the ambiguous old identity.
- Without a reliable unique identity, configuration remains connection-scoped and manual. USB path, receiver identity and display name are not durable physical-mouse identities. MCHOSE A7 V3 Ultra+ currently remains in this category; its receiver/model identity packet has not established a proven per-mouse unique ID. Reconnect/startup does not automatically write these profiles. The saved private copy can be explicitly copied/applied to the newly selected endpoint.
- Software input routes for Corsair, SteelSeries, Razer Naga, Basilisk and Pulsar require a matching readable serial and exactly one suitable input collection. Missing, conflicting or ambiguous identity rejects the software route before native writes. This may prevent software macros/remapping on hardware whose input interface exposes no usable serial; a verified container-based pairing fallback is not implemented yet. Native firmware actions do not require this software-input route.

Automatic foreground switching considers only profiles already assigned to the particular uniquely identified mouse. An unassigned device has no startup/reconnect write; an absent per-device default does not fall back to the shared Desktop template. Saved drafts do not enter automatic switching until explicitly applied.

## Verification and limits

Native tests cover two same-model Logitech mice, two MCHOSE endpoints, brand separation, private edits, template re-import, explicit cross-device copies, source selection, longest per-device application matching, old-store migration, changed receiver pairing, path changes, duplicate-ID quarantine across restart, inactive drafts and non-mouse exclusion. A real Corsair memory driver verifies stale identity rejection produces no native packets or backup directory.

`scripts/test-native-profile-routing-ui.cjs` consumes Rust-generated DTOs for two Logitech and two MCHOSE endpoints. It verifies device selection, private header choices, an exact-device DPI save, and explicit copying. The resulting IPC calls are replayed through the production Rust preflight and storage transitions by the ignored fixture test. This is a simulated dispatch/storage test, not physical Logitech/MCHOSE profile-write validation. The UI suites run in headless Edge and do not control the user's screen.

To generate the new fixture, set `OPEN_HUB_PROFILE_ROUTING_FIXTURE` to `../outputs/100-model-development/profile-routing-native-fixture.json` and run the `export_native_profile_routing_fixture` cargo test. Build the frontend, run the UI script, then set `OPEN_HUB_PROFILE_ROUTING_UI_CALLS` to the generated `profile-routing-ui-calls.json` and run `replay_profile_routing_ui_through_production_preflight_and_private_storage -- --ignored`.

This change does not add a fully accepted new mouse model or establish OEM feature/visual parity. Native Logitech profile values still cover the existing DPI/rate and onboard-reference functions; this ownership layer does not add new native macro allocation or lighting-profile persistence. Hardware serial quality and individual firmware behavior need model-specific validation. No installer, release tag, update feed or installed application is changed by this development checkpoint. The 100 complete distinct-model release gate remains unmet.

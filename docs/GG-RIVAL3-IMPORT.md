# Original wired Rival 3 GG import

Development increment, 2026-10-29. Still one partially integrated mouse model, not a fully accepted adaptation. No installer/release or physical HID test. Original Rival3 private layout is handled independently from Rival300/310 and Sensei Ten.

## Source facts

Locally retained official GG122 inert SQL, migration 3.17.0/20191223104517_squash.sql: device169, product 0x10381824, tag rival_3. Renderer factories17864 (configuration),91806 (TrueMove Core CPI table),86737 (binding selector),31021 (triggers); input event constants from factory55489. Data is parsed statically; vendor JS/SQL is never executed. Source hashes and all84 sensor code/value pairs are in development/gg122-rival3-facts.json. That file contains protocol facts, not copied executable vendor code.

The firmware PID184C uses the same admitted original Rival3 protocol layout, based on the pinned rivalcfg model; the GG adapter accepts that exact PID only with the same rival_3 name and complete validated layout. No original GG database row with184C has been physically observed here. Rival3 Wireless, Gen2 and rival_3_v2 are not accepted.

## Implemented path

- Read-only SQLite preview using the existing query-only transaction, input limits, macro references and program association checks.
- set_all_resolution has1–5 enabled consecutive slots and a separate selected1-based slot. Every source code must match the exact table. Original order and duplicate values are retained; there is no sorting or nearest-value guessing. Raw display CPI is only a checked hint, never the selected-slot authority.
- Eight input records, six physical buttons plus wheel up/down, share button_mappings. One-byte key_codes differs from the four-byte older mouse layout. Known mouse, disabled, CPI cycle, native keyboard/media and vertical scroll outputs are converted. Primary left click retained.
- Original physical macro triggers1–6 (type0) and wheel49/50 (type1) are separately checked and mapped to logical controls7/8. Shared original event conversion preserves keyboard/mouse events, release order, delays and supported playback options. They remain per-mouse software macros, with input-source verification required before hardware application.
- Four complete set_all_colors records require zone_mask15. Empty or constant matching repeat255 sequences can be expressed as a native steady palette. No replacement of arbitrary dynamic sequences with a firmware rainbow/breathing preset. All-black colors remain an exact black steady palette.
- Brand list and GG import description include original wired Rival3. Actual production UI can inspect, edit, simulate and save the converted full known plan. Quick save retains the editor result.

## Remaining work

Original GG default has303 RGB frames in each of four color sequences. Factory17864 uses a33ms simulation interval and groups logo/z3 together in the editor; this is distinct from the seven native shared patterns in Open HUB. Source timing/stop/repeat semantics and an independently scoped host playback implementation are still required. Default animated GG profiles therefore remain explicitly blocked as a whole; they do not silently lose lighting or save only DPI.

The original configuration can contain fake_settings or later-version lighting history not established in the retained migration. Unknown fields remain blocked until their semantics are verified. Function53 meta actions, optional modifier masks, non-default exact_accel/exact_aim/free_move driver behavior and unknown/custom macro events also remain gaps. Full firmware readback and physical timing/playback evidence are missing.

## Verification

Eight new default tests cover: exact native packets and four-zone cached result for both firmware IDs;84 CPI codes against independent OEM facts; five-stage order/active-slot preservation;17 malformed/dynamic cases; original physical and two wheel macro sources; wrong model/trigger isolation; unchanged read-only database bytes/program association;77 known native action/input combinations.

One separately invoked source test parses the actual unmodified official default SQL literal without executing SQL and confirms the precise dynamic-host-lighting rejection. Three browser cases use authored constant-color GG schema fixtures processed by the native converter, not an observed user database. They retain original mixed macros, edit CPI/Logo color, use both editor/quick-save paths, then pass through production profile store/export/import and actual native codec replay. Browser IPC is mocked; driver success is established separately by Rust replay, not by the mocked UI response. Existing Rival600 GG import UI regression also passes.

See development/2026-10-29-gg-rival3-validation.json for final counts and artifacts.

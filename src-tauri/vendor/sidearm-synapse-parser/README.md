# Adapted Sidearm Synapse parser

Origin: https://github.com/danscMax/Sidearm/tree/master/src-tauri/src/synapse_import
MIT, Copyright (c) 2026 Dennis Max. See LICENSE.

Open HUB adaptations: parser-only library; remove Sidearm AppConfig merger;
separate Rust 2024 crate; distinguish Synapse v3 ZIP from v4 JSON; bounded explicit
file selection, no automatic sibling-directory scanning; preview command and
public-export integration tests. Original unit/property tests retained.

The fixture Sidearm_profile.synapse4 is from the same MIT project at
src-tauri/resources/Sidearm_profile.synapse4. It is a public sample, not a user
export. Parsing this file is not a hardware test.

`raw_source` adds a bounded internal sidecar containing original JSON payloads
or ZIP entry bytes, including unsupported content. It is skipped during serde
serialization. Legacy Naga normalization is unchanged; application-owned exact
model converters can validate the original data instead of trusting that DTO.
The `modelAction` preview variant is a display-only label and cannot execute a
command. Open HUB's strict DA converter uses its own separately validated plan.

Known limits: the mapping tables are based on Naga V2 HyperSpeed. Do not apply
them to other models. No DPI import currently. Unsupported actions and macro
flattening/clamping are surfaced as warnings. Parsed keyboard macro steps are
keystrokes, not lossless down/up timing: the original export must be retained.
This crate does not activate profiles, write HID, or execute macros.

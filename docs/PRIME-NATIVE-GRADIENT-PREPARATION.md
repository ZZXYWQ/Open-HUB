# Prime native gradient codec preparation

2026-10-30. Existing wired Prime only; no increase in partial or complete model counts.

Static inspection of the same GG122 include chain confirms an80-byte native program: section count, initial RGB, little-endian duration and initial tick, four movement bytes, and sixteen RGB/weight records. The HID Feature report includes report ID0 and command62, padded to643 bytes. Source hashes remain in gg122-prime-native-facts.json. No OEM code is executed.

Implemented a typed native Program/Section/Motion codec with checked section capacity and duration domain, zeroed initial tick and reserved records. Existing production static lighting now uses this codec and preserves its exact bytes. Three additional tests cover static equivalence, all16 records and motion/duration offsets, invalid counts/durations, zero-duration static restrictions and zero-weight handling. This is native encoding groundwork; dynamic UI, application schema and original-profile conversion remain unimplemented.

OEM conversion needs more work before admission: colorshift appends a closing initial-color record; breathe expands stops with black transitions and a closing record. The declared16 original colors can exceed the actual16-record output capacity after expansion. We must reject or explicitly represent that mismatch, never silently truncate. Position quantization and minimum-speed evaluation still need independent vectors. Movement fields are encoded but their physical response is unverified.

Readback structures DD/DF/EC return values after report ID without echoed commands. The current generic command-recognition query cannot establish correlation for these replies. No readback is enabled and template/session values remain labelled accordingly.

Temporary decoded sources were hash-checked against the retained facts and removed after research. No installer or release is produced.

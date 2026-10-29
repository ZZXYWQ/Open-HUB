//! Reconcile the system selector, complete palette and indicator together.
//! Does not claim atomic writes or prevent concurrent firmware changes.
use super::*;
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    system: Vec<u8>,
    lights: Vec<u8>,
}
impl Atk {
    fn capture_lights(&mut self) -> Result<Snapshot> {
        self.identify()?;
        let system = self.read(0, 10)?;
        let lights = self.read_blob(44, 40)?;
        self.identify()?;
        Ok(Snapshot { system, lights })
    }
    fn verify_lights(&mut self, expected: &Snapshot) -> Result<()> {
        if self.capture_lights()? != *expected {
            return Err(Error::invalid(
                "COMPX palette, indicator or system selector changed; no blind restoration",
            ));
        }
        Ok(())
    }
    fn commit_lights(&mut self, before: Snapshot, edits: Vec<(usize, Vec<u8>)>) -> Result<()> {
        let mut wanted = before.clone();
        let mut touched = [false; 40];
        if edits.is_empty() || edits.len() > 2 {
            return Err(Error::invalid("COMPX lighting transaction geometry"));
        }
        for (offset, bytes) in &edits {
            if !((*offset < 32 && offset % 4 == 0 && bytes.len() == 4)
                || (*offset == 32 && bytes.len() == 8))
                || touched[*offset..*offset + bytes.len()].iter().any(|v| *v)
            {
                return Err(Error::invalid(
                    "COMPX incomplete or overlapping lighting row",
                ));
            }
            touched[*offset..*offset + bytes.len()].fill(true);
            wanted.lights[*offset..*offset + bytes.len()].copy_from_slice(bytes);
        }
        self.verify_lights(&before)?;
        let mut stage = before.clone();
        let mut attempted = Vec::new();
        let operation = (|| -> Result<()> {
            for (offset, bytes) in &edits {
                if stage.lights[*offset..*offset + bytes.len()] == *bytes {
                    continue;
                }
                self.verify_lights(&stage)?;
                attempted.push((*offset, bytes.len()));
                self.write_transaction_row(44 + *offset as u16, bytes)?;
                stage.lights[*offset..*offset + bytes.len()].copy_from_slice(bytes);
                self.verify_lights(&stage)?;
            }
            self.verify_lights(&wanted)
        })();
        if let Err(error) = operation {
            let recovery = (|| -> Result<()> {
                let mut actual = self.capture_lights()?;
                let mut allowed = before.clone();
                for (offset, len) in &attempted {
                    let current = &actual.lights[*offset..*offset + *len];
                    if current != &before.lights[*offset..*offset + *len]
                        && current != &wanted.lights[*offset..*offset + *len]
                    {
                        return Err(Error::invalid(
                            "COMPX partial or foreign lighting row retained",
                        ));
                    }
                    allowed.lights[*offset..*offset + *len].copy_from_slice(current);
                }
                if actual != allowed {
                    return Err(Error::invalid(
                        "COMPX untouched palette, indicator or system bytes changed",
                    ));
                }
                self.verify_lights(&actual)?;
                for (offset, len) in attempted.iter().rev() {
                    if actual.lights[*offset..*offset + *len]
                        == before.lights[*offset..*offset + *len]
                    {
                        continue;
                    }
                    self.verify_lights(&actual)?;
                    self.write_transaction_row(
                        44 + *offset as u16,
                        &before.lights[*offset..*offset + *len],
                    )?;
                    actual.lights[*offset..*offset + *len]
                        .copy_from_slice(&before.lights[*offset..*offset + *len]);
                    self.verify_lights(&actual)?;
                }
                self.verify_lights(&before)
            })();
            return Err(Error::invalid(format!("{error}; {}", match recovery {
                Ok(()) => "COMPX original lighting rows restored and verified".to_owned(),
                Err(e) => format!("COMPX lighting state uncertain; keep backup and refresh without retry: {e}"),
            })));
        }
        Ok(())
    }
    pub(super) fn apply_palette_color(&mut self, stage: u8, color: Rgb) -> Result<()> {
        let before = self.capture_lights()?;
        let offset = usize::from(stage) * 4;
        if offset >= 32 || sum(&before.lights[offset..offset + 4]) != 0x55 {
            return Err(Error::invalid(
                "COMPX original DPI color checksum; unknown row retained",
            ));
        }
        self.commit_lights(before, vec![(offset, color_row(color))])
    }
    pub(super) fn apply_indicator_row(
        &mut self,
        value: &gearwise_core::model::DpiIndicatorState,
    ) -> Result<()> {
        let before = self.capture_lights()?;
        indicator::decode(&before.lights[32..])?;
        self.commit_lights(before, vec![(32, indicator::encode(value).to_vec())])
    }
    pub(super) fn apply_legacy_light(&mut self, state: &ZoneState) -> Result<()> {
        let before = self.capture_lights()?;
        let mut row = before.lights[32..].to_vec();
        let mut edits = Vec::new();
        if let LightingEffect::Static { color } = state.effect {
            let active = checked_pair(&before.system[4..6])?;
            let count = checked_pair(&before.system[2..4])?;
            if count == 0 || count > self.model.max_stages || active >= count || active >= 8 {
                return Err(Error::invalid("ATK active DPI color index"));
            }
            let offset = usize::from(active) * 4;
            if sum(&before.lights[offset..offset + 4]) != 0x55 {
                return Err(Error::invalid(
                    "COMPX original active color checksum; unknown row retained",
                ));
            }
            edits.push((offset, color_row(color)));
            row[..2].copy_from_slice(&pair(1));
            row[6..8].copy_from_slice(&pair(1));
        } else {
            row[6..8].copy_from_slice(&pair(0));
        }
        edits.push((32, row));
        self.commit_lights(before, edits)
    }
}
fn color_row(color: Rgb) -> Vec<u8> {
    vec![
        color.r,
        color.g,
        color.b,
        0x55u8
            .wrapping_sub(color.r)
            .wrapping_sub(color.g)
            .wrapping_sub(color.b),
    ]
}

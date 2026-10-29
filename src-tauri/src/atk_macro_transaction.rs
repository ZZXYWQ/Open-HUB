//! Reconcile the complete selected slot and current-bank references. No host
//! input, bank switching, cross-bank ownership claim or blind recovery.
use super::*;
#[derive(Clone, PartialEq, Eq)]
struct Snapshot {
    slot: Vec<u8>,
    buttons: Vec<u8>,
}
impl Atk {
    fn capture_macro_state(&mut self, base: u16) -> Result<Snapshot> {
        self.identify()?;
        let size = if (256..448).contains(&base) && (base - 256) % 32 == 0 {
            32
        } else {
            384
        };
        let slot = self.read_blob(base, size)?;
        let buttons = self.read_blob(96, usize::from(self.model.buttons) * 4)?;
        self.identify()?;
        Ok(Snapshot { slot, buttons })
    }
    fn verify_macro_state(&mut self, base: u16, expected: &Snapshot) -> Result<()> {
        if self.capture_macro_state(base)? != *expected {
            return Err(Error::invalid(
                "COMPX macro slot or current-bank bindings changed; no blind restoration",
            ));
        }
        Ok(())
    }
    // No recursive restoration: the whole transaction owns recovery decisions.
    pub(super) fn write_transaction_row(&mut self, address: u16, next: &[u8]) -> Result<()> {
        self.identify()?;
        let packet = frame(7, address, next.len(), next)?;
        if self.io.write(&packet)? != 17 {
            return Err(Error::invalid("COMPX transaction short EEPROM write"));
        }
        std::thread::sleep(Duration::from_millis(10));
        if self.read(address, next.len())? != next {
            return Err(Error::invalid("COMPX transaction row readback mismatch"));
        }
        self.identify()?;
        Ok(())
    }
    pub(super) fn commit_native_macro(
        &mut self,
        id: u16,
        base: u16,
        payload: &[u8],
        original_binding: &[u8],
        next_binding: &[u8],
    ) -> Result<()> {
        if payload.is_empty()
            || payload.len() > 384
            || id == 0
            || id > self.model.buttons
            || base < 768
            || (base - 768) % 384 != 0
            || (base - 768) / 384 >= 12
            || original_binding.len() != 4
            || next_binding.len() != 4
        {
            return Err(Error::invalid("COMPX macro transaction geometry"));
        }
        self.commit_native_record(id, base, payload, original_binding, next_binding, false)
    }
    pub(super) fn commit_native_shortcut(
        &mut self,
        id: u16,
        payload: &[u8],
        original_binding: &[u8],
    ) -> Result<()> {
        if !self.model.native_macros
            || !self.model.native_buttons
            || !(2..=self.model.buttons).contains(&id)
            || payload.is_empty()
            || payload.len() > 20
            || original_binding.len() != 4
        {
            return Err(Error::invalid("COMPX shortcut transaction geometry"));
        }
        let mut proof = vec![0u8; 32];
        proof[..payload.len()].copy_from_slice(payload);
        if payload.len() != 2 + usize::from(proof[0]) * 3 || shortcuts::decode(&proof).is_err() {
            return Err(Error::invalid(
                "COMPX shortcut payload is not a complete verified chord",
            ));
        }
        self.commit_native_record(
            id,
            256 + (id - 1) * 32,
            payload,
            original_binding,
            &[5, 0, 0, 80],
            true,
        )
    }
    fn commit_native_record(
        &mut self,
        id: u16,
        base: u16,
        payload: &[u8],
        original_binding: &[u8],
        next_binding: &[u8],
        shortcut: bool,
    ) -> Result<()> {
        let before = self.capture_macro_state(base)?;
        let target_at = usize::from(id - 1) * 4;
        if &before.buttons[target_at..target_at + 4] != original_binding {
            return Err(Error::invalid(
                "COMPX target binding changed during allocation",
            ));
        }
        let slot = if shortcut {
            0
        } else {
            ((base - 768) / 384) as u8
        };
        let immutable = before.slot[..payload.len()] == *payload;
        let mut other_reference = false;
        for (button, record) in before.buttons.chunks_exact(4).enumerate() {
            if sum(record) != 0x55 {
                return Err(Error::invalid("COMPX allocation binding checksum"));
            }
            if !shortcut && button != usize::from(id - 1) && record[0] == 6 && record[1] == slot {
                other_reference = true;
                if !immutable {
                    return Err(Error::invalid(
                        "COMPX macro slot gained another current-bank reference",
                    ));
                }
            }
        }
        let reusing = original_binding[0] == 6 && original_binding[1] == slot;
        if !shortcut
            && !reusing
            && !(immutable && other_reference)
            && !matches!(before.slot[31], 0 | 255)
        {
            return Err(Error::invalid(
                "COMPX selected free slot is no longer empty",
            ));
        }
        if shortcut
            && !before.slot.iter().all(|b| *b == 0)
            && !before.slot.iter().all(|b| *b == 255)
            && shortcuts::decode(&before.slot).is_err()
        {
            return Err(Error::invalid(
                "COMPX unknown shortcut table retained; no allocation or overwrite",
            ));
        }
        let mut wanted = before.clone();
        wanted.slot[..payload.len()].copy_from_slice(payload);
        wanted.buttons[target_at..target_at + 4].copy_from_slice(next_binding);
        let mut stage = before.clone();
        let mut attempted = Vec::<(usize, usize)>::new();
        let mut binding_attempted = false;
        let operation = (|| -> Result<()> {
            self.verify_macro_state(base, &stage)?;
            for (at, chunk) in payload.chunks(10).enumerate() {
                let offset = at * 10;
                if stage.slot[offset..offset + chunk.len()] == *chunk {
                    continue;
                }
                self.verify_macro_state(base, &stage)?;
                attempted.push((offset, chunk.len()));
                self.write_transaction_row(base + offset as u16, chunk)?;
                stage.slot[offset..offset + chunk.len()].copy_from_slice(chunk);
                self.verify_macro_state(base, &stage)?;
            }
            self.verify_macro_state(base, &stage)?;
            if stage.buttons[target_at..target_at + 4] != *next_binding {
                binding_attempted = true;
                self.write_transaction_row(96 + target_at as u16, next_binding)?;
                stage.buttons[target_at..target_at + 4].copy_from_slice(next_binding);
            }
            self.verify_macro_state(base, &wanted)
        })();
        if let Err(error) = operation {
            let recovery = (|| -> Result<()> {
                let mut actual = self.capture_macro_state(base)?;
                let mut allowed = before.clone();
                for (at, len) in &attempted {
                    let current = &actual.slot[*at..*at + *len];
                    if current != &before.slot[*at..*at + *len]
                        && current != &wanted.slot[*at..*at + *len]
                    {
                        return Err(Error::invalid(
                            "COMPX third-party or partial macro row; no blind restoration",
                        ));
                    }
                    allowed.slot[*at..*at + *len].copy_from_slice(current);
                }
                if binding_attempted {
                    let current = &actual.buttons[target_at..target_at + 4];
                    if current != original_binding && current != next_binding {
                        return Err(Error::invalid(
                            "COMPX third-party macro binding; no blind restoration",
                        ));
                    }
                    allowed.buttons[target_at..target_at + 4].copy_from_slice(current);
                }
                if actual != allowed {
                    return Err(Error::invalid("COMPX untouched slot bytes or other bindings changed; no blind restoration"));
                }
                self.verify_macro_state(base, &actual)?;
                // Remove our new reference before putting original code back.
                if actual.buttons[target_at..target_at + 4] != *original_binding {
                    self.write_transaction_row(96 + target_at as u16, original_binding)?;
                    actual.buttons[target_at..target_at + 4].copy_from_slice(original_binding);
                    self.verify_macro_state(base, &actual)?;
                }
                for (at, len) in attempted.iter().rev() {
                    if actual.slot[*at..*at + *len] == before.slot[*at..*at + *len] {
                        continue;
                    }
                    self.verify_macro_state(base, &actual)?;
                    self.write_transaction_row(base + *at as u16, &before.slot[*at..*at + *len])?;
                    actual.slot[*at..*at + *len].copy_from_slice(&before.slot[*at..*at + *len]);
                    self.verify_macro_state(base, &actual)?;
                }
                self.verify_macro_state(base, &before)
            })();
            return Err(Error::invalid(format!(
                "{error}; {}",
                match recovery {
                    Ok(()) =>
                        if shortcut {
                            "COMPX original shortcut and binding restored and verified".to_string()
                        } else {
                            "COMPX original macro and binding restored and verified".to_string()
                        },
                    Err(e) =>
                        format!("COMPX state uncertain; keep backup and re-read without retry: {e}"),
                }
            )));
        }
        Ok(())
    }
}

//! Reconcile all affected performance rows before writes and recovery.
use super::*;
use gearwise_core::tuning::MouseTuning;
type Row = (u16, Vec<u8>, Vec<u8>);
impl Atk {
    fn capture_tuning_rows(&mut self, rows: &[Row]) -> Result<Vec<Vec<u8>>> {
        self.identify()?;
        let mut out = Vec::new();
        for (address, before, _) in rows {
            out.push(self.read(*address, before.len())?);
        }
        self.identify()?;
        Ok(out)
    }
    fn verify_tuning_rows(&mut self, rows: &[Row], expected: &[Vec<u8>]) -> Result<()> {
        if self.capture_tuning_rows(rows)? != expected {
            return Err(Error::invalid(
                "COMPX performance rows changed; no blind restoration",
            ));
        }
        Ok(())
    }
    pub(super) fn commit_tuning_rows(&mut self, rows: &[Row], patch: &MouseTuning) -> Result<()> {
        if rows.is_empty() || rows.len() > 3 {
            return Err(Error::invalid("COMPX performance transaction geometry"));
        }
        let mut addresses = std::collections::HashSet::new();
        for (address, before, next) in rows {
            let size = match address {
                10 => 2,
                169 => 10,
                181 => 6,
                _ => return Err(Error::invalid("COMPX unverified performance row")),
            };
            if before.len() != size || next.len() != size || !addresses.insert(*address) {
                return Err(Error::invalid(
                    "COMPX overlapping or incomplete performance rows",
                ));
            }
        }
        let before = rows
            .iter()
            .map(|(_, before, _)| before.clone())
            .collect::<Vec<_>>();
        let wanted = rows
            .iter()
            .map(|(_, _, next)| next.clone())
            .collect::<Vec<_>>();
        self.verify_tuning_rows(rows, &before)?;
        let mut stage = before.clone();
        let mut attempted = vec![false; rows.len()];
        let operation = (|| -> Result<()> {
            for (index, (address, _, next)) in rows.iter().enumerate() {
                if stage[index] == *next {
                    continue;
                }
                self.verify_tuning_rows(rows, &stage)?;
                attempted[index] = true;
                self.write_transaction_row(*address, next)?;
                stage[index] = next.clone();
                self.verify_tuning_rows(rows, &stage)?;
            }
            self.verify_tuning_rows(rows, &wanted)?;
            let actual = self.read_tuning()?;
            let mut expected = actual.clone();
            patch.merge(&mut expected);
            if expected != actual {
                return Err(Error::invalid(
                    "COMPX complete performance readback mismatch",
                ));
            }
            self.verify_tuning_rows(rows, &wanted)
        })();
        if let Err(error) = operation {
            let recovery = (|| -> Result<()> {
                let mut actual = self.capture_tuning_rows(rows)?;
                for index in 0..rows.len() {
                    if actual[index] != before[index]
                        && (!attempted[index] || actual[index] != wanted[index])
                    {
                        return Err(Error::invalid(
                            "COMPX third-party or partial performance row; no blind restoration",
                        ));
                    }
                }
                self.verify_tuning_rows(rows, &actual)?;
                for index in (0..rows.len()).rev() {
                    if actual[index] == before[index] {
                        continue;
                    }
                    self.verify_tuning_rows(rows, &actual)?;
                    self.write_transaction_row(rows[index].0, &before[index])?;
                    actual[index] = before[index].clone();
                    self.verify_tuning_rows(rows, &actual)?;
                }
                self.verify_tuning_rows(rows, &before)
            })();
            return Err(Error::invalid(format!("{error}; {}",match recovery{Ok(())=>"performance original rows restoration: confirmed".to_string(),Err(e)=>format!("COMPX performance state uncertain; keep backup and refresh without retry: {e}")})));
        }
        Ok(())
    }
}

use super::*;
pub(super) fn shortcut(usage: u8, mods: Modifiers, no_delay: bool) -> Macro {
    let mut steps = Vec::new();
    for n in 0..8 {
        if mods.0 & (1 << n) != 0 {
            steps.push(MacroStep::KeyDown(0xe0 + n))
        }
    }
    steps.push(MacroStep::KeyDown(usage));
    if !no_delay {
        steps.push(MacroStep::Delay(20))
    }
    steps.push(MacroStep::KeyUp(usage));
    for n in (0..8).rev() {
        if mods.0 & (1 << n) != 0 {
            steps.push(MacroStep::KeyUp(0xe0 + n))
        }
    }
    Macro {
        id: format!("wired-key-{usage}-{}", mods.0),
        name: "Keyboard shortcut".into(),
        mode: MacroMode::Once,
        steps,
    }
}
pub(super) fn validate(model: &Model, m: &Macro) -> Result<()> {
    if !model.macros || m.mode != MacroMode::Once {
        return Err(invalid(
            "this model only supports reviewed single-play native macros",
        ));
    }
    let max = if model.brand == "etekcity" { 50 } else { 500 };
    let mut held = std::collections::BTreeSet::new();
    let mut events = 0;
    for (n, s) in m.steps.iter().enumerate() {
        match s{
 MacroStep::KeyDown(k)|MacroStep::KeyUp(k)=>{let valid=if model.brand=="gskill"{matches!(k,4..=0x6a|0xe0..=0xe7)}else{matches!(k,4..=0xa4|0xe0..=0xe7)};if !valid{return Err(invalid("native macro keyboard usage"))}if matches!(s,MacroStep::KeyDown(_)){if !held.insert(*k){return Err(invalid("duplicate key press"))}}else if !held.remove(k){return Err(invalid("unmatched key release"))}events+=1;},
 MacroStep::Delay(ms)if model.brand!="etekcity"&&*ms>0&&*ms<=10000&&!(model.brand=="roccat"&&n==0)=>{},
 _=>return Err(invalid("native macro only supports balanced keyboard events; Etekcity timing is firmware controlled and cannot encode delays"))}
    }
    if !held.is_empty() || events == 0 || events > max {
        return Err(invalid("native macro event count or held key"));
    }
    Ok(())
}
pub(super) fn is_binding(m: &Model, row: &[u8]) -> bool {
    row[0]
        == match m.brand.as_str() {
            "roccat" => 48,
            "etekcity" => 16,
            "gskill" => 6,
            _ => 255,
        }
        && m.macros
}
fn wire(m: &Model, profile: u8, id: u16, def: &Macro) -> Result<Vec<u8>> {
    validate(m, def)?;
    let button = (id - 1) as u8;
    let mut p = vec![
        0;
        match m.brand.as_str() {
            "roccat" => 2082,
            "etekcity" => 130,
            _ => 2052,
        }
    ];
    if m.brand == "gskill" {
        p[..7].copy_from_slice(&[4, 0, profile * 10 + button, 0, 5, 0, 1]);
        let name = def
            .name
            .encode_utf16()
            .take(120)
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>();
        p[9] = name.len() as u8;
        p[10..10 + name.len()].copy_from_slice(&name);
        let mut bytes = Vec::new();
        for s in &def.steps {
            match s {
                MacroStep::KeyDown(k) | MacroStep::KeyUp(k) => {
                    let c = if *k >= 0xe0 { *k - 0x70 } else { *k };
                    bytes.push(if matches!(s, MacroStep::KeyUp(_)) {
                        c + 0x80
                    } else {
                        c
                    });
                }
                MacroStep::Delay(ms) => {
                    bytes.push(1);
                    bytes.extend_from_slice(&(*ms as u16).to_le_bytes())
                }
                _ => unreachable!(),
            }
        }
        if bytes.len() > 1786 {
            return Err(invalid("G.Skill macro payload exceeds 1786 bytes"));
        }
        p[7..9].copy_from_slice(&(bytes.len() as u16).to_le_bytes());
        p[266..266 + bytes.len()].copy_from_slice(&bytes);
        protocol::gskill_checksum(&mut p);
        return Ok(p);
    }
    let roc = m.brand == "roccat";
    let (name_start, count_start, event_start, event_size) = if roc {
        p[..6].copy_from_slice(&[8, 0x22, 8, profile, button, 1]);
        p[30..32].copy_from_slice(b"g0");
        (54, 78, 80, 4)
    } else {
        p[..5].copy_from_slice(&[9, 0x82, profile, button, 1]);
        (5, 29, 30, 2)
    };
    let name = def.name.as_bytes();
    let mut end = name.len().min(23);
    while !def.name.is_char_boundary(end) {
        end -= 1
    }
    p[name_start..name_start + end].copy_from_slice(&name[..end]);
    let mut count = 0;
    for s in &def.steps {
        match s {
            MacroStep::KeyDown(k) | MacroStep::KeyUp(k) => {
                let a = event_start + count * event_size;
                p[a] = *k;
                p[a + 1] = if matches!(s, MacroStep::KeyDown(_)) {
                    if roc {
                        1
                    } else {
                        0
                    }
                } else if roc {
                    2
                } else {
                    0x80
                };
                if roc {
                    p[a + 2..a + 4].copy_from_slice(
                        &if matches!(s, MacroStep::KeyDown(_)) {
                            10u16
                        } else {
                            50u16
                        }
                        .to_le_bytes(),
                    );
                }
                count += 1;
            }
            MacroStep::Delay(ms) => {
                let a = event_start + (count - 1) * 4 + 2;
                p[a..a + 2].copy_from_slice(&(*ms as u16).to_le_bytes())
            }
            _ => unreachable!(),
        }
    }
    if roc {
        p[count_start..count_start + 2].copy_from_slice(&(count as u16).to_le_bytes());
        protocol::checksum(&mut p);
    } else {
        p[count_start] = count as u8;
    }
    Ok(p)
}
fn read(d: &mut Wired, id: u16) -> Result<Vec<u8>> {
    let profile = d.pinned.ok_or(invalid("profile"))?;
    let button = (id - 1) as u8;
    let p = match d.m.brand.as_str() {
        "gskill" => {
            protocol::gcmd(d, 11, profile * 10 + button, 0, false)?;
            d.sleep(100);
            d.get(4, 2052)?
        }
        "roccat" => {
            protocol::select(d, 0)?;
            protocol::select(d, button)?;
            d.get(8, 2082)?
        }
        _ => {
            protocol::select(d, button)?;
            d.get(9, 130)?
        }
    };
    let ok = match d.m.brand.as_str() {
        "gskill" => {
            p[2] == profile * 10 + button && p[3..].iter().fold(0u8, |a, b| a.wrapping_add(*b)) == 0
        }
        "roccat" => {
            p[0] == 8
                && p[3] == profile
                && p[4] == button
                && p[..2080]
                    .iter()
                    .fold(0u16, |a, b| a.wrapping_add(u16::from(*b)))
                    == u16::from_le_bytes([p[2080], p[2081]])
        }
        _ => p[0] == 9 && p[2] == profile && p[3] == button && p[29] <= 50,
    };
    if ok {
        Ok(p)
    } else {
        Err(invalid("native macro slot identity/checksum"))
    }
}
fn write(d: &mut Wired, id: u16, p: &[u8]) -> Result<()> {
    let profile = d.pinned.ok_or(invalid("profile"))?;
    if protocol::current(d)?.0 != profile {
        return Err(invalid("profile changed before macro write"));
    }
    let mut p = p.to_vec();
    match d.m.brand.as_str() {
        "gskill" => {
            protocol::gcmd(d, 11, profile * 10 + (id - 1) as u8, 1, false)?;
            d.sleep(200);
            p[0] = 4;
            p[1] = 0;
            d.set(&p)?;
        }
        "roccat" => {
            protocol::select(d, (id - 1) as u8)?;
            d.set(&p)?;
            protocol::wait(d)?;
        }
        _ => {
            protocol::select(d, (id - 1) as u8)?;
            d.set(&p)?;
            d.sleep(100);
        }
    }
    Ok(())
}
fn equivalent(m: &Model, a: &[u8], b: &[u8]) -> bool {
    if m.brand == "gskill" {
        a[2..] == b[2..]
    } else {
        a == b
    }
}
pub(super) fn apply(
    d: &mut Wired,
    id: u16,
    def: &Macro,
    before: &Snapshot,
    original_action: ButtonAction,
) -> Result<()> {
    let expected = wire(d.m, before.profile, id, def)?;
    let old = read(d, id)?;
    let mut keys = before.keys.clone();
    let action = match d.m.brand.as_str() {
        "gskill" => vec![6, 0, 0, 0, 0],
        "roccat" => vec![48],
        _ => vec![16],
    };
    protocol::replace_row(d.m, &mut keys, id, &action)?;
    let result = (|| {
        write(d, id, &expected)?;
        if !equivalent(d.m, &read(d, id)?, &expected) {
            return Err(invalid("native macro payload readback mismatch"));
        }
        protocol::write_keys(d, &keys)?;
        let prior = d.owned.insert(id, def.clone());
        let prior_action = d.owned_actions.insert(id, original_action.clone());
        let verified = d.verify(&Setting::Button {
            id,
            action: original_action,
        });
        if verified.is_err() {
            if let Some(a) = prior_action {
                d.owned_actions.insert(id, a);
            } else {
                d.owned_actions.remove(&id);
            }
            if let Some(prior) = prior {
                d.owned.insert(id, prior);
            } else {
                d.owned.remove(&id);
            }
        }
        verified
    })();
    if let Err(error) = result {
        let restore = (|| {
            if protocol::current(d)?.0 != before.profile {
                return Err(invalid("profile changed; macro restoration refused"));
            }
            write(d, id, &old)?;
            protocol::write_keys(d, &before.keys)
        })();
        return Err(invalid(&format!(
            "macro write failed: {error}; restoration: {restore:?}"
        )));
    }
    Ok(())
}

pub(super) fn owned_valid(d: &mut Wired, id: u16) -> Result<bool> {
    let Some(def) = d.owned.get(&id).cloned() else {
        return Ok(false);
    };
    let expected = wire(d.m, d.pinned.ok_or(invalid("profile"))?, id, &def)?;
    Ok(equivalent(d.m, &read(d, id)?, &expected))
}

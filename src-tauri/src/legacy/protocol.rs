use super::*;
fn le(p: &[u8], n: usize) -> u16 {
    u16::from_le_bytes([p[n], p[n + 1]])
}
fn put(p: &mut [u8], n: usize, v: u16) {
    p[n..n + 2].copy_from_slice(&v.to_le_bytes());
}
pub(super) fn checksum(p: &mut [u8]) {
    let sum = p[..p.len() - 2]
        .iter()
        .fold(0u16, |a, b| a.wrapping_add(u16::from(*b)));
    let n = p.len() - 2;
    put(p, n, sum);
}
fn crc_valid(p: &[u8]) -> bool {
    p[..p.len() - 2]
        .iter()
        .fold(0u16, |a, b| a.wrapping_add(u16::from(*b)))
        == le(p, p.len() - 2)
}
pub(super) fn gskill_checksum(p: &mut [u8]) {
    p[3] = 0u8.wrapping_sub(p[4..].iter().fold(0u8, |a, b| a.wrapping_add(*b)));
}
fn gskill_valid(p: &[u8]) -> bool {
    p[3..].iter().fold(0u8, |a, b| a.wrapping_add(*b)) == 0
}
pub(super) fn asus(d: &mut Wired, cmd: u16, params: &[u8]) -> Result<Vec<u8>> {
    let mut p = [0; 64];
    p[..2].copy_from_slice(&cmd.to_le_bytes());
    p[2..2 + params.len()].copy_from_slice(params);
    if d.io.write(&p)? != 64 {
        return Err(invalid("short ASUS output write"));
    }
    // A numbered report (0x12 for reads, 0x51 for settings), no zero prefix.
    let mut r = vec![0; 64];
    for _ in 0..8 {
        let n = d.io.read_timeout(&mut r, 80)?;
        if n == 0 {
            continue;
        }
        if n != 64 {
            return Err(invalid("ASUS input length"));
        }
        if le(&r, 0) == 0xaaff {
            return Err(invalid("ASUS device sleeping or command rejected"));
        }
        return Ok(r);
    }
    Err(Error::Timeout)
}
pub(super) fn gcmd(d: &mut Wired, cmd: u8, index: u8, read: u8, status: bool) -> Result<Vec<u8>> {
    let p = [12, 0xc4, cmd, index, read, 0, 0, 0, 0];
    d.set(&p)?;
    if !status {
        return Ok(p.to_vec());
    }
    for _ in 0..10 {
        d.sleep(20);
        let r = d.get(0, 9)?;
        match r[1] {
            0 | 0xb0 => return Ok(r),
            0xb1 => continue,
            _ => return Err(invalid("G.Skill command status")),
        }
    }
    Err(Error::Timeout)
}
pub(super) fn wait(d: &mut Wired) -> Result<()> {
    if d.m.brand != "roccat" {
        d.sleep(100);
        return Ok(());
    }
    for _ in 0..10 {
        let p = d.get(4, 3)?;
        match p[1] {
            1 => return Ok(()),
            3 => d.sleep(10),
            _ => return Err(invalid("Roccat ready status")),
        }
    }
    Err(Error::Timeout)
}
pub(super) fn select(d: &mut Wired, kind: u8) -> Result<()> {
    let profile = d.pinned.ok_or(invalid("profile not pinned"))?;
    d.set(&[4, profile, kind])?;
    wait(d)
}
fn mars_request(
    d: &mut Wired,
    cmd: u8,
    index: u8,
    size: u8,
    id: u8,
    len: usize,
) -> Result<Vec<u8>> {
    let mut p = [0; 16];
    p[..8].copy_from_slice(&[2, 3, cmd, index, size, 0, 0xfa, 0xfa]);
    d.set(&p)?;
    let r = d.get(id, len)?;
    if r[2] != cmd || r[3] != index || r[4] != size {
        return Err(invalid("MM4 query identity"));
    }
    Ok(r)
}
pub(super) fn current(d: &mut Wired) -> Result<(u8, Option<u8>)> {
    let out = match d.m.brand.as_str() {
        "asus" => {
            let r = asus(d, 0x0012, &[])?;
            (r[10], r[11].checked_sub(1))
        }
        "gskill" => (gcmd(d, 7, 0, 1, true)?[3], None),
        "marsgaming" => (mars_request(d, 0x43, 0, 1, 2, 16)?[8], None),
        _ => {
            let p = d.get(5, 3)?;
            if p[0] != 5 {
                return Err(invalid("profile report id"));
            }
            (p[2], None)
        }
    };
    if out.0 >= if d.m.brand == "asus" { 1 } else { 5 } {
        return Err(invalid("active profile index"));
    }
    Ok(out)
}
fn unpack_dpi(
    m: &Model,
    values: &[(u16, u16)],
    physical: &[usize],
    active: usize,
) -> Result<DpiState> {
    if physical.is_empty() || physical.len() > m.stages as usize {
        return Err(invalid("DPI mask/count"));
    }
    let active = physical
        .iter()
        .position(|v| *v == active)
        .ok_or(invalid("active DPI disabled"))?;
    let stages = physical
        .iter()
        .map(|i| DpiStage {
            x: values[*i].0,
            y: values[*i].1,
        })
        .collect::<Vec<_>>();
    // Read broader Roccat factory XY ranges, but do not silently snap them.
    if stages.iter().any(|s| {
        s.x == 0
            || s.y == 0
            || s.x > if m.brand == "roccat" { 8200 } else { m.max }
            || s.y > if m.brand == "roccat" { 5000 } else { m.max }
    }) {
        return Err(invalid("device DPI response out of range"));
    }
    Ok(DpiState {
        stages,
        active: active as u8,
    })
}
pub(super) fn snapshot(d: &mut Wired) -> Result<Snapshot> {
    let (profile, active) = current(d)?;
    if d.pinned.is_some_and(|v| v != profile) {
        return Err(invalid("active hardware profile changed"));
    }
    d.pinned = Some(profile);
    let (config, keys, light, dpi, rate) = match d.m.brand.as_str() {
        "asus" => {
            let c = asus(d, 0x0412, &[0])?;
            let k = asus(d, 0x0512, &[0])?;
            let l = asus(d, 0x0312, &[0])?;
            let vals = (0..2)
                .map(|n| {
                    let v = le(&c, 4 + 2 * n)
                        .checked_mul(50)
                        .and_then(|v| v.checked_add(50))
                        .ok_or(invalid("ASUS DPI overflow"))?;
                    Ok((v, v))
                })
                .collect::<Result<Vec<_>>>()?;
            let a = active.ok_or(invalid("ASUS active DPI preset unavailable"))?;
            let r = le(&c, 8);
            if r >= 4 {
                return Err(invalid("ASUS rate index"));
            }
            let dpi = unpack_dpi(d.m, &vals, &[0, 1], a as usize)?;
            (c, k, l, dpi, [125, 250, 500, 1000][r as usize])
        }
        "gskill" => {
            gcmd(d, 12, profile, 0, false)?;
            d.sleep(100);
            let c = d.get(5, 644)?;
            if c[2] != profile || !gskill_valid(&c) {
                return Err(invalid("G.Skill profile index/checksum"));
            }
            let count = c[7] >> 4;
            let a = c[7] & 15;
            let vals = (0..5)
                .map(|n| (u16::from(c[8 + 2 * n]) * 50, u16::from(c[9 + 2 * n]) * 50))
                .collect::<Vec<_>>();
            let physical = (0..count as usize).collect::<Vec<_>>();
            let dpi = unpack_dpi(d.m, &vals, &physical, a as usize)?;
            let r = 1000 / (u16::from(c[4] & 15) + 1);
            if !d.m.rates.contains(&r) {
                return Err(invalid(
                    "G.Skill polling index outside reviewed 500/1000 Hz",
                ));
            }
            (c.clone(), c, vec![], dpi, r)
        }
        "marsgaming" => {
            let c = mars_request(d, 0x4f, profile, 0x2a, 3, 64)?;
            let k = mars_request(d, 0x90, profile, 0x4d, 4, 1024)?;
            let l = mars_request(d, 0xf1, profile, 6, 4, 16)?;
            let interval = mars_request(d, 0x48 | profile, 0, 1, 2, 16)?[8];
            if ![1, 2, 4, 8].contains(&interval) || k[8] < 19 {
                return Err(invalid("MM4 button count or polling interval"));
            }
            let count = usize::from(c[8]);
            if count == 0 || count > 5 {
                return Err(invalid("MM4 resolution count outside reviewed five stages"));
            }
            let physical = (0..count)
                .filter(|n| c[10 + n * 8] != 0)
                .collect::<Vec<_>>();
            let vals = (0..6)
                .map(|n| {
                    (
                        le(&c, 11 + n * 8).saturating_mul(50),
                        le(&c, 13 + n * 8).saturating_mul(50),
                    )
                })
                .collect::<Vec<_>>();
            let dpi = unpack_dpi(d.m, &vals, &physical, c[9] as usize)?;
            (c, k, l, dpi, 1000 / u16::from(interval))
        }
        _ => {
            let roc = d.m.brand == "roccat";
            select(d, if roc { 0x80 } else { 0x10 })?;
            let c = d.get(6, if roc { 31 } else { 40 })?;
            select(d, if roc { 0x90 } else { 0x20 })?;
            let k = d.get(7, if roc { 59 } else { 50 })?;
            if c[0] != 6 || c[2] != profile || k[0] != 7 || k[2] != profile || roc && !crc_valid(&k)
            {
                return Err(invalid("settings/keymap report identity or CRC"));
            }
            let (mask, x, y, a, r, n) = if roc {
                (6, 7, 13, 12, 19, 5)
            } else {
                (5, 6, 12, 18, 26, 6)
            };
            let bits = c[mask];
            if bits == 0 || bits & !((1u8 << n) - 1) != 0 || c[r] >= 4 {
                return Err(invalid("DPI mask or rate index"));
            }
            let physical = (0..n).filter(|i| bits & (1 << i) != 0).collect::<Vec<_>>();
            let vals = (0..n)
                .map(|i| (u16::from(c[x + i]) * 50, u16::from(c[y + i]) * 50))
                .collect::<Vec<_>>();
            let dpi = unpack_dpi(d.m, &vals, &physical, c[a] as usize)?;
            let rate = [125, 250, 500, 1000][c[r] as usize];
            (c, k, vec![], dpi, rate)
        }
    };
    if current(d)?.0 != profile {
        return Err(invalid("profile changed during snapshot"));
    }
    Ok(Snapshot {
        profile,
        config,
        keys,
        light,
        dpi,
        rate,
    })
}
pub(super) fn row<'a>(m: &Model, p: &'a [u8], id: u16) -> Result<&'a [u8]> {
    if id == 0 || id > m.buttons {
        return Err(invalid("button index"));
    }
    let n = usize::from(id - 1);
    let (start, len) = match m.brand.as_str() {
        "asus" => (4 + [0, 1, 2, 5][n] * 2, 2),
        "gskill" => (311 + n * 5, 5),
        "marsgaming" => (9 + n * 4, 4),
        "etekcity" => (3 + (if n < 8 { n } else { n + 5 }) * 3, 3),
        _ => (3 + n * 3, 3),
    };
    p.get(start..start + len)
        .ok_or(invalid("button report length"))
}
pub(super) fn replace_row(m: &Model, p: &mut [u8], id: u16, value: &[u8]) -> Result<()> {
    let old = row(m, p, id)?;
    if value.len() > old.len() {
        return Err(invalid("button action length"));
    }
    let offset = old.as_ptr() as usize - p.as_ptr() as usize;
    p[offset..offset + value.len()].copy_from_slice(value);
    Ok(())
}
pub(super) fn write_keys(d: &mut Wired, p: &[u8]) -> Result<()> {
    let mut p = p.to_vec();
    match d.m.brand.as_str() {
        "gskill" => {
            gskill_checksum(&mut p);
            gcmd(d, 12, p[2], 1, false)?;
            d.sleep(200);
            p[0] = 5;
            d.set(&p)?;
            gcmd(d, 0, 0, 0, true)?;
        }
        "marsgaming" => {
            p[1] = 2;
            p[6] = 0xfa;
            p[7] = 0xfa;
            d.set(&p)?;
        }
        _ => {
            let roc = d.m.brand == "roccat";
            if roc {
                checksum(&mut p)
            }
            select(d, if roc { 0x90 } else { 0x20 })?;
            d.set(&p)?;
            wait(d)?;
        }
    }
    Ok(())
}
fn write_config(d: &mut Wired, p: &[u8]) -> Result<()> {
    match d.m.brand.as_str() {
        "gskill" => write_keys(d, p),
        "marsgaming" => {
            let mut p = p.to_vec();
            p[1] = 2;
            p[6] = 0xfa;
            p[7] = 0xfa;
            d.set(&p)
        }
        _ => {
            select(d, if d.m.brand == "roccat" { 0x80 } else { 0x10 })?;
            d.set(p)?;
            wait(d)
        }
    }
}
pub(super) fn write_setting(d: &mut Wired, b: &Snapshot, s: &Setting) -> Result<()> {
    if current(d)?.0 != b.profile {
        return Err(invalid("profile changed before write"));
    }
    if d.m.brand == "asus" {
        match s {
            Setting::Dpi(v) => {
                for (n, v) in v.stages.iter().enumerate() {
                    let raw = (v.x - 50) / 50;
                    asus(d, 0x3151, &[n as u8, 0, raw as u8, (raw >> 8) as u8])?;
                }
            }
            Setting::ReportRate(r) => {
                let index =
                    d.m.rates
                        .iter()
                        .position(|v| v == r)
                        .ok_or(invalid("rate"))?;
                asus(d, 0x3151, &[2, 0, index as u8])?;
            }
            Setting::Button { id, action } => {
                let a = encode_action(d.m, *id, action)?;
                let source = [0xf0, 0xf1, 0xf2, 0xe6][usize::from(*id - 1)];
                asus(d, 0x2151, &[0, 0, source, 1, a[0], a[1]])?;
            }
            Setting::Lighting { state, .. } => {
                let p = light_bytes(d.m, *state);
                asus(d, 0x2851, &[0, 0, p[0], p[1], p[2], p[3], p[4]])?;
            }
            _ => return Err(invalid("setting")),
        }
        asus(d, 0x0350, &[])?;
        return Ok(());
    }
    match s {
        Setting::Dpi(v) => {
            let mut p = b.config.clone();
            let count = v.stages.len();
            match d.m.brand.as_str() {
                "gskill" => {
                    p[7] = ((count as u8) << 4) | v.active;
                    for (n, v) in v.stages.iter().enumerate() {
                        p[8 + n * 2] = (v.x / 50) as u8;
                        p[9 + n * 2] = (v.y / 50) as u8;
                    }
                }
                "marsgaming" => {
                    p[8] = count as u8;
                    p[9] = v.active;
                    for n in 0..5 {
                        p[10 + n * 8] = u8::from(n < count);
                        if let Some(v) = v.stages.get(n) {
                            put(&mut p, 11 + n * 8, v.x / 50);
                            put(&mut p, 13 + n * 8, v.y / 50);
                            p[15 + n * 8] = ((1u16 << n) - 1) as u8;
                        }
                    }
                }
                _ => {
                    let (mask, x, y, a) = if d.m.brand == "roccat" {
                        (6, 7, 13, 12)
                    } else {
                        (5, 6, 12, 18)
                    };
                    p[mask] = ((1u16 << count) - 1) as u8;
                    p[a] = v.active;
                    for (n, v) in v.stages.iter().enumerate() {
                        p[x + n] = (v.x / 50) as u8;
                        p[y + n] = (v.y / 50) as u8;
                    }
                }
            }
            write_config(d, &p)
        }
        Setting::ReportRate(r) => {
            if d.m.brand == "marsgaming" {
                let mut p = [0; 16];
                p[..9].copy_from_slice(&[
                    2,
                    2,
                    0x48 | b.profile,
                    0,
                    1,
                    0,
                    0xfa,
                    0xfa,
                    (1000 / r) as u8,
                ]);
                d.set(&p)
            } else {
                let mut p = b.config.clone();
                if d.m.brand == "gskill" {
                    p[4] = (p[4] & 0xf0) | ((1000 / r - 1) as u8)
                } else {
                    let n = if d.m.brand == "roccat" { 19 } else { 26 };
                    p[n] =
                        d.m.rates
                            .iter()
                            .position(|x| x == r)
                            .ok_or(invalid("rate"))? as u8;
                }
                write_config(d, &p)
            }
        }
        Setting::Button { id, action } => {
            let mut k = b.keys.clone();
            replace_row(d.m, &mut k, *id, &encode_action(d.m, *id, action)?)?;
            write_keys(d, &k)
        }
        Setting::Lighting { state, .. } => {
            write_mars_light(d, b.profile, &light_bytes(d.m, *state))
        }
        _ => Err(invalid("setting")),
    }
}
fn write_mars_light(d: &mut Wired, profile: u8, bytes: &[u8]) -> Result<()> {
    let mut live = [0; 16];
    live[..8].copy_from_slice(&[2, 4, bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], 1]);
    d.set(&live)?;
    let mut stored = [0; 16];
    stored[..8].copy_from_slice(&[2, 2, 0xf1, profile, 6, 0, 0xfa, 0xfa]);
    stored[8..13].copy_from_slice(bytes);
    d.set(&stored)
}
pub(super) fn restore_setting(d: &mut Wired, b: &Snapshot, s: &Setting) -> Result<()> {
    if current(d)?.0 != b.profile {
        return Err(invalid("profile changed; restoration refused"));
    }
    if d.m.brand == "asus" {
        return match s {
            Setting::Dpi(_) => write_setting(d, b, &Setting::Dpi(b.dpi.clone())),
            Setting::ReportRate(_) => write_setting(d, b, &Setting::ReportRate(b.rate)),
            Setting::Button { id, .. } => {
                let old = row(d.m, &b.keys, *id)?;
                let source = [0xf0, 0xf1, 0xf2, 0xe6][usize::from(*id - 1)];
                asus(d, 0x2151, &[0, 0, source, 1, old[0], old[1]])?;
                asus(d, 0x0350, &[])?;
                Ok(())
            }
            Setting::Lighting { .. } => {
                asus(
                    d,
                    0x2851,
                    &[
                        0, 0, b.light[4], b.light[5], b.light[6], b.light[7], b.light[8],
                    ],
                )?;
                asus(d, 0x0350, &[])?;
                Ok(())
            }
            _ => Err(invalid("restore setting")),
        };
    }
    match s {
        Setting::Dpi(_) => write_config(d, &b.config),
        Setting::ReportRate(_) if d.m.brand == "marsgaming" => {
            write_setting(d, b, &Setting::ReportRate(b.rate))
        }
        Setting::ReportRate(_) => write_config(d, &b.config),
        Setting::Button { .. } => write_keys(d, &b.keys),
        Setting::Lighting { .. } => write_mars_light(d, b.profile, &b.light[8..13]),
        _ => Err(invalid("restore")),
    }
}
pub(super) fn light_bytes(m: &Model, s: ZoneState) -> Vec<u8> {
    let color = match s.effect {
        LightingEffect::Static { color } | LightingEffect::Breathing { color, .. } => color,
        _ => Rgb::BLACK,
    };
    let off = matches!(s.effect, LightingEffect::Off);
    let brightness = if off {
        0
    } else {
        (u16::from(s.brightness) * if m.brand == "asus" { 4 } else { 3 } / 100) as u8
    };
    if m.brand == "asus" {
        vec![
            match s.effect {
                LightingEffect::Breathing { .. } => 1,
                LightingEffect::ColorCycle { .. } => 2,
                _ => 0,
            },
            brightness,
            color.r,
            color.g,
            color.b,
        ]
    } else {
        vec![
            255 - color.r,
            255 - color.g,
            255 - color.b,
            brightness,
            match s.effect {
                LightingEffect::Breathing { period_ms, .. } => (period_ms / 2000) as u8,
                _ => 0,
            },
        ]
    }
}
pub(super) fn decode_light(m: &Model, p: &[u8]) -> Option<ZoneState> {
    let (a, c) = if m.brand == "asus" {
        (4, Rgb::new(*p.get(6)?, *p.get(7)?, *p.get(8)?))
    } else {
        (
            8,
            Rgb::new(255 - *p.get(8)?, 255 - *p.get(9)?, 255 - *p.get(10)?),
        )
    };
    let (brightness, effect) = if m.brand == "asus" {
        if p[a + 1] > 4 {
            return None;
        }
        (
            p[a + 1] * 25,
            if p[a + 1] == 0 {
                LightingEffect::Off
            } else {
                match p[a] {
                    0 => LightingEffect::Static { color: c },
                    1 => LightingEffect::Breathing {
                        color: c,
                        period_ms: 0,
                    },
                    2 => LightingEffect::ColorCycle { period_ms: 0 },
                    _ => return None,
                }
            },
        )
    } else {
        if p[a + 3] > 3 {
            return None;
        }
        (
            (u16::from(p[a + 3]) * 100 / 3) as u8,
            if p[a + 3] == 0 {
                LightingEffect::Off
            } else if p[a + 4] == 0 || p[a + 4] >= 10 {
                LightingEffect::Static { color: c }
            } else {
                LightingEffect::Breathing {
                    color: c,
                    period_ms: u16::from(p[a + 4]) * 2000,
                }
            },
        )
    };
    Some(ZoneState { effect, brightness })
}
pub(super) fn quantized_light(m: &Model, s: ZoneState) -> ZoneState {
    let mut p = vec![0; if m.brand == "asus" { 64 } else { 16 }];
    let a = if m.brand == "asus" { 4 } else { 8 };
    p[a..a + 5].copy_from_slice(&light_bytes(m, s));
    decode_light(m, &p).unwrap()
}

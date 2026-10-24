//! In-development protocol codecs. No HID handles, discovery or UI claims here.
//! Sources and limits: MULTIBRAND-PROTOCOL-WORK.md. Report bytes exclude Windows
//! HID's leading report-ID byte unless the upstream output format includes it.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Binding {
    Mouse { button: u8 },
    Key { key: u8, modifiers: Vec<u8> },
    DpiCycle {},
    Disabled {},
    Macro { events: Vec<KeyEvent> },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyEvent {
    pub key: u8,
    pub down: bool,
    pub delay_ms: u16,
}

fn dpi(value: u16, max: u16, step: u16) -> Result<(), String> {
    if value < 100 || value > max || value % step != 0 {
        return Err(format!("DPI must be 100..{max}, step {step}"));
    }
    Ok(())
}

/// OpenRazer 90-byte command, not including HID report ID.
fn razer_command(class: u8, command: u8, arguments: &[u8]) -> Result<Vec<u8>, String> {
    if arguments.len() > 80 {
        return Err("Razer argument overflow".into());
    }
    let mut data = vec![0; 90];
    data[1] = 0x3f; // DeathAdder Elite transaction ID, not a brand-wide default.
    data[5] = arguments.len() as u8;
    data[6] = class;
    data[7] = command;
    data[8..8 + arguments.len()].copy_from_slice(arguments);
    data[88] = data[2..88].iter().fold(0, |a, b| a ^ b);
    Ok(data)
}
pub fn deathadder_elite_dpi(x: u16, y: u16) -> Result<Vec<u8>, String> {
    dpi(x, 16000, 1)?;
    dpi(y, 16000, 1)?;
    let [xh, xl] = x.to_be_bytes();
    let [yh, yl] = y.to_be_bytes();
    razer_command(4, 5, &[1, xh, xl, yh, yl, 0, 0])
}
pub fn deathadder_elite_rate(rate: u32) -> Result<Vec<u8>, String> {
    let code = match rate {
        1000 => 1,
        500 => 2,
        125 => 8,
        _ => return Err("Unsupported DeathAdder Elite polling rate".into()),
    };
    razer_command(0, 5, &[code])
}
/// Validate CRC, exact transaction and command, final packet and success status.
/// A successful transport send alone is never a successful Razer operation.
pub fn razer_response(request: &[u8], response: &[u8]) -> Result<(), String> {
    if request.len() != 90 || response.len() != 90 {
        return Err("Truncated Razer report".into());
    }
    if response[88] != response[2..88].iter().fold(0, |a, b| a ^ b) {
        return Err("Razer CRC mismatch".into());
    }
    if response[1..5] != request[1..5]
        || response[5] != request[5]
        || response[6..8] != request[6..8]
        || response[89] != 0
    {
        return Err("Razer response identity/shape mismatch".into());
    }
    match response[0] {
        2 => Ok(()),
        1 => Err("Razer device busy; operation not complete".into()),
        3 => Err("Razer device rejected command".into()),
        5 => Err("Razer command unsupported".into()),
        _ => Err("Razer operation did not complete".into()),
    }
}

/// ckb-next NXP volatile DPI stage write. Does not change active stage, mask,
/// colours, firmware or saved hardware profile. Transport/startup still required.
pub fn corsair_m65_pro_dpi(stage: u8, x: u16, y: u16) -> Result<Vec<u8>, String> {
    if stage >= 6 {
        return Err("Corsair DPI stage must be 0..5".into());
    }
    dpi(x, 12000, 1)?;
    dpi(y, 12000, 1)?;
    let mut data = vec![0; 64];
    data[..4].copy_from_slice(&[7, 0x13, 0xd0 | stage, 0]);
    data[4] = u8::from(x != y);
    data[5..7].copy_from_slice(&x.to_le_bytes());
    data[7..9].copy_from_slice(&y.to_le_bytes());
    Ok(data)
}

/// Rival 310, USB 1038:1720, libratbag protocol version 2.
/// Each returned vector includes the zero output report ID used by upstream.
pub fn rival310_dpi(stage: u8, value: u16) -> Result<Vec<u8>, String> {
    if stage > 1 {
        return Err("Rival 310 has two DPI stages".into());
    }
    dpi(value, 12000, 100)?;
    let mut data = vec![0; 64];
    data[1] = 0x53;
    data[3] = stage + 1;
    data[4] = (value / 100 - 1) as u8;
    data[7] = 0x42;
    Ok(data)
}
pub fn rival310_rate(rate: u32) -> Result<Vec<u8>, String> {
    if ![125, 250, 500, 1000].contains(&rate) {
        return Err("Unsupported Rival 310 report rate".into());
    }
    let mut data = vec![0; 64];
    data[1] = 0x54;
    data[3] = (1000 / rate) as u8;
    Ok(data)
}
pub fn rival310_buttons(bindings: &[Binding]) -> Result<Vec<u8>, String> {
    if bindings.len() != 6 {
        return Err("Rival 310 needs exactly six bindings".into());
    }
    if !bindings
        .iter()
        .any(|b| matches!(b, Binding::Mouse { button: 1 }))
    {
        return Err("Keep a direct left click".into());
    }
    let mut data = vec![0; 262];
    data[1] = 0x31;
    for (i, b) in bindings.iter().enumerate() {
        let at = 3 + i * 5;
        match b {
            Binding::Mouse{button} if (1..=5).contains(button)=>data[at]=*button,
            Binding::Key{key,modifiers}=>{
                if !(4..=0xa4).contains(key)||modifiers.len()>3||modifiers.iter().any(|v|!(0xe0..=0xe7).contains(v)) {
                    return Err("Rival 310 shortcut requires one HID key and up to three modifiers".into());
                }
                let mut unique=modifiers.clone();unique.sort_unstable();unique.dedup();
                if unique.len()!=modifiers.len(){return Err("Duplicate shortcut modifier".into());}
                data[at]=0x51;data[at+1..at+1+modifiers.len()].copy_from_slice(modifiers);data[at+1+modifiers.len()]=*key;
            }
            Binding::DpiCycle {}=>data[at]=0x30,
            Binding::Disabled {}=>{},
            Binding::Macro{..}=>return Err("Rival 310 protocol supports shortcuts, not timed macros; host macro engine is not implemented".into()),
            _=>return Err("Unsupported mouse button".into()),
        }
    }
    Ok(data)
}
/// No silent truncation of timed macros to shortcuts. Useful before either a
/// future host-macro scheduler or an onboard-macro codec accepts imported data.
pub fn validate_macro(events: &[KeyEvent]) -> Result<(), String> {
    if events.is_empty() || events.len() > 256 {
        return Err("Macro must contain 1..256 events".into());
    }
    let mut held = std::collections::HashSet::new();
    let mut duration = 0u32;
    for event in events {
        if !((4..=0xa4).contains(&event.key) || (0xe0..=0xe7).contains(&event.key))
            || event.delay_ms > 10000
        {
            return Err("Invalid macro key/delay".into());
        }
        duration += u32::from(event.delay_ms);
        if event.down {
            if !held.insert(event.key) {
                return Err("Duplicate key down".into());
            }
        } else if !held.remove(&event.key) {
            return Err("Key up without key down".into());
        }
    }
    if !held.is_empty() || duration > 60000 {
        return Err("Unreleased key or macro longer than 60 seconds".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn standard() -> Vec<Binding> {
        vec![
            Binding::Mouse { button: 1 },
            Binding::Mouse { button: 2 },
            Binding::Mouse { button: 3 },
            Binding::Mouse { button: 4 },
            Binding::Mouse { button: 5 },
            Binding::DpiCycle {},
        ]
    }
    #[test]
    fn razer_dpi_golden() {
        let p = deathadder_elite_dpi(800, 1600).unwrap();
        assert_eq!(
            &p[..15],
            &[0, 0x3f, 0, 0, 0, 7, 4, 5, 1, 3, 0x20, 6, 0x40, 0, 0]
        );
        assert_eq!(p.len(), 90);
        assert_eq!(p[88], 0x62);
    }
    #[test]
    fn razer_rate_golden() {
        let p = deathadder_elite_rate(125).unwrap();
        assert_eq!(&p[5..9], &[1, 0, 5, 8]);
        assert_eq!(p[88], 12);
    }
    #[test]
    fn razer_rejects_unsupported_values() {
        assert!(deathadder_elite_dpi(0, 800).is_err());
        assert!(deathadder_elite_dpi(16001, 800).is_err());
        assert!(deathadder_elite_rate(250).is_err());
        assert!(razer_command(0, 0, &[0; 81]).is_err());
    }
    fn response() -> (Vec<u8>, Vec<u8>) {
        let q = deathadder_elite_dpi(800, 800).unwrap();
        let mut r = q.clone();
        r[0] = 2;
        (q, r)
    }
    #[test]
    fn razer_response_success() {
        let (q, r) = response();
        assert!(razer_response(&q, &r).is_ok());
    }
    #[test]
    fn razer_response_rejects_busy() {
        let (q, mut r) = response();
        for s in [0, 1, 3, 4, 5, 255] {
            r[0] = s;
            assert!(razer_response(&q, &r).is_err());
        }
    }
    #[test]
    fn razer_response_rejects_wrong_identity() {
        let (q, mut r) = response();
        r[1] ^= 1;
        assert!(razer_response(&q, &r).is_err());
    }
    #[test]
    fn razer_response_rejects_corruption() {
        let (q, mut r) = response();
        r[10] ^= 1;
        assert!(razer_response(&q, &r).is_err());
        assert!(razer_response(&q, &r[..89]).is_err());
    }
    #[test]
    fn corsair_xy_golden() {
        let p = corsair_m65_pro_dpi(2, 800, 1600).unwrap();
        assert_eq!(&p[..9], &[7, 0x13, 0xd2, 0, 1, 0x20, 3, 0x40, 6]);
        assert!(p[9..].iter().all(|b| *b == 0));
        assert_eq!(p.len(), 64);
    }
    #[test]
    fn corsair_limits() {
        assert!(corsair_m65_pro_dpi(6, 800, 800).is_err());
        assert!(corsair_m65_pro_dpi(0, 12001, 800).is_err());
        assert_eq!(corsair_m65_pro_dpi(0, 800, 800).unwrap()[4], 0);
    }
    #[test]
    fn steel_dpi_golden() {
        let p = rival310_dpi(1, 12000).unwrap();
        assert_eq!(&p[..8], &[0, 0x53, 0, 2, 119, 0, 0, 0x42]);
        assert_eq!(p.len(), 64);
    }
    #[test]
    fn steel_dpi_rejects_rounding() {
        assert!(rival310_dpi(0, 850).is_err());
        assert!(rival310_dpi(2, 800).is_err());
    }
    #[test]
    fn steel_rate_1000_not_u8_overflow() {
        for (r, c) in [(125, 8), (250, 4), (500, 2), (1000, 1)] {
            assert_eq!(rival310_rate(r).unwrap()[3], c);
        }
        assert!(rival310_rate(8000).is_err());
    }
    #[test]
    fn steel_buttons_golden() {
        let mut b = standard();
        b[4] = Binding::Key {
            key: 4,
            modifiers: vec![0xe0, 0xe1, 0xe2],
        };
        let p = rival310_buttons(&b).unwrap();
        assert_eq!(p.len(), 262);
        assert_eq!(p[1], 0x31);
        assert_eq!(&p[23..28], &[0x51, 0xe0, 0xe1, 0xe2, 4]);
        assert_eq!(p[28], 0x30);
    }
    #[test]
    fn steel_preserves_left_click() {
        let mut b = standard();
        b[0] = Binding::Disabled {};
        assert!(rival310_buttons(&b).is_err());
        assert!(rival310_buttons(&b[..5]).is_err());
    }
    #[test]
    fn steel_rejects_timed_macros() {
        let mut b = standard();
        b[5] = Binding::Macro {
            events: vec![
                KeyEvent {
                    key: 4,
                    down: true,
                    delay_ms: 5,
                },
                KeyEvent {
                    key: 4,
                    down: false,
                    delay_ms: 5,
                },
            ],
        };
        assert!(rival310_buttons(&b)
            .unwrap_err()
            .contains("host macro engine"));
    }
    #[test]
    fn steel_rejects_shortcut_overflow() {
        let mut b = standard();
        b[5] = Binding::Key {
            key: 4,
            modifiers: vec![0xe0; 4],
        };
        assert!(rival310_buttons(&b).is_err());
        b[5] = Binding::Key {
            key: 4,
            modifiers: vec![0xe0, 0xe0],
        };
        assert!(rival310_buttons(&b).is_err());
    }
    #[test]
    fn macro_balanced() {
        let e = vec![
            KeyEvent {
                key: 4,
                down: true,
                delay_ms: 0,
            },
            KeyEvent {
                key: 4,
                down: false,
                delay_ms: 100,
            },
        ];
        assert!(validate_macro(&e).is_ok());
        assert!(validate_macro(&e[..1]).is_err());
        assert!(validate_macro(&e[1..]).is_err());
    }
    #[test]
    fn macro_duplicate_and_bounds() {
        let d = KeyEvent {
            key: 4,
            down: true,
            delay_ms: 0,
        };
        assert!(validate_macro(&[d.clone(), d]).is_err());
        assert!(validate_macro(&[]).is_err());
        assert!(validate_macro(&vec![
            KeyEvent {
                key: 4,
                down: true,
                delay_ms: 0
            };
            257
        ])
        .is_err());
    }
    #[test]
    fn binding_roundtrip_preserves_macro_events() {
        let b = Binding::Macro {
            events: vec![
                KeyEvent {
                    key: 4,
                    down: true,
                    delay_ms: 0,
                },
                KeyEvent {
                    key: 4,
                    down: false,
                    delay_ms: 100,
                },
            ],
        };
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(serde_json::from_str::<Binding>(&json).unwrap(), b);
        assert!(serde_json::from_str::<Binding>(r#"{"kind":"disabled","events":[]}"#).is_err());
    }
}

/// Transport-neutral write transaction for a *fully compiled* Rival 310 plan.
/// A failed write stops the transaction and never sends the persistence command.
/// No rollback guarantee is made: this protocol cannot read back all bindings.
pub trait Output {
    fn send(&mut self, report: &[u8]) -> Result<usize, String>;
}
pub fn rival310_compile(
    levels: &[u16],
    rate: u32,
    bindings: &[Binding],
) -> Result<Vec<Vec<u8>>, String> {
    if levels.len() != 2 {
        return Err("Rival 310 requires two DPI stages".into());
    }
    let mut out = vec![
        rival310_dpi(0, levels[0])?,
        rival310_dpi(1, levels[1])?,
        rival310_rate(rate)?,
        rival310_buttons(bindings)?,
    ];
    let mut save = vec![0; 64];
    save[1] = 0x59;
    out.push(save);
    Ok(out)
}
pub fn rival310_send(
    link: &mut impl Output,
    levels: &[u16],
    rate: u32,
    bindings: &[Binding],
) -> Result<(), String> {
    let packets = rival310_compile(levels, rate, bindings)?;
    for (index, packet) in packets.iter().enumerate() {
        let written = link.send(packet).map_err(|e| {
            format!("Write {index} failed; earlier volatile writes may remain: {e}")
        })?;
        if written != packet.len() {
            return Err(format!(
                "Short write at report {index}; earlier volatile writes may remain"
            ));
        }
    }
    Ok(())
}
/// DPI portion of protocol-v2 settings response (upstream excludes the zero
/// report ID). Other bytes include LED state; do not manufacture report rate,
/// button bindings or macros from this limited readback.
pub fn rival310_decode_dpi(bytes: &[u8]) -> Result<(usize, [u16; 2]), String> {
    if bytes.len() != 64 || bytes[1] < 1 || bytes[1] > 2 {
        return Err("Invalid Rival 310 settings response".into());
    }
    let values = [
        (u16::from(bytes[2]) + 1) * 100,
        (u16::from(bytes[4]) + 1) * 100,
    ];
    for value in values {
        dpi(value, 12000, 100)?;
    }
    Ok((usize::from(bytes[1] - 1), values))
}

#[cfg(test)]
mod transaction_tests {
    use super::*;
    #[derive(Default)]
    struct Bus {
        sent: Vec<Vec<u8>>,
        fail: Option<usize>,
        short: bool,
    }
    impl Output for Bus {
        fn send(&mut self, p: &[u8]) -> Result<usize, String> {
            if self.fail == Some(self.sent.len()) {
                return Err("simulated disconnect".into());
            }
            self.sent.push(p.to_vec());
            Ok(p.len() - usize::from(self.short))
        }
    }
    fn buttons() -> Vec<Binding> {
        vec![
            Binding::Mouse { button: 1 },
            Binding::Mouse { button: 2 },
            Binding::Mouse { button: 3 },
            Binding::Mouse { button: 4 },
            Binding::Mouse { button: 5 },
            Binding::DpiCycle {},
        ]
    }
    #[test]
    fn complete_plan_emits_persistence_last() {
        let mut bus = Bus::default();
        rival310_send(&mut bus, &[800, 1600], 1000, &buttons()).unwrap();
        assert_eq!(
            bus.sent.iter().map(|p| p[1]).collect::<Vec<_>>(),
            vec![0x53, 0x53, 0x54, 0x31, 0x59]
        );
    }
    #[test]
    fn invalid_last_binding_prevents_all_writes() {
        let mut b = buttons();
        b[5] = Binding::Key {
            key: 0,
            modifiers: vec![],
        };
        let mut bus = Bus::default();
        assert!(rival310_send(&mut bus, &[800, 1600], 1000, &b).is_err());
        assert!(bus.sent.is_empty());
    }
    #[test]
    fn macro_never_silently_disappears() {
        let mut b = buttons();
        b[5] = Binding::Macro { events: vec![] };
        let mut bus = Bus::default();
        assert!(rival310_send(&mut bus, &[800, 1600], 1000, &b).is_err());
        assert!(bus.sent.is_empty());
    }
    #[test]
    fn disconnect_at_each_report_never_reports_success() {
        for i in 0..5 {
            let mut bus = Bus {
                fail: Some(i),
                ..Default::default()
            };
            assert!(rival310_send(&mut bus, &[800, 1600], 1000, &buttons()).is_err());
            assert_eq!(bus.sent.len(), i);
            assert!(!bus.sent.iter().any(|p| p[1] == 0x59));
        }
    }
    #[test]
    fn short_write_stops_transaction() {
        let mut bus = Bus {
            short: true,
            ..Default::default()
        };
        assert!(rival310_send(&mut bus, &[800, 1600], 1000, &buttons()).is_err());
        assert_eq!(bus.sent.len(), 1);
    }
    #[test]
    fn settings_decode() {
        let mut p = vec![0; 64];
        p[1] = 2;
        p[2] = 7;
        p[4] = 15;
        assert_eq!(rival310_decode_dpi(&p).unwrap(), (1, [800, 1600]));
        p[1] = 0;
        assert!(rival310_decode_dpi(&p).is_err());
        p[1] = 1;
        p[4] = 255;
        assert!(rival310_decode_dpi(&p).is_err());
        assert!(rival310_decode_dpi(&p[..63]).is_err());
    }
}

/// Independent rivalcfg reference dialect. Deliberately separate from the
/// libratbag-based draft above: the upstreams disagree about DPI suffix offset
/// and 125/250 Hz codes. Do not choose a dialect by vendor alone.
pub fn rival310_rivalcfg_dpi(stage:u8,value:u16)->Result<Vec<u8>,String>{
 if stage>1{return Err("Rival 310 has two DPI stages".into());}dpi(value,12000,100)?;
 Ok(vec![0,0x53,0,stage+1,(value/100-1) as u8,0,0x42])
}
pub fn rival310_rivalcfg_rate(rate:u32)->Result<Vec<u8>,String>{
 let code=match rate{125=>4,250=>3,500=>2,1000=>1,_=>return Err("Unsupported report rate".into())};Ok(vec![0,0x54,0,code])
}
#[cfg(test)]mod independent_reference_tests{
 use super::*;
 #[test]fn generated_upstream_packets_match_dpi_and_rates(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../tests/fixtures/rival310-rivalcfg.json")).unwrap();
 let mut checked=0;for case in fixture["cases"].as_array().unwrap(){let method=case["method"].as_str().unwrap();let value=case["value"].as_u64();let result=match method{"set_sensitivity1"=>Some(rival310_rivalcfg_dpi(0,value.unwrap() as u16).unwrap()),"set_sensitivity2"=>Some(rival310_rivalcfg_dpi(1,value.unwrap() as u16).unwrap()),"set_polling_rate"=>Some(rival310_rivalcfg_rate(value.unwrap() as u32).unwrap()),_=>None};if let Some(result)=result{let expected:Vec<u8>=serde_json::from_value(case["report"].clone()).unwrap();assert_eq!(result,expected,"{}",case["name"]);checked+=1;}}assert_eq!(checked,12);
 }
 #[test]fn generated_upstream_button_bytes_match_our_layout(){
 let fixture:serde_json::Value=serde_json::from_str(include_str!("../tests/fixtures/rival310-rivalcfg.json")).unwrap();let mut checked=0;
 for case in fixture["cases"].as_array().unwrap(){if case["method"]!="set_buttons_mapping"{continue;}let mut bindings=vec![Binding::Mouse{button:1},Binding::Mouse{button:2},Binding::Mouse{button:3},Binding::Mouse{button:4},Binding::Mouse{button:5},Binding::DpiCycle{}];if case["name"]=="buttons-key-a"{bindings[5]=Binding::Key{key:4,modifiers:vec![]};}let actual=rival310_buttons(&bindings).unwrap();let expected:Vec<u8>=serde_json::from_value(case["report"].clone()).unwrap();assert_eq!(&actual[..expected.len()],expected.as_slice());assert!(actual[expected.len()..].iter().all(|b|*b==0));checked+=1;}assert_eq!(checked,2);
 }
 #[test]fn protocol_disagreement_is_explicit_and_not_silently_interchanged(){assert_ne!(&rival310_dpi(0,800).unwrap()[..8],&[0,0x53,0,1,7,0,0x42,0]);assert_ne!(rival310_rate(125).unwrap()[3],rival310_rivalcfg_rate(125).unwrap()[3]);assert_ne!(rival310_rate(250).unwrap()[3],rival310_rivalcfg_rate(250).unwrap()[3]);}
}

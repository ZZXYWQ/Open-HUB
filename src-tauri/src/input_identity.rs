//! Live source association only. Container ids never become profile owners,
//! persistent unit ids or permission to resume software macros after reconnect.
use crate::{host_macros::Source, raw_input::SourceInfo};
use gearwise_core::{model::Connection, Device};
#[derive(Clone, Debug)]
pub struct Target {
    pub id: String,
    pub vendor: u16,
    pub product: u16,
    pub serial: Option<String>,
    pub control_path: Option<String>,
    pub connection_key: Option<String>,
    pub wired: bool,
}
impl Target {
    pub fn of(device: &dyn Device) -> Self {
        let d = device.descriptor();
        Self {
            id: d.id.clone(),
            vendor: d.vendor_id,
            product: d.product_id,
            serial: d.serial.clone(),
            control_path: device.control_path().map(str::to_owned),
            connection_key: device.connection_key().map(str::to_owned),
            wired: d.connection == Connection::Wired,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proof {
    source: Source,
    target: String,
    control_path: String,
    connection_key: String,
    container: [u8; 16],
    // A Razer serial is required evidence, not merely a lookup hint. Retaining
    // it prevents a replacement source and target from both changing serials
    // while reproducing the old paths, connection key and container metadata.
    required_serial: Option<String>,
}
fn valid_serial(target: &Target, value: Option<&str>) -> bool {
    !crate::profile_routing::serial_key(target.vendor, target.product, value).is_empty()
}
fn reviewed_razer(target: &Target) -> bool {
    target.vendor == 0x1532 && matches!(target.product, 0x00b6 | 0x00aa | 0x00ab | 0x00b4)
}
fn nonempty_identity(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}
/// Reviewed Razer serial matches and the three reviewed wired SteelSeries
/// pairs retain a unique live physical control/keyboard association. Razer
/// never uses the SteelSeries serialless fallback or shared receiver B7.
pub fn verify(
    sources: Vec<SourceInfo>,
    source: &Source,
    target: &Target,
    peers: &[Target],
    mut lookup: impl FnMut(&str) -> Option<[u8; 16]>,
) -> Result<Option<Proof>, String> {
    if (source.vendor, source.product) != (target.vendor, target.product) {
        return Err("输入来源与当前目标鼠标型号不一致".into());
    }
    let razer = reviewed_razer(target);
    if target.vendor == 0x1532 && !razer {
        return Err("此 Razer 型号的独立物理输入关联尚未核实，未启用软件宏".into());
    }
    let steel = target.wired && crate::steelseries::supported(target.vendor, target.product);
    let serial_result = crate::macro_runtime::verify_input_source(
        sources.clone(),
        source,
        target.serial.as_deref(),
    );
    if serial_result.is_ok() {
        if peers
            .iter()
            .filter(|p| {
                (p.vendor, p.product) == (target.vendor, target.product)
                    && p.serial.as_deref().map(str::trim) == target.serial.as_deref().map(str::trim)
            })
            .count()
            != 1
        {
            return Err("控制接口序列号对应多只鼠标，无法确认独立目标".into());
        }
        if !razer && !steel {
            return Ok(None);
        }
    }
    if serial_result.is_err() && !steel {
        return Err(serial_result.unwrap_err());
    }
    if razer && sources.iter().filter(|s| s.source == *source).count() != 1 {
        return Err("所选鼠标输入来源不唯一，请重新读取".into());
    }
    let source_item = sources
        .iter()
        .find(|s| s.source == *source)
        .ok_or("未找到所选鼠标的输入来源")?;
    if source_item.kind != "Keyboard report" {
        return Err("此鼠标需要自身的键盘输入接口".into());
    }
    let required_serial = if razer {
        let serial = target.serial.as_deref().map(str::trim)
            .filter(|serial| valid_serial(target, Some(serial)))
            .ok_or("此 Razer 鼠标需要有效的控制接口序列号")?;
        if !valid_serial(target, source_item.serial.as_deref())
            || source_item.serial.as_deref().map(str::trim) != Some(serial) {
            return Err("Razer 输入接口与控制鼠标序列号不一致或不可读".into());
        }
        if !nonempty_identity(&source.path) {
            return Err("当前鼠标键盘输入路径不可核对".into());
        }
        Some(serial.to_owned())
    } else { None };
    if valid_serial(target, target.serial.as_deref())
        && valid_serial(target, source_item.serial.as_deref())
        && target.serial.as_deref().map(str::trim) != source_item.serial.as_deref().map(str::trim)
    {
        return Err("已知输入序列号与控制鼠标冲突，未使用物理关联覆盖".into());
    }
    let path = target
        .control_path
        .as_deref()
        .filter(|p| nonempty_identity(p))
        .ok_or("此驱动尚未提供可核对的控制接口")?;
    let key = target
        .connection_key
        .as_deref()
        .filter(|p| nonempty_identity(p))
        .ok_or("当前控制连接身份不可核对")?;
    let container = lookup(path)
        .filter(|id| *id != [0; 16] && *id != [0xff; 16])
        .ok_or("无法读取鼠标控制接口的物理关联")?;
    if lookup(&source.path) != Some(container) {
        return Err("输入接口不属于这只鼠标，未启用软件宏".into());
    }
    let source_count = sources
        .iter()
        .filter(|s| {
            (razer || s.source.vendor == source.vendor && s.source.product == source.product)
                && s.kind == "Keyboard report"
                && lookup(&s.source.path) == Some(container)
        })
        .count();
    if source_count != 1 {
        return Err("同一物理鼠标有多个候选键盘接口，不能确定独立输入来源".into());
    }
    let peer_count = peers
        .iter()
        .filter(|p| {
            p.control_path
                .as_deref()
                .is_some_and(|p| lookup(p) == Some(container))
        })
        .count();
    if peer_count != 1
        || !peers.iter().any(|p| {
            p.id == target.id
                && p.connection_key == target.connection_key
                && p.control_path == target.control_path
                && (!razer || (p.vendor, p.product) == (target.vendor, target.product)
                    && p.serial.as_deref().map(str::trim) == required_serial.as_deref())
        })
    {
        return Err("鼠标控制接口存在别名或已重新打开，请重新选择".into());
    }
    Ok(Some(Proof {
        source: source.clone(),
        target: target.id.clone(),
        control_path: path.into(),
        connection_key: key.into(),
        container,
        required_serial,
    }))
}
pub fn recheck(
    proof: &Proof,
    sources: Vec<SourceInfo>,
    target: &Target,
    peers: &[Target],
    lookup: impl FnMut(&str) -> Option<[u8; 16]>,
) -> Result<(), String> {
    if target.id != proof.target
        || target.connection_key.as_deref() != Some(&proof.connection_key)
        || target.control_path.as_deref() != Some(&proof.control_path)
        || proof.required_serial.as_deref().is_some_and(|serial| {
            target.serial.as_deref().map(str::trim) != Some(serial)
        })
    {
        return Err("鼠标控制连接已变化，未沿用旧输入关联".into());
    }
    if verify(sources, &proof.source, target, peers, lookup)?.as_ref() != Some(proof) {
        return Err("鼠标输入关联已变化，请重新读取".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    fn target(unit: &str) -> Target {
        Target {
            id: format!("steel:{unit}"),
            vendor: 0x1038,
            product: 0x1722,
            serial: None,
            control_path: Some(format!("control:{unit}")),
            connection_key: Some(format!("open:{unit}")),
            wired: true,
        }
    }
    fn source(unit: &str) -> SourceInfo {
        SourceInfo {
            source: Source {
                vendor: 0x1038,
                product: 0x1722,
                path: format!("keyboard:{unit}"),
            },
            kind: "Keyboard report".into(),
            serial: None,
        }
    }
    fn metadata() -> HashMap<String, [u8; 16]> {
        [
            ("control:A".into(), [1; 16]),
            ("keyboard:A".into(), [1; 16]),
            ("control:B".into(), [2; 16]),
            ("keyboard:B".into(), [2; 16]),
        ]
        .into()
    }
    #[test]
    fn two_identical_serialless_wired_mice_resolve_only_their_own_keyboard_collection() {
        let a = target("A");
        let b = target("B");
        let sources = vec![source("A"), source("B")];
        let data = metadata();
        for (t, s) in [(&a, &sources[0].source), (&b, &sources[1].source)] {
            assert!(
                verify(sources.clone(), s, t, &[a.clone(), b.clone()], |p| data
                    .get(p)
                    .copied())
                .unwrap()
                .is_some()
            );
        }
        assert!(verify(
            sources.clone(),
            &sources[1].source,
            &a,
            &[a.clone(), b],
            |p| data.get(p).copied()
        )
        .is_err());
    }
    #[test]
    fn container_association_requires_unique_control_and_keyboard_no_path_prefix_guesses() {
        let a = target("A");
        let mut data = metadata();
        let source_a = source("A");
        let mut duplicate = source_a.clone();
        duplicate.source.path = "keyboard:duplicate".into();
        data.insert(duplicate.source.path.clone(), [1; 16]);
        assert!(verify(
            vec![source_a.clone(), duplicate],
            &source_a.source,
            &a,
            &[a.clone()],
            |p| data.get(p).copied()
        )
        .is_err());
        let mut alias = a.clone();
        alias.id = "alias".into();
        assert!(verify(
            vec![source_a.clone()],
            &source_a.source,
            &a,
            &[a.clone(), alias],
            |p| data.get(p).copied()
        )
        .is_err());
        for container in [None, Some([0; 16]), Some([9; 16])] {
            assert!(verify(
                vec![source_a.clone()],
                &source_a.source,
                &a,
                &[a.clone()],
                |p| if p.starts_with("control:") {
                    container
                } else {
                    Some([1; 16])
                }
            )
            .is_err());
        }
    }
    #[test]
    fn only_reviewed_wired_models_can_fall_back_and_real_serial_conflicts_are_never_overridden() {
        for (v, p, wired) in [
            (0x1532, 0x00aa, true),
            (0x1b1c, 0x1b2e, true),
            (0x1038, 0x1722, false),
            (0x1038, 0x172e, true),
        ] {
            let mut t = target("A");
            t.vendor = v;
            t.product = p;
            t.wired = wired;
            let mut s = source("A");
            s.source.vendor = v;
            s.source.product = p;
            assert!(
                verify(vec![s.clone()], &s.source, &t, &[t.clone()], |_| Some(
                    [1; 16]
                ))
                .is_err()
            );
        }
        let mut t = target("A");
        t.serial = Some("unit-A".into());
        let mut s = source("A");
        s.serial = Some("unit-B".into());
        assert!(
            verify(vec![s.clone()], &s.source, &t, &[t.clone()], |_| Some(
                [1; 16]
            ))
            .is_err()
        );
    }
    #[test]
    fn reopened_same_path_same_container_is_rejected_by_ephemeral_open_identity() {
        let t = target("A");
        let s = source("A");
        let data = metadata();
        let proof = verify(vec![s.clone()], &s.source, &t, &[t.clone()], |p| {
            data.get(p).copied()
        })
        .unwrap()
        .unwrap();
        let mut reopened = t.clone();
        reopened.connection_key = Some("open:reopened".into());
        assert!(recheck(
            &proof,
            vec![s.clone()],
            &reopened,
            &[reopened.clone()],
            |p| data.get(p).copied()
        )
        .is_err());
        assert!(recheck(&proof, vec![s.clone()], &t, &[t.clone()], |p| data
            .get(p)
            .copied())
        .is_ok());
        assert!(recheck(&proof, vec![], &t, &[t.clone()], |p| data.get(p).copied()).is_err());
    }
    #[test]
    fn wired_serial_success_still_requires_current_physical_pair_and_duplicate_serial_controls_are_rejected(
    ) {
        let mut a = target("A");
        a.serial = Some("valid-unit".into());
        let mut sa = source("A");
        sa.serial = Some("valid-unit".into());
        let data = metadata();
        assert!(
            verify(vec![sa.clone()], &sa.source, &a, &[a.clone()], |p| data
                .get(p)
                .copied())
            .unwrap()
            .is_some()
        );
        assert!(verify(vec![sa.clone()], &sa.source, &a, &[a.clone()], |_| None).is_err());
        let mut b = target("B");
        b.serial = a.serial.clone();
        assert!(
            verify(vec![sa.clone()], &sa.source, &a, &[a.clone(), b], |p| data
                .get(p)
                .copied())
            .is_err()
        );
    }
    #[test]
    fn native_steel_control_path_is_exact_and_reopening_never_reuses_instance_generation() {
        let (a, _) = gearwise_steelseries::simulated_model(0x1722).unwrap();
        let (b, _) = gearwise_steelseries::simulated_model(0x1722).unwrap();
        assert_eq!(a.control_path(), b.control_path());
        assert_ne!(a.connection_key(), b.connection_key());
        assert_eq!(a.descriptor().id, b.descriptor().id);
    }
}

#[cfg(test)]
mod razer_identity_tests {
    use super::*;
    use std::collections::HashMap;

    const PRODUCTS: [u16; 4] = [0x00b6, 0x00aa, 0x00ab, 0x00b4];
    fn pair(product: u16, unit: &str) -> (Target, SourceInfo) {
        let serial = format!("RAZER-{unit}");
        (Target {
            id: format!("razer:{product:04x}:{unit}"),
            vendor: 0x1532,
            product,
            serial: Some(serial.clone()),
            control_path: Some(format!("control:{unit}")),
            connection_key: Some(format!("open:{unit}")),
            wired: product != 0x00ab,
        }, SourceInfo {
            source: Source { vendor: 0x1532, product, path: format!("keyboard:{unit}") },
            kind: "Keyboard report".into(),
            serial: Some(serial),
        })
    }
    fn metadata() -> HashMap<String, [u8; 16]> {
        [("control:A".into(), [1;16]), ("keyboard:A".into(), [1;16]),
            ("control:B".into(), [2;16]), ("keyboard:B".into(), [2;16])].into()
    }
    fn verify_pair(target: &Target, item: &SourceInfo) -> Result<Option<Proof>, String> {
        let data = metadata();
        verify(vec![item.clone()], &item.source, target, &[target.clone()], |p| data.get(p).copied())
    }

    #[test]
    fn reviewed_razer_matches_retain_serial_and_unique_physical_proof_for_each_unit() {
        for product in PRODUCTS {
            let (a, sa) = pair(product, "A");
            let (b, sb) = pair(product, "B");
            let data = metadata();
            for (target, item) in [(&a, &sa), (&b, &sb)] {
                let proof = verify(vec![sa.clone(), sb.clone()], &item.source, target,
                    &[a.clone(), b.clone()], |p| data.get(p).copied()).unwrap().unwrap();
                assert_eq!(proof.required_serial.as_deref(), target.serial.as_deref());
                recheck(&proof, vec![sa.clone(), sb.clone()], target,
                    &[a.clone(), b.clone()], |p| data.get(p).copied()).unwrap();
            }
            assert!(verify(vec![sa.clone(), sb.clone()], &sb.source, &a,
                &[a.clone(), b], |p| data.get(p).copied()).is_err());
            let mut padded = sa.clone();
            padded.serial = Some("  RAZER-A  ".into());
            assert!(verify_pair(&a, &padded).unwrap().is_some());
        }
    }

    #[test]
    fn razer_never_uses_container_metadata_to_replace_missing_invalid_or_conflicting_serials() {
        for product in PRODUCTS {
            let (a, sa) = pair(product, "A");
            for serial in [None, Some(""), Some("  "), Some("000000"), Some("FFFF"), Some("unknown"), Some("x\ny")] {
                let mut target = a.clone();
                let mut item = sa.clone();
                target.serial = serial.map(str::to_owned);
                item.serial = serial.map(str::to_owned);
                assert!(verify_pair(&target, &item).is_err());
            }
            let mut missing = sa.clone();
            missing.serial = None;
            assert!(verify_pair(&a, &missing).is_err());
            let mut conflict = sa.clone();
            conflict.serial = Some("RAZER-B".into());
            assert!(verify_pair(&a, &conflict).is_err());
            let mut missing_target = a.clone();
            missing_target.serial = None;
            assert!(verify_pair(&missing_target, &sa).is_err());
            let mut long_target = a.clone();
            let mut long_source = sa.clone();
            long_target.serial = Some("x".repeat(257));
            long_source.serial = long_target.serial.clone();
            assert!(verify_pair(&long_target, &long_source).is_err());
        }
    }

    #[test]
    fn razer_proof_requires_nonempty_native_identity_keyboard_and_valid_same_container() {
        for product in PRODUCTS {
            let (a, sa) = pair(product, "A");
            for value in [None, Some(""), Some("  "), Some("open\0bad")] {
                let mut target = a.clone();
                target.control_path = value.map(str::to_owned);
                assert!(verify_pair(&target, &sa).is_err());
                target = a.clone();
                target.connection_key = value.map(str::to_owned);
                assert!(verify_pair(&target, &sa).is_err());
            }
            for path in ["", "  ", "keyboard\0bad"] {
                let mut item = sa.clone();
                item.source.path = path.into();
                assert!(verify(vec![item.clone()], &item.source, &a, &[a.clone()], |_| Some([1;16])).is_err());
            }
            for kind in ["Mouse report", "Vendor HID", ""] {
                let mut item = sa.clone();
                item.kind = kind.into();
                assert!(verify_pair(&a, &item).is_err());
            }
            for control in [None, Some([0;16]), Some([0xff;16]), Some([2;16])] {
                assert!(verify(vec![sa.clone()], &sa.source, &a, &[a.clone()], |p| {
                    if p == "control:A" { control } else { Some([1;16]) }
                }).is_err());
            }
            assert!(verify(vec![sa.clone()], &sa.source, &a, &[a.clone()], |p| {
                if p == "keyboard:A" { None } else { Some([1;16]) }
            }).is_err());
        }
    }

    #[test]
    fn razer_refuses_duplicate_sources_serial_peers_container_aliases_and_stale_target_snapshots() {
        for product in PRODUCTS {
            let (a, sa) = pair(product, "A");
            let data = metadata();
            let mut duplicate = sa.clone();
            duplicate.kind = "Mouse report".into();
            assert!(verify(vec![sa.clone(), duplicate], &sa.source, &a, &[a.clone()],
                |p| data.get(p).copied()).is_err());
            let mut second_keyboard = sa.clone();
            second_keyboard.source.path = "keyboard:alias".into();
            second_keyboard.serial = Some("RAZER-OTHER".into());
            assert!(verify(vec![sa.clone(), second_keyboard], &sa.source, &a, &[a.clone()],
                |_| Some([1;16])).is_err());
            let mut other_model_keyboard = sa.clone();
            other_model_keyboard.source.product = 0x0084;
            other_model_keyboard.source.path = "keyboard:other-model".into();
            other_model_keyboard.serial = Some("RAZER-OTHER".into());
            assert!(verify(vec![sa.clone(), other_model_keyboard], &sa.source, &a, &[a.clone()],
                |_| Some([1;16])).is_err());
            let (mut serial_peer, _) = pair(product, "B");
            serial_peer.serial = a.serial.clone();
            assert!(verify(vec![sa.clone()], &sa.source, &a, &[a.clone(), serial_peer.clone()],
                |p| data.get(p).copied()).is_err());
            let mut alias = a.clone();
            alias.id = "control-alias".into();
            alias.vendor = 0x1038;
            alias.product = 0x1722;
            assert!(verify(vec![sa.clone()], &sa.source, &a, &[a.clone(), alias.clone()],
                |p| data.get(p).copied()).is_err());
            // A same-serial peer on unit B cannot validate a stale target whose
            // path/key are present only through a peer of another model.
            alias.id = a.id.clone();
            assert!(verify(vec![sa.clone()], &sa.source, &a, &[alias, serial_peer],
                |p| data.get(p).copied()).is_err());
            for field in ["id", "serial", "path", "key"] {
                let mut peer = a.clone();
                match field {
                    "id" => peer.id = "another-id".into(),
                    "serial" => peer.serial = Some("RAZER-B".into()),
                    "path" => peer.control_path = Some("control:B".into()),
                    _ => peer.connection_key = Some("open:reopened".into()),
                }
                assert!(verify(vec![sa.clone()], &sa.source, &a, &[peer],
                    |p| data.get(p).copied()).is_err());
            }
        }
    }

    #[test]
    fn retained_razer_proof_rejects_both_ends_swapping_serial_without_a_path_or_container_change() {
        for product in PRODUCTS {
            let (a, sa) = pair(product, "A");
            let proof = verify_pair(&a, &sa).unwrap().unwrap();
            let mut replacement = a.clone();
            let mut replacement_source = sa.clone();
            replacement.serial = Some("REPLACEMENT-UNIT".into());
            replacement_source.serial = replacement.serial.clone();
            let new_proof = verify_pair(&replacement, &replacement_source).unwrap().unwrap();
            assert_ne!(proof, new_proof);
            let data = metadata();
            assert!(recheck(&proof, vec![replacement_source], &replacement,
                &[replacement.clone()], |p| data.get(p).copied()).is_err());
        }
    }

    #[test]
    fn retained_razer_proof_never_survives_reopen_source_removal_or_container_reassociation() {
        for product in PRODUCTS {
            let (a, sa) = pair(product, "A");
            let proof = verify_pair(&a, &sa).unwrap().unwrap();
            let data = metadata();
            let mut reopened = a.clone();
            reopened.connection_key = Some("open:new-generation".into());
            assert!(recheck(&proof, vec![sa.clone()], &reopened, &[reopened.clone()],
                |p| data.get(p).copied()).is_err());
            assert!(recheck(&proof, vec![], &a, &[a.clone()], |p| data.get(p).copied()).is_err());
            assert!(recheck(&proof, vec![sa.clone()], &a, &[], |p| data.get(p).copied()).is_err());
            assert!(recheck(&proof, vec![sa.clone()], &a, &[a.clone()], |_| Some([9;16])).is_err());
            let mut source = sa.clone();
            source.source.path = "keyboard:reconnected".into();
            assert!(recheck(&proof, vec![source], &a, &[a.clone()], |_| Some([1;16])).is_err());
        }
    }

    #[test]
    fn b7_and_unreviewed_razer_are_blocked_before_physical_lookup_other_brands_keep_serial_rules() {
        for product in [0x00b7, 0x0084, 0x00c0, 0x00c1, 0x00c2, 0xffff] {
            let (a, sa) = pair(product, "A");
            assert!(verify(vec![sa.clone()], &sa.source, &a, &[a.clone()],
                |_| panic!("unreviewed Razer must not query physical metadata")).is_err());
        }
        let (mut target, mut item) = pair(0x00b6, "A");
        target.vendor = 0x3554;
        target.product = 0xf507;
        item.source.vendor = target.vendor;
        item.source.product = target.product;
        assert!(verify(vec![item.clone()], &item.source, &target, &[target.clone()],
            |_| panic!("other brands retain the existing serial-only path")).unwrap().is_none());
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
pub struct Fixture {
    pub sources: Vec<SourceInfo>,
    pub containers: std::collections::HashMap<String, [u8; 16]>,
}

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
}
fn valid_serial(target: &Target, value: Option<&str>) -> bool {
    !crate::profile_routing::serial_key(target.vendor, target.product, value).is_empty()
}
/// Keep serial checks for other models; the three reviewed wired SteelSeries
/// pairs also require a unique physical control/keyboard association.
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
        if !target.wired || !crate::steelseries::supported(target.vendor, target.product) {
            return Ok(None);
        }
    }
    if !target.wired || !crate::steelseries::supported(target.vendor, target.product) {
        return Err(serial_result.unwrap_err());
    }
    let source_item = sources
        .iter()
        .find(|s| s.source == *source)
        .ok_or("未找到所选鼠标的输入来源")?;
    if source_item.kind != "Keyboard report" {
        return Err("此有线鼠标需要自身的键盘输入接口".into());
    }
    if valid_serial(target, target.serial.as_deref())
        && valid_serial(target, source_item.serial.as_deref())
        && target.serial.as_deref().map(str::trim) != source_item.serial.as_deref().map(str::trim)
    {
        return Err("已知输入序列号与控制鼠标冲突，未使用物理关联覆盖".into());
    }
    let path = target
        .control_path
        .as_deref()
        .filter(|p| !p.is_empty())
        .ok_or("此驱动尚未提供可核对的控制接口")?;
    let key = target
        .connection_key
        .as_deref()
        .filter(|p| !p.is_empty())
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
            s.source.vendor == source.vendor
                && s.source.product == source.product
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
#[derive(Clone, Default)]
pub struct Fixture {
    pub sources: Vec<SourceInfo>,
    pub containers: std::collections::HashMap<String, [u8; 16]>,
}

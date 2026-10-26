//! Per-physical-device profile ownership. Imported profiles are templates, never
//! global targets. This module contains no HID/OS input calls.
use crate::profiles::{new_id, Profile, ProfileStore};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeviceProfiles {
    pub active_id: Option<String>,
    pub default_id: Option<String>,
    /// Template id -> private copy. Re-import never silently edits private copies.
    #[serde(default)]
    pub copies: BTreeMap<String, String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Native,
    Multi,
    Sino,
}
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub id: String,
    pub name: String,
    pub key: String,
    /// Snapshot from the manager before duplicate-id quarantine. Recheck under the I/O lock.
    pub observed_key: String,
    /// MCHOSE transport generation captured by UI/tray; not persistent ownership.
    pub connection_key: Option<String>,
    pub vendor: u16,
    pub models: Vec<u16>,
    pub firmware: String,
    pub route: Route,
}
impl Endpoint {
    pub fn matches(&self, p: &Profile) -> bool {
        if !p.targets(self.vendor, &self.models) {
            return false;
        }
        match self.route {
            Route::Native => p.multi.is_none() && p.sino.is_none() && p.mchose.as_ref().is_none_or(|m|m.vendor==self.vendor&&self.models.contains(&m.model)),
            Route::Multi => {
                p.sino.is_none() && p.mchose.is_none()
                    && p.multi.as_ref().is_some_and(|m| {
                        m.vendor == self.vendor && self.models.contains(&m.product)
                    })
            }
            Route::Sino => {
                p.multi.is_none() && p.mchose.is_none()
                    && match &p.sino {
                        Some(s) => {
                            s.model.vendor == self.vendor
                                && self.models.contains(&s.model.product)
                                && s.model.firmware == self.firmware
                        }
                        None => p.ghub_id.is_none() && p.mchose_board.is_none(),
                    }
            }
        }
    }
    /// Port/path and receiver identity do not prove which mouse is paired.
    pub fn can_auto_apply(&self) -> bool {
        self.key.starts_with("unit:") || self.key.starts_with("serial:")
    }
}
#[derive(Default)]
pub struct Sessions {
    keys: HashMap<String, String>,
    collisions: std::collections::HashSet<String>,
}
impl Sessions {
    pub fn quarantine(&mut self, keys: impl IntoIterator<Item = String>) {
        self.collisions.extend(keys);
    }
    pub fn collisions(&self) -> &std::collections::HashSet<String> {
        &self.collisions
    }
    /// Resolve duplicate/factory serials to separate connection-only identities.
    /// Missing devices lose session keys; reconnect cannot inherit a prior unit.
    pub fn resolve(&mut self, endpoints: &mut [Endpoint]) {
        self.keys
            .retain(|id, _| endpoints.iter().any(|e| &e.id == id));
        let mut counts = HashMap::<String, usize>::new();
        for e in endpoints.iter().filter(|e| e.can_auto_apply()) {
            *counts.entry(e.key.clone()).or_default() += 1;
        }
        self.collisions.extend(
            counts
                .iter()
                .filter(|(_, n)| **n != 1)
                .map(|(key, _)| key.clone()),
        );
        for e in endpoints {
            if e.key.is_empty() || self.collisions.contains(&e.key) {
                e.key = self
                    .keys
                    .entry(e.id.clone())
                    .or_insert_with(|| format!("session:{}", new_id()))
                    .clone();
            }
        }
    }
}
/// HID++ 0x0003 bytes 1..4 are the unit id. Require an observed long payload,
/// not an old compatibility-layer zero-padded short report.
pub fn logitech_unit(reply: &[u8]) -> Option<String> {
    if reply.len() < 13 {
        return None;
    }
    let id = &reply[1..5];
    if id.iter().all(|x| *x == 0) || id.iter().all(|x| *x == 0xff) {
        return None;
    }
    Some(format!(
        "unit:046d:{:02x}{:02x}{:02x}{:02x}",
        id[0], id[1], id[2], id[3]
    ))
}
pub fn serial_key(vendor: u16, product: u16, serial: Option<&str>) -> String {
    let Some(s) = serial
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
    else {
        return String::new();
    };
    let normalized = s.to_ascii_lowercase();
    if matches!(normalized.as_str(), "none" | "unknown" | "default" | "n/a")
        || s.chars()
            .all(|c| c == '0' || c == 'f' || c == 'F' || c == '-' || c == ' ')
    {
        return String::new();
    }
    // Keep different model protocols distinct even if a vendor reuses a serial.
    format!("serial:{vendor:04x}:{product:04x}:{s}")
}
pub fn choose<'a>(
    store: &ProfileStore,
    endpoints: &'a [Endpoint],
    profile: &Profile,
    selected: Option<&str>,
) -> Result<&'a Endpoint, String> {
    choose_with_copy(store, endpoints, profile, selected, false)
}
fn choose_with_copy<'a>(
    store: &ProfileStore,
    endpoints: &'a [Endpoint],
    profile: &Profile,
    selected: Option<&str>,
    as_copy: bool,
) -> Result<&'a Endpoint, String> {
    let candidates = endpoints
        .iter()
        .filter(|e| {
            selected.is_none_or(|id| e.id == id)
                && e.matches(profile)
                && (as_copy
                    || store
                        .profile_owners
                        .get(&profile.id)
                        .is_none_or(|key| key == &e.key))
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [one] => Ok(one),
        [] => Err("未连接这份配置所属的鼠标，请重新选择目标".into()),
        _ => Err("连接了多只匹配鼠标，请选择其中一只；本次未应用任何配置".into()),
    }
}
impl ProfileStore {
    pub fn repair_device_profiles(&mut self) {
        self.profile_owners
            .retain(|id, _| self.profiles.iter().any(|p| &p.id == id));
        for (key, d) in &mut self.device_profiles {
            let valid = |id: &String| self.profile_owners.get(id) == Some(key);
            d.copies.retain(|_, id| valid(id));
            if !d.default_id.as_ref().is_some_and(valid) {
                d.default_id = None;
            }
            if !d.active_id.as_ref().is_some_and(valid) {
                d.active_id = d.default_id.clone();
            }
        }
    }
    pub fn active_for(&self, e: &Endpoint) -> Option<&Profile> {
        let id = self.device_profiles.get(&e.key)?.active_id.as_deref()?;
        self.get(id)
            .filter(|p| self.profile_owners.get(id) == Some(&e.key) && e.matches(p))
    }
    /// Creates a private copy once. It has no G HUB import id, so re-imports
    /// cannot replace it through the source application's id.
    pub fn private_copy(&mut self, e: &Endpoint, source: &Profile) -> Result<Profile, String> {
        self.private_copy_as(e, source, false)
    }
    fn private_copy_as(
        &mut self,
        e: &Endpoint,
        source: &Profile,
        as_copy: bool,
    ) -> Result<Profile, String> {
        if !e.matches(source) {
            return Err("配置与目标鼠标不匹配".into());
        }
        if let Some(owner) = self.profile_owners.get(&source.id) {
            if owner == &e.key {
                return Ok(source.clone());
            }
            if !as_copy {
                return Err("此配置属于另一只鼠标".into());
            }
        }
        if !as_copy {
            if let Some(id) = self
                .device_profiles
                .get(&e.key)
                .and_then(|d| d.copies.get(&source.id))
            {
                if self.profile_owners.get(id) == Some(&e.key) {
                    if let Some(p) = self.get(id) {
                        return Ok(p.clone());
                    }
                }
            }
        }
        let mut copy = source.clone();
        copy.id = new_id();
        copy.ghub_id = None;
        copy.target_vendor = Some(e.vendor);
        if copy.target_model.is_none() {
            copy.target_model = e.models.first().copied();
        }
        self.upsert(copy.clone());
        self.profile_owners.insert(copy.id.clone(), e.key.clone());
        if !source.id.is_empty() {
            self.device_profiles
                .entry(e.key.clone())
                .or_default()
                .copies
                .insert(source.id.clone(), copy.id.clone());
        }
        Ok(copy)
    }
    pub fn select_for(&mut self, e: &Endpoint, id: &str) -> Result<(), String> {
        let p = self
            .get(id)
            .filter(|p| e.matches(p) && self.profile_owners.get(id) == Some(&e.key))
            .ok_or("配置不属于所选鼠标")?;
        let is_default = p.app_path.as_deref().is_none_or(|s| s.trim().is_empty());
        let d = self.device_profiles.entry(e.key.clone()).or_default();
        d.active_id = Some(id.into());
        if is_default {
            d.default_id = Some(id.into());
        }
        self.active_id = id.into();
        Ok(())
    }
    /// Only profiles explicitly assigned to this unit participate in auto-switch.
    /// A missing default means keep the current device state, never global Desktop.
    pub fn match_for(&self, e: &Endpoint, exe: &str) -> Option<&Profile> {
        let exe = normalize(exe);
        self.profiles
            .iter()
            .filter(|p| self.profile_owners.get(&p.id) == Some(&e.key) && e.matches(p))
            .filter_map(|p| {
                let path = normalize(p.app_path.as_deref()?);
                (!path.is_empty() && (exe == path || exe.starts_with(&format!("{path}\\"))))
                    .then_some((path.len(), p))
            })
            .max_by_key(|(n, _)| *n)
            .map(|(_, p)| p)
            .or_else(|| {
                self.device_profiles
                    .get(&e.key)?
                    .default_id
                    .as_deref()
                    .and_then(|id| self.get(id))
                    .filter(|p| e.matches(p) && self.profile_owners.get(&p.id) == Some(&e.key))
            })
    }
    pub fn save_for(&mut self, e: &Endpoint, edited: &Profile) -> Result<Profile, String> {
        let copy = self.save_private(e, edited)?;
        self.select_for(e, &copy.id)?;
        Ok(copy)
    }
    pub fn save_private(&mut self, e: &Endpoint, edited: &Profile) -> Result<Profile, String> {
        let mut copy = self.private_copy(e, edited)?;
        let id = copy.id.clone();
        copy = edited.clone();
        copy.id = id;
        copy.ghub_id = None;
        copy.target_vendor = Some(e.vendor);
        if copy.target_model.is_none() {
            copy.target_model = e.models.first().copied();
        }
        self.upsert(copy.clone());
        Ok(copy)
    }
}
fn normalize(path: &str) -> String {
    path.trim()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Prepared state is committed only after the single target I/O succeeds.
pub struct Prepared {
    pub target: Endpoint,
    pub profile: Profile,
    pub updated: ProfileStore,
}
pub fn prepare(
    store: &ProfileStore,
    endpoints: &[Endpoint],
    id: &str,
    selected: Option<&str>,
    as_copy: bool,
    manual: bool,
) -> Result<Prepared, String> {
    let source = store.get(id).ok_or("profile not found")?;
    if source.sino.is_some() || source.multi.is_some() || source.mchose.is_some() {
        crate::profile_transfer::validate(source)?;
    }
    if as_copy && (!manual || selected.is_none()) {
        return Err("复制配置必须明确选择目标鼠标".into());
    }
    let target = choose_with_copy(store, endpoints, source, selected, as_copy)?.clone();
    if !manual && !target.can_auto_apply() {
        return Err("此鼠标暂无已验证的唯一身份，请在本次连接中手动应用配置".into());
    }
    let mut updated = store.clone();
    let profile = updated.private_copy_as(&target, source, as_copy)?;
    Ok(Prepared {
        target,
        profile,
        updated,
    })
}

#[cfg(test)]
#[path = "profile_routing_tests.rs"]
mod tests;

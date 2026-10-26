//! Ephemeral Raw Input connection leases. A path reused after removal is a new
//! connection, even when its vendor, product and serial strings are unchanged.
use crate::host_macros::Source;
use std::{collections::HashMap, sync::{Arc, Weak, atomic::{AtomicBool, Ordering}}};

#[derive(Clone, Debug)]
pub(super) struct Lease { source: Source, live: Arc<AtomicBool> }
impl Lease {
    pub(super) fn source(&self) -> &Source { &self.source }
    pub(super) fn is_live(&self) -> bool { self.live.load(Ordering::Acquire) }
}
struct Slot { source: Source, token: Weak<AtomicBool> }
pub(super) struct Registry { live: bool, slots: HashMap<String, Slot> }
impl Default for Registry {
    fn default() -> Self { Self { live: true, slots: HashMap::new() } }
}
impl Registry {
    pub(super) fn capture(&mut self, source: &Source) -> Result<Lease, String> {
        if !self.live { return Err("输入监听连接已停止，请重新准备方案".into()); }
        if source.path.is_empty() || source.path.len()>4096 || source.path.contains('\0') {
            return Err("输入来源路径无效".into());
        }
        self.slots.retain(|_, slot| slot.token.strong_count()>0);
        if let Some(slot)=self.slots.get(&source.path) {
            if slot.source!=*source && slot.token.upgrade().is_some_and(|v|v.load(Ordering::Acquire)) {
                return Err("输入路径仍属于另一来源，未改变其正在运行的方案".into());
            }
            if slot.source==*source {
                if let Some(live)=slot.token.upgrade().filter(|v|v.load(Ordering::Acquire)) {
                    return Ok(Lease { source: source.clone(), live });
                }
            }
        }
        self.invalidate_path(&source.path);
        if self.slots.len()>=1024 { return Err("准备中的输入来源过多，请停止旧方案后重试".into()); }
        let live=Arc::new(AtomicBool::new(true));
        self.slots.insert(source.path.clone(),Slot { source: source.clone(), token: Arc::downgrade(&live) });
        Ok(Lease { source: source.clone(), live })
    }
    pub(super) fn check(&self, lease: &Lease) -> Result<(), String> {
        let registered=self.slots.get(&lease.source.path).is_some_and(|slot|
            slot.source==lease.source && slot.token.upgrade().is_some_and(|token|Arc::ptr_eq(&token,&lease.live)));
        if self.live && lease.is_live() && registered { Ok(()) }
        else { Err("输入连接已变化，旧方案未继续运行；请重新读取并选择此鼠标".into()) }
    }
    pub(super) fn invalidate_path(&mut self, path: &str) {
        if let Some(slot)=self.slots.remove(path) {
            if let Some(token)=slot.token.upgrade() { token.store(false,Ordering::Release); }
        }
    }
    pub(super) fn invalidate_all(&mut self) {
        for (_,slot) in self.slots.drain() {
            if let Some(token)=slot.token.upgrade() { token.store(false,Ordering::Release); }
        }
    }
    pub(super) fn close(&mut self) { self.live=false; self.invalidate_all(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(unit:&str)->Source { Source { vendor:0x1532, product:0x00b6, path:format!("simulation:keyboard:{unit}") } }
    #[test]
    fn removal_and_identical_path_reuse_never_revive_old_connection() {
        let mut registry=Registry::default();let source=source("A");
        let first=registry.capture(&source).unwrap();let same=registry.capture(&source).unwrap();
        assert!(Arc::ptr_eq(&first.live,&same.live));
        registry.invalidate_path(&source.path);assert!(!first.is_live());
        let second=registry.capture(&source).unwrap();assert!(registry.check(&second).is_ok());
        assert!(registry.check(&first).is_err()&&registry.check(&same).is_err());
        assert!(!Arc::ptr_eq(&first.live,&second.live));
    }
    #[test]
    fn one_source_change_does_not_invalidate_another_mouse() {
        let mut registry=Registry::default();let a=registry.capture(&source("A")).unwrap();
        let b=registry.capture(&source("B")).unwrap();registry.invalidate_path(&a.source.path);
        assert!(registry.check(&a).is_err());assert!(registry.check(&b).is_ok());
        let mut reused=b.source.clone();reused.vendor=0x1038;reused.product=0x1720;
        assert!(registry.capture(&reused).is_err());assert!(registry.check(&b).is_ok());
        registry.invalidate_path(&b.source.path);
        let other=registry.capture(&reused).unwrap();assert!(registry.check(&b).is_err());assert!(registry.check(&other).is_ok());
    }
    #[test]
    fn unknown_topology_and_shutdown_invalidate_all_without_counter_wrap() {
        let mut registry=Registry::default();let a=registry.capture(&source("A")).unwrap();
        let b=registry.capture(&source("B")).unwrap();registry.invalidate_all();
        assert!(registry.check(&a).is_err()&&registry.check(&b).is_err());
        let new=registry.capture(&source("A")).unwrap();registry.close();
        assert!(registry.check(&new).is_err());assert!(registry.capture(&source("A")).is_err());
    }
    #[test]
    fn dropped_preparations_are_pruned_and_only_this_registry_can_accept_a_lease() {
        let mut registry=Registry::default();
        for i in 0..2048 { drop(registry.capture(&source(&i.to_string())).unwrap()); }
        let lease=registry.capture(&source("held")).unwrap();assert_eq!(registry.slots.len(),1);
        let other=Registry::default();assert!(other.check(&lease).is_err());
    }
}

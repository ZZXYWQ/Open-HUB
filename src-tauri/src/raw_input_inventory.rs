//! Handle inventory for Raw Input topology notifications. Keep unrelated handles
//! so their removal does not invalidate a mouse that has its own verified path.
use super::{Event, Source};
use std::collections::HashMap;

#[derive(Clone)]
enum Entry { Known(Option<Source>), Unresolved }
#[derive(Default)]
pub(super) struct Inventory { entries: HashMap<usize, Entry> }
impl Inventory {
    pub(super) fn seed(&mut self,handle:usize,source:Result<Option<Source>,()>) {
        self.entries.insert(handle,match source { Ok(source)=>Entry::Known(source),Err(())=>Entry::Unresolved });
    }
    pub(super) fn get(&self,handle:usize)->Option<Source> {
        match self.entries.get(&handle) { Some(Entry::Known(source))=>source.clone(),_=>None }
    }
    pub(super) fn observe(&mut self,handle:usize,source:Option<Source>,arrival:bool)->Vec<Event> {
        let previous=self.entries.insert(handle,Entry::Known(source.clone()));
        let old=match previous { Some(Entry::Known(old))=>old,_=>None };
        if !arrival&&old==source { return vec![]; }
        let mut events=Vec::new();
        if old!=source { if let Some(old)=old { events.push(Event::Removed { path:old.path }); } }
        if let Some(source)=source { events.push(Event::Arrived { source }); }
        events
    }
    pub(super) fn unresolved(&mut self,handle:usize,arrival:bool)->Vec<Event> {
        let previous=self.entries.insert(handle,Entry::Unresolved);
        if !arrival&&matches!(previous,Some(Entry::Unresolved)) { return vec![]; }
        let mut events=Vec::new();
        if let Some(Entry::Known(Some(old)))=previous { events.push(Event::Removed { path:old.path }); }
        events.push(Event::InventoryUncertain);events
    }
    pub(super) fn remove(&mut self,handle:usize)->Vec<Event> {
        match self.entries.remove(&handle) {
            Some(Entry::Known(Some(source)))=>vec![Event::Removed { path:source.path }],
            Some(Entry::Known(None))=>vec![],
            Some(Entry::Unresolved)|None=>vec![Event::InventoryUncertain],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(unit:&str)->Source { Source { vendor:0x1532,product:0x00b6,path:format!("simulation:{unit}") } }
    #[test]
    fn existing_handle_is_quiet_but_rearrival_of_same_path_starts_new_lifetime() {
        let mut inv=Inventory::default();inv.seed(1,Ok(Some(source("A"))));
        assert!(inv.observe(1,Some(source("A")),false).is_empty());
        assert_eq!(inv.observe(1,Some(source("A")),true),vec![Event::Arrived { source:source("A") }]);
    }
    #[test]
    fn recycled_handle_invalidates_previous_path_before_publishing_the_new_one() {
        let mut inv=Inventory::default();inv.seed(1,Ok(Some(source("A"))));
        assert_eq!(inv.observe(1,Some(source("B")),true),vec![Event::Removed { path:source("A").path },Event::Arrived { source:source("B") }]);
        assert_eq!(inv.get(1),Some(source("B")));
        assert_eq!(inv.remove(1),vec![Event::Removed { path:source("B").path }]);
    }
    #[test]
    fn unrelated_known_handle_leaves_other_mouse_routes_alone() {
        let mut inv=Inventory::default();inv.seed(1,Ok(Some(source("A"))));inv.seed(2,Ok(None));
        assert!(inv.observe(2,None,true).is_empty());assert!(inv.remove(2).is_empty());
        assert_eq!(inv.get(1),Some(source("A")));
        assert_eq!(inv.remove(42),vec![Event::InventoryUncertain]);
    }
    #[test]
    fn unresolvable_arrival_or_removal_cannot_leave_stale_mouse_authorization() {
        let mut inv=Inventory::default();inv.seed(1,Ok(Some(source("A"))));
        assert_eq!(inv.unresolved(1,true),vec![Event::Removed { path:source("A").path },Event::InventoryUncertain]);
        assert!(inv.unresolved(1,false).is_empty());
        assert_eq!(inv.remove(1),vec![Event::InventoryUncertain]);
        assert_eq!(inv.observe(1,Some(source("A")),false),vec![Event::Arrived { source:source("A") }]);
    }
}

//! A timed-out input listener may finish creating its window later. Cancellation
//! and registration share a lock so it cannot steal a retry's registration.
use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};
static REGISTRATION:Mutex<()>=Mutex::new(());
#[derive(Default)]
pub(super) struct Startup { cancelled:AtomicBool }
impl Startup {
    pub(super) fn register(&self,register:impl FnOnce()->Result<(),String>)->Result<(),String> {
        let _guard=REGISTRATION.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.cancelled.load(Ordering::Acquire) { return Err("输入监听启动已取消".into()); }
        register()
    }
    pub(super) fn cancel(&self) {
        let _guard=REGISTRATION.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        self.cancelled.store(true,Ordering::Release);
    }
}
#[cfg(test)]mod tests {
    use super::*;use std::sync::{Arc,mpsc};use std::time::Duration;
    #[test]
    fn late_startup_cannot_register_after_timeout_and_successful_retry() {
        let old=Arc::new(Startup::default());let late=old.clone();
        let (go,wait)=mpsc::channel();let log=Arc::new(Mutex::new(Vec::new()));let record=log.clone();
        let worker=std::thread::spawn(move||{wait.recv().unwrap();late.register(||{record.lock().unwrap().push("old");Ok(())})});
        old.cancel();Startup::default().register(||{log.lock().unwrap().push("new");Ok(())}).unwrap();
        go.send(()).unwrap();assert!(worker.join().unwrap().is_err());assert_eq!(*log.lock().unwrap(),vec!["new"]);
    }
    #[test]
    fn cancellation_waits_for_inflight_registration_before_allowing_retry() {
        let old=Arc::new(Startup::default());let late=old.clone();let log=Arc::new(Mutex::new(Vec::new()));let record=log.clone();
        let (entered,ready)=mpsc::channel();let (release,wait)=mpsc::channel();
        let worker=std::thread::spawn(move||late.register(||{entered.send(()).unwrap();wait.recv().unwrap();record.lock().unwrap().push("old");Ok(())}));
        ready.recv_timeout(Duration::from_secs(1)).unwrap();let cancel=old.clone();let (done,completed)=mpsc::channel();
        let cancelling=std::thread::spawn(move||{cancel.cancel();done.send(()).unwrap();});
        assert!(completed.recv_timeout(Duration::from_millis(10)).is_err());release.send(()).unwrap();
        completed.recv_timeout(Duration::from_secs(1)).unwrap();worker.join().unwrap().unwrap();cancelling.join().unwrap();
        Startup::default().register(||{log.lock().unwrap().push("new");Ok(())}).unwrap();
        assert_eq!(*log.lock().unwrap(),vec!["old","new"]);
    }
}

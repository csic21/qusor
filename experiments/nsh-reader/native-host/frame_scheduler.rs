//! Rust-local task/result retirement only. No Host acknowledgement or renderer
//! release is implied, including when `local_drained()` returns true.
#[path = "retirement_owner.rs"]
mod retirement_owner;
use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub mount: String,
    pub generation: u64,
    pub frame: String,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub closed: bool,
    pub pending: usize,
    pub running: usize,
    pub undelivered: usize,
    pub delivered_local: usize,
    pub handed_to_bridge: u64,
    pub encoded_containers: usize,
    pub encoded_referenced_bytes: usize,
    pub uncertain: bool,
}
impl Snapshot {
    /// Closed Rust task/result lanes only; encoded_containers can still be >0
    /// through actual Blob/ImageData references, never treated as native release.
    pub fn local_drained(&self) -> bool {
        self.closed
            && !self.uncertain
            && self.pending == 0
            && self.running == 0
            && self.undelivered == 0
            && self.delivered_local == 0
    }
}
struct OwnerState {
    scope: Arc<()>,
    id: u64,
    binding: Binding,
    closed: AtomicBool,
    stop: Arc<AtomicBool>,
    counters: Mutex<Snapshot>,
    uncertain: AtomicBool,
}
#[derive(Clone)]
pub struct Owner(Arc<OwnerState>);
impl Owner {
    pub fn mark_unknown(&self) {
        self.0.uncertain.store(true, Ordering::SeqCst);
    }
    pub fn charge_encoded(&self, bytes: usize) -> Result<()> {
        let mut s = self
            .0
            .counters
            .lock()
            .map_err(|_| "owner counters poisoned")?;
        if s.closed || self.0.closed.load(Ordering::SeqCst) || self.0.stop.load(Ordering::SeqCst) {
            return Err("closed owner cannot create encoded data".into());
        }
        let containers = s
            .encoded_containers
            .checked_add(1)
            .ok_or("encoded containers exhausted")?;
        let total = s
            .encoded_referenced_bytes
            .checked_add(bytes)
            .ok_or("encoded bytes exhausted")?;
        s.encoded_containers = containers;
        s.encoded_referenced_bytes = total;
        Ok(())
    }
    pub fn release_encoded(&self, bytes: usize) {
        self.change(|s| {
            match (
                s.encoded_containers.checked_sub(1),
                s.encoded_referenced_bytes.checked_sub(bytes),
            ) {
                (Some(containers), Some(total)) => {
                    s.encoded_containers = containers;
                    s.encoded_referenced_bytes = total;
                }
                _ => s.uncertain = true,
            }
        });
    }
    #[cfg(test)]
    pub fn same_identity(&self, other: &Owner) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn binding(&self) -> &Binding {
        &self.0.binding
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        let mut s = self
            .0
            .counters
            .lock()
            .map_err(|_| "owner counters poisoned")?
            .clone();
        s.uncertain |= self.0.uncertain.load(Ordering::SeqCst);
        Ok(s)
    }
    fn change(&self, f: impl FnOnce(&mut Snapshot)) {
        match self.0.counters.lock() {
            Ok(mut s) => f(&mut s),
            Err(_) => self.0.uncertain.store(true, Ordering::SeqCst),
        }
    }
}
#[derive(Clone)]
pub struct Cancellation {
    owner: Owner,
    task: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}
impl Cancellation {
    pub fn owner(&self) -> Owner {
        self.owner.clone()
    }
    pub fn cancelled(&self) -> bool {
        self.task.load(Ordering::SeqCst)
            || self.stop.load(Ordering::SeqCst)
            || self.owner.0.closed.load(Ordering::SeqCst)
    }
}
struct Completion<T> {
    value: Option<Result<T>>,
    waker: Option<Waker>,
    abandoned: bool,
    charged: bool,
}
struct Task<T> {
    owner: Owner,
    cancel: Cancellation,
    work: Box<dyn FnOnce(Cancellation) -> Result<T> + Send>,
    completion: Arc<Mutex<Completion<T>>>,
}
struct Queue<T> {
    pending: Option<Task<T>>,
    running: Option<Cancellation>,
    stopped: bool,
    next: u64,
    owners: BTreeMap<u64, Owner>,
    completions: Vec<(Owner, std::sync::Weak<Mutex<Completion<T>>>)>,
}
struct Shared<T> {
    scope: Arc<()>,
    queue: Mutex<Queue<T>>,
    wake: Condvar,
    exited: AtomicBool,
    stopping: Arc<AtomicBool>,
}
pub use retirement_owner::Phase as RetirementPhase;
#[derive(Clone)]
pub struct RetirementTicket(retirement_owner::Token);
impl RetirementTicket {
    pub fn phase(&self) -> RetirementPhase {
        self.0.phase()
    }
    pub fn joined(&self) -> Result<bool> {
        self.0.joined()
    }
    pub fn wait_until(&self, deadline: Instant) -> Result<bool> {
        loop {
            if self.joined()? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
}
pub fn shutdown_retirement_until(deadline: Instant) -> Result<bool> {
    retirement_owner::shutdown_until(deadline)
}
pub struct FrameScheduler<T: Send + 'static> {
    shared: Arc<Shared<T>>,
    retirement: retirement_owner::Token,
}
/// Clone group retains one local-result charge until its last Rust clone drops.
#[derive(Clone)]
pub struct LocalResultLease(Arc<LocalResultHold>);
struct LocalResultHold {
    owner: Owner,
}
impl LocalResultLease {
    /// Call immediately BEFORE attempting JsHost::new_file. Failure after this
    /// boundary remains unknown, never decremented as if native release occurred.
    pub fn record_bridge_attempt(&self) -> Result<()> {
        let mut s = self
            .0
            .owner
            .0
            .counters
            .lock()
            .map_err(|_| "frame counters poisoned")?;
        if s.closed
            || self.0.owner.0.closed.load(Ordering::SeqCst)
            || self.0.owner.0.stop.load(Ordering::SeqCst)
        {
            return Err("closed frame cannot create a bridge File".into());
        }
        s.handed_to_bridge = s
            .handed_to_bridge
            .checked_add(1)
            .ok_or("bridge ledger exhausted")?;
        Ok(())
    }
}
impl Drop for LocalResultHold {
    fn drop(&mut self) {
        self.owner.change(|s| s.delivered_local -= 1);
    }
}
/// Retains Rust-local delivered ownership until dropped or explicitly handed off.
pub struct Delivery<T> {
    value: Option<T>,
    owner: Owner,
    charged: bool,
}
impl<T> Delivery<T> {
    #[cfg(test)]
    pub fn value(&self) -> &T {
        self.value.as_ref().expect("delivery consumed")
    }
    /// Moves the local charge into a concrete bridge value, preserving its ABI.
    /// The caller must drop the payload BEFORE the last LocalResultLease clone.
    pub fn into_local_value(mut self) -> (T, LocalResultLease) {
        let value = self.value.take().expect("delivery consumed");
        self.charged = false;
        let lease = LocalResultLease(Arc::new(LocalResultHold {
            owner: self.owner.clone(),
        }));
        (value, lease)
    }
    /// Transfers beyond this scheduler's proof boundary, not a resource release.
    #[cfg(test)]
    pub fn handoff_to_bridge(mut self) -> T {
        let value = self.value.take().expect("delivery consumed");
        self.owner
            .change(|s| s.handed_to_bridge = s.handed_to_bridge.saturating_add(1));
        value
    }
}
impl<T> Drop for Delivery<T> {
    fn drop(&mut self) {
        drop(self.value.take());
        if self.charged {
            self.owner.change(|s| s.delivered_local -= 1);
        }
    }
}
pub struct FrameJob<T> {
    owner: Owner,
    completion: Arc<Mutex<Completion<T>>>,
    cancel: Cancellation,
}
impl<T> Future for FrameJob<T> {
    type Output = Result<Delivery<T>>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = match self.completion.lock() {
            Ok(s) => s,
            Err(_) => {
                self.owner.0.uncertain.store(true, Ordering::SeqCst);
                return Poll::Ready(Err("completion poisoned".into()));
            }
        };
        if let Some(value) = state.value.take() {
            let charged = std::mem::replace(&mut state.charged, false);
            // Keep the payload charged until destroyed, or atomically move its
            // charge to the delivered lease. Never expose a false zero gap.
            if self.cancel.cancelled() {
                drop(value);
                if charged {
                    self.owner.change(|s| s.undelivered -= 1);
                }
                return Poll::Ready(Err("frame owner closed or request cancelled".into()));
            }
            return Poll::Ready(match value {
                Ok(value) => {
                    self.owner.change(|s| {
                        if charged {
                            s.undelivered -= 1;
                        }
                        s.delivered_local += 1;
                    });
                    Ok(Delivery {
                        value: Some(value),
                        owner: self.owner.clone(),
                        charged: true,
                    })
                }
                Err(error) => {
                    if charged {
                        self.owner.change(|s| s.undelivered -= 1);
                    }
                    Err(error)
                }
            });
        }
        if self.cancel.cancelled() {
            return Poll::Ready(Err("frame owner closed or request cancelled".into()));
        }
        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}
impl<T> Drop for FrameJob<T> {
    fn drop(&mut self) {
        self.cancel.task.store(true, Ordering::SeqCst);
        match self.completion.lock() {
            Ok(mut s) => {
                s.abandoned = true;
                let value = s.value.take();
                drop(value);
                if s.charged {
                    s.charged = false;
                    self.owner.change(|c| c.undelivered -= 1);
                }
                s.waker = None;
            }
            Err(_) => self.owner.0.uncertain.store(true, Ordering::SeqCst),
        }
    }
}
fn finish<T>(task: Task<T>, run: bool) {
    let Task {
        owner,
        cancel,
        work,
        completion,
    } = task;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if run && !cancel.cancelled() {
            work(cancel.clone())
        } else {
            drop(work);
            Err("frame superseded or closed".into())
        }
    }))
    .unwrap_or_else(|_| Err("frame work panicked".into()));
    // Running/pending is charged until payload and closure are actually settled.
    let wake = match completion.lock() {
        Ok(mut s) => {
            if s.abandoned || cancel.cancelled() {
                drop(result);
            } else {
                s.value = Some(result);
                s.charged = true;
                owner.change(|c| c.undelivered += 1);
            }
            s.waker.take()
        }
        Err(_) => {
            drop(result);
            owner.0.uncertain.store(true, Ordering::SeqCst);
            None
        }
    };
    owner.change(|s| if run { s.running -= 1 } else { s.pending -= 1 });
    if let Some(w) = wake
        && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.wake())).is_err()
    {
        owner.0.uncertain.store(true, Ordering::SeqCst);
    }
}
fn cancel_completion<T>(owner: &Owner, completion: Arc<Mutex<Completion<T>>>) {
    let (value, charged, wake) = match completion.lock() {
        Ok(mut s) => {
            let value = s.value.take();
            let charged = std::mem::replace(&mut s.charged, false);
            if !s.abandoned {
                s.value = Some(Err("frame owner closed".into()));
            }
            (value, charged, s.waker.take())
        }
        Err(_) => {
            owner.0.uncertain.store(true, Ordering::SeqCst);
            return;
        }
    };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(value))).is_err() {
        owner.0.uncertain.store(true, Ordering::SeqCst);
    }
    if charged {
        owner.change(|s| s.undelivered -= 1);
    }
    if let Some(w) = wake
        && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.wake())).is_err()
    {
        owner.0.uncertain.store(true, Ordering::SeqCst);
    }
}
fn close_shared<T>(shared: &Shared<T>) -> Result<()> {
    shared.stopping.store(true, Ordering::SeqCst);

    let (pending, completions) = {
        let mut q = shared.queue.lock().map_err(|_| "queue poisoned")?;
        q.stopped = true;
        for o in q.owners.values() {
            o.0.closed.store(true, Ordering::SeqCst);
            o.change(|s| s.closed = true);
        }
        if let Some(r) = &q.running {
            r.task.store(true, Ordering::SeqCst);
        }
        let completions = q
            .completions
            .iter()
            .filter_map(|(o, w)| w.upgrade().map(|c| (o.clone(), c)))
            .collect::<Vec<_>>();
        (q.pending.take(), completions)
    };
    shared.wake.notify_all();
    if let Some(t) = pending {
        t.cancel.task.store(true, Ordering::SeqCst);
        finish(t, false);
    }
    for (o, c) in completions {
        cancel_completion(&o, c);
    }
    Ok(())
}

impl<T: Send + 'static> FrameScheduler<T> {
    pub fn new() -> Result<Self> {
        let retirement = retirement_owner::Token::reserve()?;
        let shared = Arc::new(Shared {
            scope: Arc::new(()),
            queue: Mutex::new(Queue {
                pending: None,
                running: None,
                stopped: false,
                next: 0,
                owners: BTreeMap::new(),
                completions: Vec::new(),
            }),
            wake: Condvar::new(),
            exited: AtomicBool::new(false),
            stopping: Arc::new(AtomicBool::new(false)),
        });
        let worker = shared.clone();
        let spawned = thread::Builder::new()
            .name("frame-retirement-candidate".into())
            .spawn(move || {
                struct Exit<T>(Arc<Shared<T>>);
                impl<T> Drop for Exit<T> {
                    fn drop(&mut self) {
                        self.0.exited.store(true, Ordering::SeqCst);
                        self.0.wake.notify_all();
                    }
                }
                let _exit = Exit(worker.clone());
                loop {
                    let task = {
                        let mut q = match worker.queue.lock() {
                            Ok(q) => q,
                            Err(_) => return,
                        };
                        while q.pending.is_none()
                            && !q.stopped
                            && !worker.stopping.load(Ordering::SeqCst)
                        {
                            q = match worker.wake.wait(q) {
                                Ok(q) => q,
                                Err(_) => return,
                            };
                        }
                        if q.stopped {
                            return;
                        }
                        let Some(t) = q.pending.take() else { continue };
                        t.owner.change(|s| {
                            s.pending -= 1;
                            s.running += 1
                        });
                        q.running = Some(t.cancel.clone());
                        t
                    };
                    finish(task, true);
                    let mut q = match worker.queue.lock() {
                        Ok(q) => q,
                        Err(_) => return,
                    };
                    q.running = None;
                    worker.wake.notify_all();
                }
            });
        let handle = match spawned {
            Ok(h) => h,
            Err(e) => {
                retirement.not_spawned();
                return Err(e.to_string());
            }
        };
        let cleanup = shared.clone();
        retirement.attach(
            handle,
            Arc::new(move || {
                let result = close_shared(&cleanup);
                cleanup.wake.notify_all();
                result
            }),
        );
        Ok(Self { shared, retirement })
    }
    /// Trusted adapter allocates once per non-reusable mount/frame identity and retains
    /// the opaque Owner. A descriptive Binding never authorizes a guest callback.
    pub fn new_owner(&self, binding: Binding) -> Result<Owner> {
        if binding.mount.is_empty()
            || binding.mount.len() > 256
            || binding.frame.is_empty()
            || binding.frame.len() > 256
            || binding.generation == 0
        {
            return Err("invalid trusted frame binding bounds".into());
        }
        let mut q = self.shared.queue.lock().map_err(|_| "queue poisoned")?;
        if q.stopped || self.shared.stopping.load(Ordering::SeqCst) {
            return Err("scheduler stopped".into());
        }
        q.completions.retain(|(_, w)| w.strong_count() > 0);
        // Retire TASK records only. Live encoded guards independently retain
        // the opaque Owner and their real counters; no native quota is returned.
        q.owners
            .retain(|_, o| !(o.snapshot().is_ok_and(|s| s.local_drained())));
        if q.owners.len() >= 64 {
            return Err("local owner budget reached".into());
        }
        q.next = q.next.checked_add(1).ok_or("owner identity exhausted")?;
        let owner = Owner(Arc::new(OwnerState {
            scope: self.shared.scope.clone(),
            id: q.next,
            binding,
            closed: AtomicBool::new(false),
            stop: self.shared.stopping.clone(),
            counters: Mutex::new(Snapshot::default()),
            uncertain: AtomicBool::new(false),
        }));
        q.owners.insert(owner.0.id, owner.clone());
        Ok(owner)
    }
    fn validate(&self, owner: &Owner) -> Result<()> {
        if !Arc::ptr_eq(&owner.0.scope, &self.shared.scope) {
            return Err("foreign scheduler owner".into());
        }
        Ok(())
    }
    pub fn submit(
        &self,
        owner: &Owner,
        work: impl FnOnce(Cancellation) -> Result<T> + Send + 'static,
    ) -> Result<FrameJob<T>> {
        self.validate(owner)?;
        let completion = Arc::new(Mutex::new(Completion {
            value: None,
            waker: None,
            abandoned: false,
            charged: false,
        }));
        let cancel = Cancellation {
            owner: owner.clone(),
            task: Arc::new(AtomicBool::new(false)),
            stop: self.shared.stopping.clone(),
        };
        let task = Task {
            owner: owner.clone(),
            cancel: cancel.clone(),
            work: Box::new(work),
            completion: completion.clone(),
        };
        let previous = {
            let mut q = self.shared.queue.lock().map_err(|_| "queue poisoned")?;
            if q.stopped
                || self.shared.stopping.load(Ordering::SeqCst)
                || owner.0.closed.load(Ordering::SeqCst)
            {
                return Err("owner or scheduler closed".into());
            }
            let snapshot = owner.snapshot()?;
            if snapshot.pending + snapshot.running + snapshot.undelivered + snapshot.delivered_local
                >= 4
            {
                return Err("owner local task/result budget reached".into());
            }
            q.completions.retain(|(_, w)| w.strong_count() > 0);
            if q.completions.len() >= 256 {
                return Err("local completion handle budget reached".into());
            }
            q.completions
                .push((owner.clone(), Arc::downgrade(&completion)));
            owner.change(|s| s.pending += 1);
            q.pending.replace(task)
        };
        self.shared.wake.notify_one();
        if let Some(t) = previous {
            t.cancel.task.store(true, Ordering::SeqCst);
            finish(t, false);
        }
        Ok(FrameJob {
            owner: owner.clone(),
            completion,
            cancel,
        })
    }
    /// Invalidates now; cannot prove a blocked worker has exited. No waits here.
    pub fn close_owner(&self, owner: &Owner) -> Result<Snapshot> {
        self.validate(owner)?;
        let (pending, completions) = {
            let mut q = self.shared.queue.lock().map_err(|_| "queue poisoned")?;
            owner.0.closed.store(true, Ordering::SeqCst);
            owner.change(|s| s.closed = true);
            if let Some(r) = &q.running
                && Arc::ptr_eq(&r.owner.0, &owner.0)
            {
                r.task.store(true, Ordering::SeqCst);
            }
            let pending = if q
                .pending
                .as_ref()
                .is_some_and(|t| Arc::ptr_eq(&t.owner.0, &owner.0))
            {
                q.pending.take()
            } else {
                None
            };
            let completions = q
                .completions
                .iter()
                .filter(|(o, _)| Arc::ptr_eq(&o.0, &owner.0))
                .filter_map(|(_, w)| w.upgrade())
                .collect::<Vec<_>>();
            (pending, completions)
        };
        if let Some(t) = pending {
            t.cancel.task.store(true, Ordering::SeqCst);
            finish(t, false);
        }
        for c in completions {
            cancel_completion(owner, c);
        }
        owner.snapshot()
    }
    /// Nonblocking request only. A process-lifetime retirement owner drains
    /// payloads and joins outside this caller (including caller-held Gate locks).
    pub fn close(&self) -> Result<()> {
        self.shared.stopping.store(true, Ordering::SeqCst);
        self.retirement.request();
        Ok(())
    }
    pub fn retirement_ticket(&self) -> RetirementTicket {
        RetirementTicket(self.retirement.clone())
    }
    #[cfg(test)]
    pub fn try_join(&self) -> Result<bool> {
        match self.retirement.joined() {
            Ok(joined) => Ok(joined),
            Err(e) => {
                self.mark_uncertain();
                Err(e)
            }
        }
    }
    /// Call only on a retirement/shutdown thread with no Gate/UI/queue lock held.
    /// Timeout retains the owned JoinHandle and reports pending, never success.
    #[cfg(test)]
    pub fn join_until(&self, deadline: Instant) -> Result<bool> {
        loop {
            if self.try_join()? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[cfg(test)]
    fn mark_uncertain(&self) {
        let queue = self.shared.queue.lock().unwrap_or_else(|p| p.into_inner());
        for owner in queue.owners.values() {
            owner.0.uncertain.store(true, Ordering::SeqCst);
        }
    }
    #[cfg(test)]
    pub fn exited(&self) -> bool {
        self.shared.exited.load(Ordering::SeqCst)
    }
}
impl<T: Send + 'static> Drop for FrameScheduler<T> {
    fn drop(&mut self) {
        // No join, user work/payload/Waker Drop, or queue traversal on this thread.
        // The independently owned slot already holds the actual JoinHandle.
        self.shared.stopping.store(true, Ordering::SeqCst);
        self.retirement.request();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{atomic::AtomicUsize, mpsc};
    fn binding() -> Binding {
        Binding {
            mount: "opaque-native-owner".into(),
            generation: 3,
            frame: "frame-a".into(),
        }
    }
    fn until(mut f: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(2);
        while !f() {
            assert!(Instant::now() < end, "actual worker did not settle");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn poll<T>(job: &mut FrameJob<T>) -> Poll<Result<Delivery<T>>> {
        Pin::new(job).poll(&mut Context::from_waker(Waker::noop()))
    }
    struct Probe(Arc<AtomicUsize>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn blocked_worker_close_remains_pending_until_real_exit_and_join() {
        let scheduler = FrameScheduler::<usize>::new().unwrap();
        let owner = scheduler.new_owner(binding()).unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (go_tx, go_rx) = mpsc::channel();
        let mut job = scheduler
            .submit(&owner, move |cancel| {
                entered_tx.send(()).unwrap();
                go_rx.recv().unwrap();
                assert!(cancel.cancelled());
                Ok(17)
            })
            .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        scheduler.close().unwrap();
        assert!(!scheduler.exited());
        assert!(!scheduler.try_join().unwrap());
        assert!(
            !scheduler
                .join_until(Instant::now() + Duration::from_millis(15))
                .unwrap()
        );
        let s = owner.snapshot().unwrap();
        assert_eq!(s.running, 1);
        assert!(!s.local_drained());
        assert!(matches!(poll(&mut job), Poll::Ready(Err(_))));
        go_tx.send(()).unwrap();
        assert!(
            scheduler
                .join_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
        assert!(scheduler.exited());
        assert!(owner.snapshot().unwrap().local_drained());
        assert!(scheduler.try_join().unwrap());
    }
    #[test]
    fn closing_owner_drops_unpolled_payload_even_when_future_stays_alive() {
        let scheduler = FrameScheduler::new().unwrap();
        let owner = scheduler.new_owner(binding()).unwrap();
        let drops = Arc::new(AtomicUsize::new(0));
        let d = drops.clone();
        let mut job = scheduler.submit(&owner, move |_| Ok(Probe(d))).unwrap();
        until(|| owner.snapshot().unwrap().undelivered == 1);
        let s = scheduler.close_owner(&owner).unwrap();
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert!(s.local_drained());
        assert!(matches!(poll(&mut job), Poll::Ready(Err(_))));
        scheduler.close().unwrap();
        assert!(
            scheduler
                .join_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
    }
    #[test]
    fn abandoned_future_keeps_running_charge_until_actual_work_returns() {
        let scheduler = FrameScheduler::new().unwrap();
        let owner = scheduler.new_owner(binding()).unwrap();
        let drops = Arc::new(AtomicUsize::new(0));
        let d = drops.clone();
        let (tx, rx) = mpsc::channel();
        let (go, wait) = mpsc::channel();
        let job = scheduler
            .submit(&owner, move |_| {
                tx.send(()).unwrap();
                wait.recv().unwrap();
                Ok(Probe(d))
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(job);
        scheduler.close_owner(&owner).unwrap();
        assert_eq!(owner.snapshot().unwrap().running, 1);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        go.send(()).unwrap();
        until(|| owner.snapshot().unwrap().local_drained());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        scheduler.close().unwrap();
        assert!(
            scheduler
                .join_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
    }
    #[test]
    fn delivered_local_lease_and_bridge_boundary_are_distinct() {
        let scheduler = FrameScheduler::new().unwrap();
        let owner = scheduler.new_owner(binding()).unwrap();
        let drops = Arc::new(AtomicUsize::new(0));
        let d = drops.clone();
        let mut job = scheduler.submit(&owner, move |_| Ok(Probe(d))).unwrap();
        until(|| owner.snapshot().unwrap().undelivered == 1);
        let Poll::Ready(Ok(delivery)) = poll(&mut job) else {
            panic!("result")
        };
        assert_eq!(owner.snapshot().unwrap().delivered_local, 1);
        scheduler.close_owner(&owner).unwrap();
        assert!(!owner.snapshot().unwrap().local_drained());
        let external = delivery.handoff_to_bridge();
        let s = owner.snapshot().unwrap();
        assert!(s.local_drained());
        assert_eq!(s.handed_to_bridge, 1);
        assert_eq!(drops.load(Ordering::SeqCst), 0); // explicitly NOT renderer/native release
        drop(external);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        scheduler.close().unwrap();
        assert!(
            scheduler
                .join_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
    }
    #[test]
    fn old_owner_close_and_late_result_do_not_cancel_successor() {
        let scheduler = FrameScheduler::new().unwrap();
        let old = scheduler.new_owner(binding()).unwrap();
        let (tx, rx) = mpsc::channel();
        let (go, wait) = mpsc::channel();
        let mut oldjob = scheduler
            .submit(&old, move |_| {
                tx.send(()).unwrap();
                wait.recv().unwrap();
                Ok(1usize)
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
        scheduler.close_owner(&old).unwrap();
        let new = scheduler.new_owner(binding()).unwrap();
        let mut newjob = scheduler.submit(&new, |_| Ok(2)).unwrap();
        scheduler.close_owner(&old).unwrap();
        assert_eq!(new.snapshot().unwrap().pending, 1);
        go.send(()).unwrap();
        until(|| new.snapshot().unwrap().undelivered == 1);
        assert!(matches!(poll(&mut oldjob), Poll::Ready(Err(_))));
        let Poll::Ready(Ok(value)) = poll(&mut newjob) else {
            panic!("successor result")
        };
        assert_eq!(*value.value(), 2);
        drop(value);
        assert!(old.snapshot().unwrap().local_drained());
        scheduler.close_owner(&new).unwrap();
        assert!(new.snapshot().unwrap().local_drained());
        scheduler.close().unwrap();
        assert!(
            scheduler
                .join_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
    }
    #[test]
    fn panic_work_is_settled_and_foreign_owner_cannot_release_local_work() {
        let scheduler = FrameScheduler::<usize>::new().unwrap();
        let other = FrameScheduler::<usize>::new().unwrap();
        let owner = scheduler.new_owner(binding()).unwrap();
        assert!(other.close_owner(&owner).is_err());
        assert!(other.submit(&owner, |_| Ok(0)).is_err());
        let mut job = scheduler
            .submit(&owner, |_| panic!("controlled task panic"))
            .unwrap();
        until(|| owner.snapshot().unwrap().undelivered == 1);
        assert!(matches!(poll(&mut job), Poll::Ready(Err(_))));
        scheduler.close_owner(&owner).unwrap();
        assert!(owner.snapshot().unwrap().local_drained());
        scheduler.close().unwrap();
        other.close().unwrap();
        assert!(
            scheduler
                .join_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
        assert!(
            other
                .join_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
    }
    #[test]
    fn pending_closure_drop_can_reenter_scheduler_without_queue_lock_deadlock() {
        let scheduler = Arc::new(FrameScheduler::<usize>::new().unwrap());
        let owner = scheduler.new_owner(binding()).unwrap();
        let (tx, rx) = mpsc::channel();
        let (go, wait) = mpsc::channel();
        let first = scheduler
            .submit(&owner, move |_| {
                tx.send(()).unwrap();
                wait.recv().unwrap();
                Ok(0)
            })
            .unwrap();
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
        struct Reenter(Arc<FrameScheduler<usize>>, mpsc::Sender<()>);
        impl Drop for Reenter {
            fn drop(&mut self) {
                let _ = self.0.new_owner(binding());
                self.1.send(()).unwrap();
            }
        }
        let (done, observed) = mpsc::channel();
        let r = Reenter(scheduler.clone(), done);
        let pending = scheduler
            .submit(&owner, move |_| {
                drop(r);
                Ok(1)
            })
            .unwrap();
        let s = scheduler.clone();
        let o = owner.clone();
        let closer = thread::spawn(move || s.close_owner(&o).unwrap());
        let drop_result = observed.recv_timeout(Duration::from_secs(1));
        go.send(()).unwrap();
        assert!(drop_result.is_ok());
        closer.join().unwrap();
        drop((first, pending));
        scheduler.close().unwrap();
        assert!(
            scheduler
                .join_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
        assert!(owner.snapshot().unwrap().local_drained());
    }
    #[test]
    fn abandoned_completed_future_physically_drops_its_local_payload() {
        let scheduler = FrameScheduler::new().unwrap();
        let owner = scheduler.new_owner(binding()).unwrap();
        let drops = Arc::new(AtomicUsize::new(0));
        let d = drops.clone();
        let job = scheduler.submit(&owner, move |_| Ok(Probe(d))).unwrap();
        until(|| owner.snapshot().unwrap().undelivered == 1);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        drop(job);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(owner.snapshot().unwrap().undelivered, 0);
        scheduler.close_owner(&owner).unwrap();
        assert!(owner.snapshot().unwrap().local_drained());
        scheduler.close().unwrap();
        assert!(
            scheduler
                .join_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
    }
    #[test]
    fn last_state_arc_dropped_by_work_requests_retirement_without_self_join() {
        struct State {
            scheduler: FrameScheduler<usize>,
        }
        let state = Arc::new(State {
            scheduler: FrameScheduler::new().unwrap(),
        });
        let owner = state.scheduler.new_owner(binding()).unwrap();
        let retirement = state.scheduler.retirement_ticket();
        let held = state.clone();
        let (entered, at_work) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let (dropped, observed) = mpsc::channel();
        let mut job = state
            .scheduler
            .submit(&owner, move |_| {
                entered.send(()).unwrap();
                wait.recv().unwrap();
                drop(held); // Last State -> FrameScheduler Drop runs on its own worker.
                dropped.send(()).unwrap();
                Ok(42)
            })
            .unwrap();
        at_work.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(state);
        release.send(()).unwrap();
        observed
            .recv_timeout(Duration::from_secs(1))
            .expect("worker could not release its last State Arc");
        assert!(
            retirement
                .wait_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
        assert_eq!(retirement.phase(), RetirementPhase::Joined);
        assert!(matches!(poll(&mut job), Poll::Ready(Err(_))));
        assert!(owner.snapshot().unwrap().local_drained());
    }
    #[test]
    fn scheduler_drop_while_caller_holds_gate_does_not_wait_or_drop_user_closures() {
        let scheduler = FrameScheduler::<usize>::new().unwrap();
        let owner = scheduler.new_owner(binding()).unwrap();
        let retirement = scheduler.retirement_ticket();
        let gate = Arc::new(Mutex::new(()));
        let abort = Arc::new(AtomicBool::new(false));
        let worker_gate = gate.clone();
        let worker_abort = abort.clone();
        let (entered, at_work) = mpsc::channel();
        let (allow_work, may_work) = mpsc::channel();
        let running = scheduler
            .submit(&owner, move |_| {
                entered.send(()).unwrap();
                may_work.recv().unwrap();
                loop {
                    if worker_abort.load(Ordering::SeqCst) {
                        return Err("test cleanup".into());
                    }
                    if let Ok(_gate) = worker_gate.try_lock() {
                        return Ok(1);
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            })
            .unwrap();
        at_work.recv_timeout(Duration::from_secs(1)).unwrap();
        struct GateOnDrop {
            gate: Arc<Mutex<()>>,
            abort: Arc<AtomicBool>,
            dropped: Arc<AtomicBool>,
        }
        impl Drop for GateOnDrop {
            fn drop(&mut self) {
                loop {
                    if self.abort.load(Ordering::SeqCst) {
                        return;
                    }
                    if let Ok(_gate) = self.gate.try_lock() {
                        self.dropped.store(true, Ordering::SeqCst);
                        return;
                    }
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }
        let payload_dropped = Arc::new(AtomicBool::new(false));
        let probe = GateOnDrop {
            gate: gate.clone(),
            abort: abort.clone(),
            dropped: payload_dropped.clone(),
        };
        let pending = scheduler
            .submit(&owner, move |_| {
                drop(probe);
                Ok(2)
            })
            .unwrap();
        let (held, gate_held) = mpsc::channel();
        let (drop_now, may_drop) = mpsc::channel();
        let (returned, did_return) = mpsc::channel();
        let (unlock, may_unlock) = mpsc::channel();
        let caller_gate = gate.clone();
        let caller = thread::spawn(move || {
            let _gate = caller_gate.lock().unwrap();
            held.send(()).unwrap();
            may_drop.recv().unwrap();
            drop(scheduler);
            returned.send(()).unwrap();
            may_unlock.recv().unwrap();
        });
        gate_held.recv_timeout(Duration::from_secs(1)).unwrap();
        allow_work.send(()).unwrap();
        drop_now.send(()).unwrap();
        let result = did_return.recv_timeout(Duration::from_secs(1));
        if result.is_err() {
            abort.store(true, Ordering::SeqCst);
        }
        let joined_while_held = if result.is_ok() {
            retirement
                .wait_until(Instant::now() + Duration::from_millis(15))
                .unwrap()
        } else {
            false
        };
        unlock.send(()).unwrap();
        caller.join().unwrap();
        assert!(result.is_ok(), "scheduler Drop waited for caller-held Gate");
        assert!(
            !joined_while_held,
            "joined while actual worker and payload Drop were Gate-blocked"
        );
        assert!(
            retirement
                .wait_until(Instant::now() + Duration::from_secs(1))
                .unwrap()
        );
        assert!(payload_dropped.load(Ordering::SeqCst));
        drop((running, pending));
        assert!(owner.snapshot().unwrap().local_drained());
    }
}

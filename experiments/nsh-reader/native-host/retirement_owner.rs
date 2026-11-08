//! Process-lifetime bounded retirement owner. It never belongs to a frame State.
//! Drop of a frame scheduler only requests retirement. This service owns handles
//! before any work can cause the last scheduler Arc to be released on its worker.
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Condvar, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
type Cleanup = Arc<dyn Fn() -> Result<()> + Send + Sync>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Reserved,
    Active,
    Closing,
    Joined,
    NotSpawned,
    Unknown(String),
}
struct Slot {
    phase: Mutex<Phase>,
    requested: AtomicBool,
    handle: Mutex<Option<JoinHandle<()>>>,
    cleanup: Mutex<Option<Cleanup>>,
}
struct Inner {
    slots: Mutex<(u64, BTreeMap<u64, Arc<Slot>>)>,
    wake: Condvar,
    stopping: AtomicBool,
    exited: AtomicBool,
}
struct Service {
    inner: Arc<Inner>,
    thread: Mutex<Option<JoinHandle<()>>>,
    joined: AtomicBool,
}
static SERVICE: OnceLock<Result<Service>> = OnceLock::new();
#[derive(Clone)]
pub struct Token {
    inner: Arc<Inner>,
    slot: Arc<Slot>,
}
fn service() -> Result<&'static Service> {
    match SERVICE.get_or_init(Service::new) {
        Ok(s) => Ok(s),
        Err(e) => Err(e.clone()),
    }
}
impl Slot {
    fn unknown(&self, error: String) {
        if let Ok(mut phase) = self.phase.lock() {
            *phase = Phase::Unknown(error);
        }
    }
    fn phase(&self) -> Phase {
        self.phase
            .lock()
            .map(|p| p.clone())
            .unwrap_or_else(|_| Phase::Unknown("retirement phase poisoned".into()))
    }
}
impl Service {
    fn new() -> Result<Self> {
        let inner = Arc::new(Inner {
            slots: Mutex::new((0, BTreeMap::new())),
            wake: Condvar::new(),
            stopping: AtomicBool::new(false),
            exited: AtomicBool::new(false),
        });
        let actor = inner.clone();
        let handle = thread::Builder::new()
            .name("frame-retirement-owner".into())
            .spawn(move || {
                loop {
                    let slots = match actor.slots.lock() {
                        Ok(s) => s.1.values().cloned().collect::<Vec<_>>(),
                        Err(_) => return,
                    };
                    for slot in slots {
                        if actor.stopping.load(Ordering::SeqCst) {
                            slot.requested.store(true, Ordering::SeqCst);
                        }
                        if slot.requested.load(Ordering::SeqCst) && slot.phase() == Phase::Active {
                            if let Ok(mut phase) = slot.phase.lock() {
                                *phase = Phase::Closing;
                            }
                            let cleanup = match slot.cleanup.lock() {
                                Ok(c) => c.clone(),
                                Err(_) => {
                                    slot.unknown("retirement cleanup poisoned".into());
                                    None
                                }
                            };
                            // No registry, slot, queue or caller Gate lock held over user Drop/Waker.
                            if let Some(cleanup) = cleanup {
                                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    cleanup()
                                })) {
                                    Ok(Ok(())) => {}
                                    Ok(Err(e)) => slot.unknown(e),
                                    Err(_) => slot.unknown("retirement cleanup panicked".into()),
                                }
                            }
                        }
                        if slot.requested.load(Ordering::SeqCst) {
                            let handle = match slot.handle.lock() {
                                Ok(mut h) => {
                                    if h.as_ref().is_some_and(JoinHandle::is_finished) {
                                        h.take()
                                    } else {
                                        None
                                    }
                                }
                                Err(_) => {
                                    slot.unknown("retirement handle poisoned".into());
                                    None
                                }
                            };
                            if let Some(handle) = handle {
                                let joined = handle.join().is_ok();
                                if !joined {
                                    slot.unknown("frame worker join failed".into());
                                } else if !matches!(slot.phase(), Phase::Unknown(_))
                                    && let Ok(mut p) = slot.phase.lock()
                                {
                                    *p = Phase::Joined;
                                }
                                if joined
                                    && !matches!(slot.phase(), Phase::Unknown(_))
                                    && let Ok(mut c) = slot.cleanup.lock()
                                {
                                    *c = None;
                                }
                            }
                        }
                    }
                    let mut slots = match actor.slots.lock() {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    // Unknown slots are quarantined, never returned to the reserve budget.
                    slots
                        .1
                        .retain(|_, s| !matches!(s.phase(), Phase::Joined | Phase::NotSpawned));
                    if actor.stopping.load(Ordering::SeqCst) && slots.1.is_empty() {
                        actor.exited.store(true, Ordering::SeqCst);
                        actor.wake.notify_all();
                        return;
                    }
                    let _ = actor.wake.wait_timeout(slots, Duration::from_millis(2));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            inner,
            thread: Mutex::new(Some(handle)),
            joined: AtomicBool::new(false),
        })
    }
    fn reserve(&self) -> Result<Token> {
        let mut slots = self
            .inner
            .slots
            .lock()
            .map_err(|_| "retirement registry poisoned")?;
        if self.inner.stopping.load(Ordering::SeqCst) {
            return Err("retirement owner is stopping".into());
        }
        if slots.1.len() >= 16 {
            return Err("retirement slot budget reached".into());
        }
        slots.0 = slots
            .0
            .checked_add(1)
            .ok_or("retirement identity exhausted")?;
        let slot = Arc::new(Slot {
            phase: Mutex::new(Phase::Reserved),
            requested: AtomicBool::new(false),
            handle: Mutex::new(None),
            cleanup: Mutex::new(None),
        });
        let id = slots.0;
        slots.1.insert(id, slot.clone());
        Ok(Token {
            inner: self.inner.clone(),
            slot,
        })
    }
}
impl Token {
    pub fn reserve() -> Result<Self> {
        service()?.reserve()
    }
    pub fn attach(&self, handle: JoinHandle<()>, cleanup: Cleanup) {
        // Reserved slots are private before construction publishes the scheduler.
        // The supervisor owns the handle from this point; scheduler Drop cannot join it.
        *self.slot.handle.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle);
        *self.slot.cleanup.lock().unwrap_or_else(|p| p.into_inner()) = Some(cleanup);
        *self.slot.phase.lock().unwrap_or_else(|p| p.into_inner()) = Phase::Active;
        self.inner.wake.notify_all();
    }
    pub fn not_spawned(&self) {
        *self.slot.phase.lock().unwrap_or_else(|p| p.into_inner()) = Phase::NotSpawned;
        self.inner.wake.notify_all();
    }
    pub fn request(&self) {
        self.slot.requested.store(true, Ordering::SeqCst);
        self.inner.wake.notify_all();
    }
    pub fn phase(&self) -> Phase {
        self.slot.phase()
    }
    pub fn joined(&self) -> Result<bool> {
        match self.phase() {
            Phase::Joined => Ok(true),
            Phase::Unknown(e) => Err(e),
            _ => Ok(false),
        }
    }
}
/// Explicit process shutdown only, outside caller Gate/UI locks. Timeout keeps
/// every frame handle AND this service thread handle owned in the static service.
/// Unknown cleanup retains its slot and cannot report successful shutdown.
#[allow(dead_code)]
pub fn shutdown_until(deadline: Instant) -> Result<bool> {
    let s = service()?;
    s.inner.stopping.store(true, Ordering::SeqCst);
    s.inner.wake.notify_all();
    loop {
        let handle = {
            let mut thread = s
                .thread
                .lock()
                .map_err(|_| "retirement thread handle poisoned")?;
            if thread.as_ref().is_some_and(JoinHandle::is_finished) {
                thread.take()
            } else {
                None
            }
        };
        if let Some(handle) = handle {
            if handle.join().is_err() {
                return Err("retirement service join failed".into());
            }
            s.joined.store(true, Ordering::SeqCst);
        }
        if s.joined.load(Ordering::SeqCst) {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(1));
    }
}

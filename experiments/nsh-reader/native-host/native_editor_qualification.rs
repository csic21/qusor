//! Feature-only real OS retirement qualification. Not a guest/JS capability.
use super::*;
#[derive(Default)]
struct Plan {
    point: Option<&'static str>,
    short_wait: bool,
    injected: usize,
    reaps: Vec<String>,
    hold_guard: bool,
    yielded_guard: bool,
    short_shutdown: bool,
}
static PLAN: Mutex<Plan> = Mutex::new(Plan {
    point: None,
    short_wait: false,
    injected: 0,
    reaps: Vec::new(),
    hold_guard: false,
    yielded_guard: false,
    short_shutdown: false,
});
pub(super) fn fault(point: &str) -> Result<()> {
    let mut p = PLAN.lock().unwrap();
    if p.point == Some(point) {
        p.point = None;
        p.injected += 1;
        return Err(format!("qualification injected {point}"));
    }
    Ok(())
}
pub(super) fn observed_reap(run: &str) {
    PLAN.lock().unwrap().reaps.push(run.into());
}
pub(super) fn wait_limit() -> Duration {
    let mut p = PLAN.lock().unwrap();
    if p.short_wait {
        p.short_wait = false;
        Duration::ZERO
    } else {
        Duration::from_secs(8)
    }
}
pub(super) fn hold_real_guard() -> bool {
    let mut p = PLAN.lock().unwrap();
    if p.hold_guard {
        p.hold_guard = false;
        p.yielded_guard = true;
        true
    } else {
        false
    }
}
pub(super) fn yield_real_guard() -> bool {
    let mut p = PLAN.lock().unwrap();
    std::mem::take(&mut p.yielded_guard)
}
pub(super) fn shutdown_limit() -> Duration {
    let mut p = PLAN.lock().unwrap();
    if std::mem::take(&mut p.short_shutdown) {
        Duration::ZERO
    } else {
        Duration::from_secs(10)
    }
}
fn leases() -> Result<Vec<extension_store::WorkerLease>> {
    let s = service()?;
    let mut g = mutex(&s.gate)?;
    g.host
        .with_component_authority(|store, _| store.worker_leases().map_err(text))
}
fn report(label: &str) -> Result<()> {
    let p = PLAN.lock().unwrap();
    let rows = leases()?;
    println!(
        "NATIVE_RETIREMENT_CASE {}",
        json!({"case":label,"faults":p.injected,"actual_reaped":p.reaps,"leases":rows.len()})
    );
    Ok(())
}
fn input_revision() -> Result<u64> {
    let slot = mutex(&EDITOR)?;
    let e = slot.as_ref().ok_or("no editor")?;
    Ok(e.drafts
        .read_chunk(&e.draft, None, 0, 512)
        .map_err(text)?
        .revision)
}
fn fault_case(label: &str) -> Result<()> {
    let (_fixture, _, binding) = fixture::Fixture::new(label);
    *PLAN.lock().unwrap() = Plan {
        point: if label == "unknown_wait" {
            None
        } else {
            Some(match label {
                "invocation" => "invocation",
                "gate" => "gate",
                _ => "finish",
            })
        },
        short_wait: label == "unknown_wait",
        ..Plan::default()
    };
    let result = apply(
        binding.clone(),
        input_revision()?,
        "retirement fixture input".into(),
    );
    if result.is_ok() {
        return Err(format!("{label} did not fault"));
    }
    if label == "finish" || label == "unknown_wait" {
        {
            let slot = mutex(&EDITOR)?;
            let e = slot.as_ref().unwrap();
            let owner = e.pending_run.as_ref().ok_or("pending owner lost")?;
            let p = owner.lock().unwrap();
            if label == "finish" && (!p.reaped || p.lease_finished) {
                return Err("finish failure lost real reap/lease stage".into());
            }
            if label == "unknown_wait" && p.reaped {
                return Err("unknown wait unexpectedly already reaped".into());
            }
        }
        if leases()?.is_empty() {
            return Err("unresolved run lost SQL lease".into());
        }
        {
            let mut slot = mutex(&EDITOR)?;
            let e = slot.as_mut().unwrap();
            let _ = resume_run(e);
            if e.pending_run.is_some() {
                return Err("second drain did not retire".into());
            }
        }
    }
    {
        let slot = mutex(&EDITOR)?;
        if slot.as_ref().unwrap().pending_run.is_some() {
            return Err("retired run owner still pending".into());
        }
    }
    if PLAN.lock().unwrap().reaps.len() != 1 {
        return Err("expected one actual Controller Reaped".into());
    }
    if !leases()?.is_empty() {
        return Err("own lease not cleared after actual reap".into());
    }
    discard(binding)?;
    report(label)
}
// Keep a real Controller child at GuardReady while exercising unrelated discard.
fn guarded_lease(same_install: bool) -> Result<()> {
    let label = if same_install {
        "same_install"
    } else {
        "foreign_install"
    };
    let (_fixture, _, binding) = fixture::Fixture::new(label);
    *PLAN.lock().unwrap() = Plan::default();
    // First A completes authentic guest work and all of its actual reaps.
    apply(binding.clone(), input_revision()?, "A settled".into())?;
    let controller = Controller::new().map_err(text)?;
    let boot = if same_install {
        "abcdef0123456789abcdef0123456789"
    } else {
        "0123456789abcdef0123456789abcdef"
    };
    let run = format!("{boot}-1");
    {
        let s = service()?;
        let mut g = mutex(&s.gate)?;
        g.host.with_component_authority(|store, _| {
            let receipt = if same_install {
                let slot = mutex(&EDITOR)?;
                store
                    .current(&slot.as_ref().unwrap().package.manifest().package_id)
                    .map_err(text)?
            } else {
                let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../capture-core/evidence/component-author-b-projection");
                let key =
                    std::fs::read_to_string(root.join("publisher-public-key.txt")).map_err(text)?;
                let package = extension_package::package::verify_for_host(
                    &std::fs::read(root.join("provider-b.qpk")).map_err(text)?,
                    key.trim(),
                    &SUPPORTED_HOST_APIS,
                )
                .map_err(text)?;
                store
                    .install("qualification-install-b", 0, &package)
                    .map_err(text)?
            };
            store
                .prepare_worker_lease(
                    &receipt,
                    boot,
                    &run,
                    &runtime_process::current().map_err(text)?,
                )
                .map_err(text)?;
            store.mark_worker_spawn_attempted(&run).map_err(text)
        })?;
    }
    let handle = controller
        .start(WorkerLaunch {
            run_id: run.clone(),
            startup_timeout: Duration::from_secs(2),
            execution_timeout: ExecutionLimits::default().deadline,
        })
        .map_err(text)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut guarded = false;
    while !guarded && Instant::now() < deadline {
        for event in controller.poll() {
            if let ProcessEvent::Guarded { identity, .. } = event {
                let s = service()?;
                let mut g = mutex(&s.gate)?;
                g.host.with_component_authority(|store, _| {
                    store.bind_worker_lease(&run, identity).map_err(text)
                })?;
                guarded = true;
            }
        }
        if !guarded {
            thread::sleep(Duration::from_millis(1));
        }
    }
    if !guarded {
        return Err("real GuardReady missing".into());
    }
    let before = serde_json::to_string(&leases()?).map_err(text)?;
    let outcome = discard(binding.clone());
    let after = serde_json::to_string(&leases()?).map_err(text)?;
    let checked = if before != after {
        Err("discard changed other/unknown worker lease".into())
    } else if same_install && outcome.is_ok() {
        Err("same-install live lease failed to block".into())
    } else if !same_install && outcome.is_err() {
        Err(format!(
            "foreign installation blocked A discard: {outcome:?}"
        ))
    } else {
        Ok(())
    };
    controller.terminate(&handle).map_err(text)?;
    let until = Instant::now() + Duration::from_secs(8);
    let mut reaped = false;
    while !reaped && Instant::now() < until {
        for event in controller.poll() {
            if let ProcessEvent::Reaped { handle: h, .. } = event
                && h.run_id() == run
            {
                reaped = true;
            }
        }
        if !reaped {
            thread::sleep(Duration::from_millis(1));
        }
    }
    if !reaped {
        return Err("guarded fixture cleanup unconfirmed; lease retained".into());
    }
    {
        let s = service()?;
        let mut g = mutex(&s.gate)?;
        g.host
            .with_component_authority(|store, _| store.finish_worker_lease(&run).map_err(text))?;
    }
    controller.forget(&handle).map_err(text)?;
    checked?;
    if same_install {
        discard(binding)?;
    }
    println!(
        "NATIVE_RETIREMENT_CASE {}",
        json!({"case":label,"guarded":true,"actual_reaped":true,"lease_record_unchanged":before==after,"remaining_leases":leases()?.len()})
    );
    Ok(())
}
fn shutdown_case(live_guard: bool) -> Result<()> {
    let label = if live_guard {
        "shutdown_live_guard"
    } else {
        "shutdown_idle_unknown"
    };
    let (_fixture, _, binding) = fixture::Fixture::new(label);
    *PLAN.lock().unwrap() = Plan {
        short_wait: !live_guard,
        hold_guard: live_guard,
        ..Plan::default()
    };
    if apply(
        binding.clone(),
        input_revision()?,
        "shutdown input retained".into(),
    )
    .is_ok()
    {
        return Err("expected retained pending run".into());
    }
    if BUSY.load(Ordering::SeqCst) {
        return Err("shutdown fixture unexpectedly BUSY".into());
    }
    let owner = {
        let slot = mutex(&EDITOR)?;
        slot.as_ref()
            .unwrap()
            .pending_run
            .as_ref()
            .ok_or("pending owner lost")?
            .clone()
    };
    let held_service = service()?;
    let before = serde_json::to_string(&leases()?).map_err(text)?;
    if live_guard {
        let rows = leases()?;
        let lease = rows.first().ok_or("live lease missing")?;
        let identity = lease
            .worker
            .as_ref()
            .ok_or("GuardReady identity not bound")?;
        if runtime_process::inspect(identity.pid, &lease.run_id).map_err(text)? != *identity {
            return Err("real GuardReady child identity changed".into());
        }
        PLAN.lock().unwrap().short_shutdown = true;
        if super::super::shutdown().is_ok() {
            return Err("zero-budget live shutdown falsely succeeded".into());
        }
        if super::super::SERVICE.lock().map_err(text)?.is_none() {
            return Err("Service lost while worker live".into());
        }
        if serde_json::to_string(&leases()?).map_err(text)? != before {
            return Err("unconfirmed shutdown mutated lease".into());
        }
        if runtime_process::inspect(identity.pid, &lease.run_id).map_err(text)? != *identity {
            return Err("live-worker precondition no longer holds".into());
        }
        if owner.lock().unwrap().reaped || !PLAN.lock().unwrap().reaps.is_empty() {
            return Err("zero budget invented reap".into());
        }
    }
    // Exercise the same background admission used by native exports; Later is
    // opaque to Rust callers, so no JS/native-widget qualification is implied.
    if !live_guard && !shutdown() {
        return Err("idle unknown owner did not retire at shutdown".into());
    }
    let revision = input_revision()?;
    let mut future = std::pin::pin!(background(move || apply(
        binding,
        revision,
        "new forbidden input".into()
    )));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    use std::future::Future;
    let std::task::Poll::Ready(Ok(reply)) = future.as_mut().poll(&mut cx) else {
        return Err("latched business did not reject immediately".into());
    };
    let reply: Value = serde_json::from_str(&reply).map_err(text)?;
    if reply["code"] != "editor_shutting_down" {
        return Err("latch accepted new business".into());
    }
    // Unbounded retries are prohibited: one ordinary-budget actual retirement attempt.
    super::super::shutdown()?;
    if super::super::SERVICE.lock().map_err(text)?.is_some() {
        return Err("retired Service not closed".into());
    }
    {
        let slot = mutex(&EDITOR)?;
        if slot.as_ref().unwrap().pending_run.is_some() {
            return Err("shutdown pending owner not drained".into());
        }
    }
    let remaining = {
        let mut g = mutex(&held_service.gate)?;
        g.host
            .with_component_authority(|store, _| store.worker_leases().map_err(text))?
    };
    if !remaining.is_empty() {
        return Err("retired shutdown retained SQL lease".into());
    }
    if PLAN.lock().unwrap().reaps.len() != 1 {
        return Err("shutdown requires one actual owned Reaped".into());
    }
    println!(
        "NATIVE_RETIREMENT_CASE {}",
        json!({"case":label,"busy_before":false,"actual_reaped":PLAN.lock().unwrap().reaps,"leases_after":0,"service_closed_after_real_reap":true,"unconfirmed_service_and_lease_retained":live_guard,"new_business_rejected":true,"native_release_ack":false})
    );
    Ok(())
}
pub fn run_cases() -> Result<()> {
    for label in ["invocation", "gate", "finish", "unknown_wait"] {
        fault_case(label)?;
    }
    guarded_lease(false)?;
    guarded_lease(true)?;
    shutdown_case(false)?;
    shutdown_case(true)?;
    println!(
        "NATIVE_RETIREMENT_PASSED cases=8 budgets=250ms/1000000fuel/8calls backpressure=unverified native_release_ack=unverified"
    );
    Ok(())
}

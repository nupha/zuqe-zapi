// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

//! Per-runtime extension state, keyed by [`ZqRuntime::id`].
//!
//! An extension that keeps JS values in a `thread_local` (or a `static`) has to
//! answer three questions correctly, and getting any of them wrong is a
//! use-after-free rather than a compile error:
//!
//! 1. **Which runtime?** Two runtimes can share a thread, so state must be keyed
//!    by runtime identity, not by thread.
//! 2. **Who marks it?** Values held outside the GC heap must be reported on
//!    every cycle through one trace callback.
//! 3. **Who drops it?** The runtime-finalize hook is the only place that knows a
//!    runtime is going away.
//!
//! [`ZqRuntimeState`] owns all three: create the map, install the trace once per
//! runtime on first use, and tear the entry down in the finalize hook.
//!
//! Everything here stays on the JS thread, which is the only thread that may
//! touch JS values and the only thread a runtime's callbacks run on, so a state
//! slot is only ever touched from one thread. `Mutex` appears solely because a
//! `static` requires `Sync`; nothing here actually contends.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::{ZqCtx, ZqGcTrace, ZqGcTraceCtx, ZqRuntime};

/// One trace entry per live state slot on this thread, in registration order.
/// Fed by the single per-runtime `gc_set_trace_func` hook.
type Tracer = Box<dyn Fn(&mut ZqGcTraceCtx)>;

thread_local! {
    /// Registered state slots on this thread.
    static TRACERS: RefCell<Vec<Tracer>> = const { RefCell::new(Vec::new()) };
    /// Runtimes on this thread that already have a trace hook installed, so
    /// `with` stays cheap on the hot path.
    static TRACED: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// Per-runtime, GC-traced, finalize-cleaned extension state.
///
/// ```ignore
/// #[derive(Default)]
/// struct State {
///     callbacks: HashMap<String, ZqRawValue>,
/// }
///
/// static STATE: ZqRuntimeState<State> = ZqRuntimeState::new();
///
/// fn on_event(cx: ZqCtx, k: &str) {
///     let cb = STATE.with(cx, |st| st.callbacks.get(k).copied());
///     // ...
/// }
///
/// // in `#[zq_module(finalize = ...)]`
/// STATE.finalize(rt);
/// ```
///
/// `S: Default` so a runtime gets its state on first use rather than needing an
/// explicit constructor, and `S: ZqGcTrace` so the extension decides what to
/// mark — `ZqRawValue` marks itself and the primitive impls are no-ops, so a
/// struct of plain data needs only an empty `trace`.
pub struct ZqRuntimeState<S: Default + ZqGcTrace + 'static> {
    /// `None` until the first `with`; then this slot's `runtime id -> state` map.
    map: OnceLock<&'static Mutex<HashMap<usize, S>>>,
}

impl<S: Default + ZqGcTrace + 'static> ZqRuntimeState<S> {
    /// Declare a state slot. Nothing is allocated until the first
    /// [`with`](Self::with), so this works in a `static`.
    pub const fn new() -> Self {
        Self {
            map: OnceLock::new(),
        }
    }

    /// Run `f` against this runtime's state, creating it on first use.
    ///
    /// Also installs this runtime's GC trace hook the first time any state slot
    /// is used on this thread, so a caller may store JS values here without
    /// arranging its own tracing.
    ///
    /// `f` runs while the slot is locked, so it must not re-enter the same
    /// `ZqRuntimeState` and must not trigger a GC cycle — either would re-enter
    /// the trace trampoline or this method itself. Breaking that contract panics
    /// with a name rather than hanging; touching *other* slots is fine.
    pub fn with<R>(&self, cx: ZqCtx<'_>, f: impl FnOnce(&mut S) -> R) -> R {
        let map = *self.map.get_or_init(|| {
            // Leaked on purpose: the thread-local tracer below has to reach this
            // slot's map without owning it, and a module declares a fixed,
            // handful of slots.
            let map: &'static Mutex<HashMap<usize, S>> =
                Box::leak(Box::new(Mutex::new(HashMap::new())));
            let trace_map = map;
            // Registered before any value can be stored, so a GC in between
            // cannot miss it.
            TRACERS.with(|t| {
                t.borrow_mut().push(Box::new(move |gcx: &mut ZqGcTraceCtx| {
                    for state in trace_map.lock().unwrap().values() {
                        state.trace(gcx);
                    }
                }));
            });
            map
        });

        ensure_traced(cx);
        // 	ry_lock rather than lock: everything here is single-threaded, so
        // contention can only mean  re-entered this slot (directly, or via a
        // GC it triggered). Panic with a name beats hanging with no output.
        let mut guard = map.try_lock().unwrap_or_else(|_| {
            panic!("ZqRuntimeState::with: the same state slot is already borrowed")
        });
        f(guard.entry(cx.runtime().id()).or_default())
    }

    /// Whether any runtime still holds state in this slot. An extension can use
    /// the finalize hook's return-free call plus this to detect "the last
    /// runtime is gone" (e.g. to reap processes it spawned).
    pub fn is_empty(&self) -> bool {
        self.map
            .get()
            .is_none_or(|map| map.lock().unwrap().is_empty())
    }

    /// Drop this runtime's state. Call from `#[zq_module(finalize = ...)]`.
    ///
    /// Must not touch any JS API: the runtime is already tearing down, so a
    /// leftover `S` is dropped here rather than traced.
    pub fn finalize(&self, rt: *mut ZqRuntime) {
        let key = rt as usize;
        if let Some(map) = self.map.get() {
            map.lock().unwrap().remove(&key);
        }
        let _ = TRACED.try_with(|t| t.borrow_mut().retain(|k| *k != key));
    }
}

impl<S: Default + ZqGcTrace + 'static> Default for ZqRuntimeState<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Default + ZqGcTrace + 'static> std::fmt::Debug for ZqRuntimeState<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ZqRuntimeState")
    }
}

/// Install `gc_set_trace_func` for this runtime exactly once. A single hook
/// serves every state slot, since it walks `TRACERS`.
fn ensure_traced(cx: ZqCtx<'_>) {
    let key = cx.runtime().id();
    let already = TRACED.with(|t| {
        let mut t = t.borrow_mut();
        if t.contains(&key) {
            true
        } else {
            t.push(key);
            false
        }
    });
    if !already {
        let _ = cx.gc_set_trace_func(Some(trace_all), std::ptr::null_mut());
    }
}

/// GC trace trampoline: mark every runtime's state. Runs on the JS thread.
unsafe extern "C" fn trace_all(
    gcx: *mut crate::ffi::ZqGcTraceCtxS,
    _opaque: *mut std::ffi::c_void,
) {
    let mut gcx = ZqGcTraceCtx::from(gcx);
    // A GC trace must never fire during thread-local teardown; ignore it rather
    // than aborting the process if it somehow does.
    let _ = TRACERS.try_with(|t| {
        for trace in t.borrow().iter() {
            trace(&mut gcx);
        }
    });
}

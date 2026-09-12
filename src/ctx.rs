// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::{marker::PhantomData, os::raw::c_void};

use crate::{
    ZqError, ZqErrorType, ZqGcTraceCtx, ZqRawCtx, ZqValue,
    class::Class,
    ffi::{
        ZQ_CONST_VALUE_SLOT_EXCEPTION, ZqApi, ZqCallInfoS, ZqClassDef, ZqGcTraceCtxS, ZqJobRunFn,
        ZqPromiseCapabilityS, ZqRawValue, ZqWakeHandleS,
    },
};

/// Clonable, thread-safe handle to wake the JS event loop from any thread.
///
/// Create during a callback (`ctx.wake_handle()`) and store it in the
/// worker/async state that produces the result; call `wake()` after pushing
/// the result so a parked event loop re-ticks immediately. Without this, a
/// pure event-driven host (quantum poll off) would hang until some other
/// event happens to fire.
#[derive(Debug, Clone)]
pub struct ZqWakeHandle {
    raw: *mut ZqWakeHandleS,
    /// Captured at creation (on the JS thread): `ZqApi::singleton()` is
    /// thread-local, so `wake()` from worker threads must not touch it.
    wake_fn: unsafe extern "C" fn(*mut ZqWakeHandleS),
    free_fn: unsafe extern "C" fn(*mut ZqWakeHandleS),
}

unsafe impl Send for ZqWakeHandle {}
unsafe impl Sync for ZqWakeHandle {}

impl ZqWakeHandle {
    /// Wake the parked JS event loop. Cheap no-op when not parked.
    /// Thread-safe (uses captured fn pointers, no thread-local access).
    pub fn wake(&self) {
        unsafe { (self.wake_fn)(self.raw) };
    }
}

impl Drop for ZqWakeHandle {
    fn drop(&mut self) {
        unsafe { (self.free_fn)(self.raw) };
    }
}

#[derive(Debug)]
#[repr(transparent)]
pub struct ZqCtx<'cx> {
    pub(crate) raw: ZqRawCtx,
    _marker: PhantomData<&'cx ()>,
}

/// A PromiseCapability Record (ECMA-262 §27.5.1.1): a pending Promise plus its
/// `[[Resolve]]` / `[[Reject]]` functions. Settle the promise by calling
/// `resolve` / `reject` (e.g. `cap.resolve.call(cx, ZqValue::undefined(), &[v])`).
#[derive(Debug, Clone, Copy)]
pub struct ZqPromiseCapability<'cx> {
    /// `[[Promise]]` — the newly created pending Promise.
    pub promise: ZqValue<'cx>,
    /// `[[Resolve]]` — call it to fulfill the promise.
    pub resolve: ZqValue<'cx>,
    /// `[[Reject]]` — call it to reject the promise.
    pub reject: ZqValue<'cx>,
}

impl<'cx> Clone for ZqCtx<'cx> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}
impl<'cx> Copy for ZqCtx<'cx> {}

impl<'cx> From<ZqRawCtx> for ZqCtx<'cx> {
    #[inline(always)]
    fn from(raw: ZqRawCtx) -> Self {
        Self {
            raw,
            _marker: PhantomData,
        }
    }
}

impl<'cx> From<ZqCtx<'cx>> for ZqRawCtx {
    #[inline(always)]
    fn from(ctx: ZqCtx<'cx>) -> Self {
        ctx.raw
    }
}

impl<'cx> ZqCtx<'cx> {
    /// Capture a wake handle for this context's event loop. Call on the JS
    /// thread (inside a callback / job) and move the handle into whatever
    /// thread or task produces the async result.
    pub fn wake_handle(&self) -> ZqWakeHandle {
        let api = ZqApi::singleton();
        ZqWakeHandle {
            raw: unsafe { (api.new_waker)(self.raw) },
            wake_fn: api.waker_wake,
            free_fn: api.waker_free,
        }
    }

    /// Register a polling job: `job` runs once per event-loop tick until it
    /// returns [`ZqPoll::Done`]. Return [`ZqPoll::Again`] to stay registered
    /// (the loop parks until woken — pair with `make_wake_channel` /
    /// `spawn_promise` so the wake actually arrives). Return
    /// [`ZqPoll::AgainNoHold`] for fire-and-forget jobs that must not keep
    /// the process alive.
    ///
    /// NOTE: the closure must not capture GC-managed JS values (no tracing
    /// is provided for it). For jobs that need JS values in their state,
    /// keep them behind a native payload or use the raw FFI with a trace.
    pub fn add_polling_job<F>(&self, job: F)
    where
        F: FnMut(ZqCtx<'_>) -> ZqPoll + 'static,
    {
        let trampoline: ZqJobRunFn = zq_poll_trampoline::<F>;
        unsafe {
            (ZqApi::singleton().add_polling_job)(
                self.raw,
                trampoline,
                None,
                Box::into_raw(Box::new(job)).cast::<c_void>(),
            );
        }
    }

    pub fn new_string(&self, s: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let s = unsafe { (api.new_string)(self.raw, s.as_ptr(), s.len()) };
        if s == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(s))
        }
    }

    pub fn new_object(&self) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let v = unsafe { (api.ordinary_object_create)(self.raw) };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    pub fn new_array(&self, elements: &[ZqRawValue]) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let arr =
            unsafe { (api.create_array_from_list)(self.raw, elements.len(), elements.as_ptr()) };
        if arr == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(arr))
        }
    }

    /// Create a JS `Symbol(desc)` (equivalent to calling the `Symbol` global).
    pub fn new_symbol(&self, desc: &str) -> Result<ZqValue<'cx>, ZqError> {
        let ctor = self.get_global_str("Symbol")?;
        let d = self.new_string(desc)?;
        self.call_function(ctor, ZqValue::undefined(), &[d])
    }

    /// Create a BigInt from an `i64`.
    pub fn new_bigint_i64(&self, v: i64) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.new_bigint_i64)(self.raw, v) };
        if r == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(r))
        }
    }

    /// Create a BigInt from a `u64`.
    pub fn new_bigint_u64(&self, v: u64) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.new_bigint_u64)(self.raw, v) };
        if r == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(r))
        }
    }

    /// Create (but do not throw) an `Error` value of `err_type`.
    pub fn new_error(&self, err_type: ZqErrorType, msg: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.new_error)(self.raw, err_type as i32, msg.as_ptr(), msg.len()) };
        if r == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(r))
        }
    }

    /// `val instanceof class` (honours `Symbol.hasInstance`).
    pub fn is_instance_of<T>(
        &self,
        val: ZqValue<'cx>,
        class: &Class<'_, T>,
    ) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.instanceof_operator)(self.raw, val.raw(), class.def()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Look up a property on the global object (`globalThis[name]`).
    pub fn get_global_str(&self, name: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let v = unsafe { (api.get_global)(self.raw, name.as_ptr(), name.len()) };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Assign a property on the global object (`globalThis[name] = val`).
    /// Returns whether the assignment applied (false on a silent no-op).
    pub fn set_global_str<V: crate::traits::IntoZqValue<'cx>>(
        &self,
        name: &str,
        val: V,
    ) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let val = val.into_zq_value(*self);
        let r = unsafe { (api.set_global)(self.raw, name.as_ptr(), name.len(), val.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Calls a JS function with `this` and arguments, returning its result.
    ///
    /// The host hands every argument (numbers included) to extension callbacks
    /// as proper engine JSValues, so they are forwarded unchanged.
    pub fn call_function(
        &self,
        target: ZqValue<'cx>,
        this: ZqValue,
        args: &[ZqValue],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let raw_args: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let mut info = ZqCallInfoS {
            target: target.raw(),
            this: this.raw(),
            argv: raw_args.as_ptr(),
            argc: raw_args.len(),
            magic: 0,
        };
        let ret = unsafe { (api.call)(self.raw, &mut info) };
        if ret == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(ret))
        }
    }

    /// Invoke `target` as a constructor: `new target(...args)`.
    pub fn call_constructor(
        &self,
        target: ZqValue<'cx>,
        args: &[ZqValue],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let raw_args: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let mut info = ZqCallInfoS {
            target: target.raw(),
            this: 0,
            argv: raw_args.as_ptr(),
            argc: raw_args.len(),
            magic: 0,
        };
        let ret = unsafe { (api.construct)(self.raw, &mut info) };
        if ret == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(ret))
        }
    }

    /// Register a global GC trace callback for values the extension holds
    /// outside the GC heap (thread_locals, static natives). `cb` runs at every
    /// GC and should `mark()` every such value via `ZqGcTraceCtx`.
    ///
    /// Typically called from the module `init` hook.
    ///
    /// # Safety
    ///
    /// `opaque` must point to valid extension state for the lifetime of the
    /// registration, and `cb` must be a valid function pointer that can safely
    /// access that state. The callback must not panic or perform any operation
    /// that could invalidate the JS heap while tracing is active.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn gc_set_trace_func(
        &self,
        func: Option<crate::ffi::ZqGcTraceFn>,
        opaque: *mut std::ffi::c_void,
    ) -> Result<(), ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.gc_set_trace_func)(self.raw, func, opaque) };
        if r < 0 { Err(ZqError) } else { Ok(()) }
    }

    // ── class / instance helpers (requires GLOBAL_API) ──

    pub fn register_class(&self, def: &ZqClassDef) -> Result<i32, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.register_class)(self.raw, def) };
        if r < 0 { Err(ZqError) } else { Ok(r) }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn new_instance(
        &self,
        def: &ZqClassDef,
        payload: *mut std::ffi::c_void,
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.new_instance)(self.raw, def, payload) };
        if r == 0 || r == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(r))
        }
    }

    pub fn get_native_ptr(&self, val: ZqValue<'cx>) -> *mut std::ffi::c_void {
        let api = ZqApi::singleton();
        unsafe { (api.get_native_ptr)(self.raw, val.raw()) }
    }

    /// `NewPromiseCapability(%Promise%)`: create a pending Promise together with
    /// its `[[Resolve]]` / `[[Reject]]` functions.
    pub fn new_promise_capability(&self) -> Result<ZqPromiseCapability<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let mut cap = ZqPromiseCapabilityS {
            promise: 0,
            resolve: 0,
            reject: 0,
        };
        let r = unsafe { (api.new_promise_capability)(self.raw, &mut cap) };
        if r < 0 {
            Err(ZqError)
        } else {
            Ok(ZqPromiseCapability {
                promise: ZqValue::from(cap.promise),
                resolve: ZqValue::from(cap.resolve),
                reject: ZqValue::from(cap.reject),
            })
        }
    }

    /// High-level one-shot async: run `work` on a dedicated thread, deliver
    /// its result to `convert` (JS thread) and settle the returned Promise.
    ///
    /// * `work` — the blocking/long computation; must not touch JS values.
    /// * `convert` — runs on the JS thread; builds the JS value from the
    ///   result. Return Err to reject the Promise. If it reported a JS
    ///   exception (e.g. via `cx.throw_type(...)`), that exception becomes
    ///   the rejection reason and is consumed; otherwise the Promise rejects
    ///   with a generic "async work failed" reason.
    ///
    /// Wake correctness: the worker pushes its result via a [`ZqWakeSender`]
    /// (created by [`make_wake_channel`](Self::make_wake_channel)), so every
    /// send wakes the parked event loop — no lost-wakeup. The polling job's
    /// env is re-read from the current `cx` on **each tick** via [`ZqCtx::raw`],
    /// never captured across ticks. This avoids a dangling-context UB (a
    /// `ScopedContext` is transient across `with_new_scope` ticks).
    ///
    /// If the work thread finishes without delivering (drop), the Promise
    /// rejects with an internal error — forgetting to deliver cannot hang
    /// the event loop.
    pub fn spawn_promise<T, F, C>(&self, work: F, convert: C) -> Result<ZqValue<'cx>, ZqError>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
        C: for<'a> FnOnce(ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
    {
        let api = ZqApi::singleton();

        // NewPromiseCapability(%Promise%): the promise is rooted by the host
        // while pending; the resolving functions are kept alive by `spawn_trace`
        // (they are not reachable from the promise).
        let cap = self.new_promise_capability()?;
        let (tx, rx) = self.make_wake_channel::<T>();

        std::thread::spawn(move || {
            let t = work();
            let _ = tx.send(t); // WakingSender wakes the parked loop automatically.
        });

        let state = Box::new(SpawnState {
            rx,
            convert: Some(convert),
            promise: cap.promise.raw(),
            resolve: cap.resolve.raw(),
            reject: cap.reject.raw(),
        });
        unsafe {
            (api.add_polling_job)(
                self.raw,
                spawn_run::<T, C>,
                Some(spawn_trace::<T, C>),
                Box::into_raw(state).cast::<c_void>(),
            );
        }

        Ok(cap.promise)
    }
}

/// State for the [`ZqCtx::spawn_promise`] polling job: the channel, the convert
/// closure, and the capability's raw handles (so `spawn_trace` can keep the
/// resolving functions alive while the job is pending).
struct SpawnState<T, C> {
    rx: std::sync::mpsc::Receiver<T>,
    convert: Option<C>,
    promise: ZqRawValue,
    resolve: ZqRawValue,
    reject: ZqRawValue,
}

/// Polling-job body for `spawn_promise`: settle the capability's promise via its
/// resolving functions, then free `data` and report `Done`.
unsafe extern "C" fn spawn_run<T, C>(data: *mut c_void, raw_ctx: ZqRawCtx) -> i32
where
    T: 'static,
    C: for<'a> FnOnce(ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
{
    let state = unsafe { &mut *(data as *mut SpawnState<T, C>) };
    let cx = ZqCtx::from(raw_ctx);

    match state.rx.try_recv() {
        Ok(t) => {
            let convert = state
                .convert
                .take()
                .expect("spawn_promise: convert consumed twice");
            // context must be fresh — it's transient across ticks.
            match convert(cx, t) {
                Ok(v) => {
                    let _ =
                        cx.call_function(ZqValue::from(state.resolve), ZqValue::undefined(), &[v]);
                }
                Err(_) => {
                    // The convert step failed. If it reported a JS exception
                    // (e.g. via `ZqCtx::throw_*`), reject with that exception
                    // and consume it; otherwise use a generic reason.
                    let reason = cx.take_exception();
                    let reason = if reason.is_undefined() {
                        cx.new_string("async work failed").unwrap()
                    } else {
                        reason
                    };
                    let _ = cx.call_function(
                        ZqValue::from(state.reject),
                        ZqValue::undefined(),
                        &[reason],
                    );
                }
            }
            drop(unsafe { Box::from_raw(data as *mut SpawnState<T, C>) });
            0 // Done
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => 1, // Block
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            // Worker ended without delivering — deterministic reject.
            state.convert.take();
            let reason = cx
                .new_string("async work dropped without a result")
                .unwrap();
            let _ = cx.call_function(ZqValue::from(state.reject), ZqValue::undefined(), &[reason]);
            drop(unsafe { Box::from_raw(data as *mut SpawnState<T, C>) });
            0 // Done
        }
    }
}

/// GC trace for the `spawn_promise` job: mark the capability's promise and
/// resolving functions so they survive until the job settles it.
unsafe extern "C" fn spawn_trace<T, C>(data: *mut c_void, gcx: *mut ZqGcTraceCtxS) {
    let state = unsafe { &*(data as *const SpawnState<T, C>) };
    let ctx = ZqGcTraceCtx::from(gcx);
    let _ = ctx.mark(state.promise);
    let _ = ctx.mark(state.resolve);
    let _ = ctx.mark(state.reject);
}

/// Return value of a [`ZqCtx::add_polling_job`] closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZqPoll {
    /// Job finished — the job is released.
    Done,
    /// Stay registered; run again on the next tick (the loop parks until
    /// a wake arrives).
    Again,
    /// Like `Again`, but does not keep the process alive.
    AgainNoHold,
}

/// Trampoline: host calls this per tick with the boxed closure.
unsafe extern "C" fn zq_poll_trampoline<F>(data: *mut c_void, raw_ctx: ZqRawCtx) -> i32
where
    F: FnMut(ZqCtx<'_>) -> ZqPoll + 'static,
{
    let job = unsafe { &mut *(data as *mut F) };
    let ctx = ZqCtx::from(raw_ctx);

    match job(ctx) {
        ZqPoll::Done => {
            // Done: the closure is consumed — free it (host does not free data).
            drop(unsafe { Box::from_raw(data as *mut F) });
            0
        }
        ZqPoll::Again => 1,
        ZqPoll::AgainNoHold => 2,
    }
}

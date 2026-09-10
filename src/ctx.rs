// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::{marker::PhantomData, os::raw::c_void};

use crate::{
    ZqError, ZqRawCtx, ZqValue,
    ffi::{ZqApi, ZqCallInfoS, ZqClassDef, ZqJobRunFn, ZqRawValue, ZqWakeHandleS},
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
            raw: unsafe { (api.wake_handle_new)(self.raw) },
            wake_fn: api.wake_handle_wake,
            free_fn: api.wake_handle_free,
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
        let s = unsafe { (api.new_string_utf8)(self.raw, s.as_ptr(), s.len()) };
        if s == api.value_exception {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(s))
        }
    }

    pub fn new_object(&self) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let v = unsafe { (api.new_object)(self.raw) };
        if v == api.value_exception {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    pub fn new_array(&self, elements: &[ZqRawValue]) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let arr = unsafe { (api.new_array)(self.raw, elements.as_ptr(), elements.len()) };
        if arr == api.value_exception {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(arr))
        }
    }

    /// Create a JS `Symbol(desc)`.
    pub fn new_symbol(&self, desc: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let d = self.new_string(desc)?;
        let v = unsafe { (api.new_symbol)(self.raw, d.raw()) };
        if v == api.value_exception {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Read a JS value as `f64` (ToNumber).
    pub fn value_to_float64(&self, val: ZqValue<'cx>) -> f64 {
        let api = ZqApi::singleton();
        unsafe { (api.value_to_float64)(self.raw, val.raw()) }
    }

    /// Read a JS value as bool (ToBoolean).
    pub fn value_to_bool(&self, val: ZqValue<'cx>) -> bool {
        let api = ZqApi::singleton();
        unsafe { (api.value_to_bool)(self.raw, val.raw()) != 0 }
    }

    /// Read the UTF-8 contents of a JS string value.
    pub fn read_string(&self, val: ZqRawValue) -> String {
        let api = ZqApi::singleton();
        // Measure-then-copy: a null buf returns the byte count; -1 = not a string.
        let len = unsafe { (api.get_string_utf8)(self.raw, val, std::ptr::null_mut(), 0, 0) };
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u8; len as usize];
        unsafe { (api.get_string_utf8)(self.raw, val, buf.as_mut_ptr(), len as usize, 0) };
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// Look up a property on the global object (`globalThis[name]`).
    pub fn get_global(&self, name: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let key = self.new_string(name)?;
        Ok(ZqValue::from(unsafe {
            (api.get_global)(self.raw, key.raw())
        }))
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
            args: raw_args.as_ptr(),
            argc: raw_args.len() as u32,
            magic: 0,
        };
        let ret = unsafe { (api.call_constructor)(self.raw, &mut info) };
        if ret == api.value_exception {
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
    pub fn set_gc_trace_func(
        &self,
        func: Option<crate::ffi::ZqGcTraceFn>,
        opaque: *mut std::ffi::c_void,
    ) -> Result<(), ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.set_gc_trace_func)(self.raw, func, opaque) };
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
        opaque: *mut std::ffi::c_void,
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.new_instance)(self.raw, def, opaque) };
        if r == 0 || r == api.value_exception {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(r))
        }
    }

    pub fn get_native_ptr(&self, val: ZqValue<'cx>) -> *mut std::ffi::c_void {
        let api = ZqApi::singleton();
        unsafe { (api.get_native_ptr)(self.raw, val.raw()) }
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
            args: raw_args.as_ptr(),
            argc: raw_args.len() as u32,
            magic: 0,
        };
        let ret = unsafe { (api.call_function)(self.raw, &mut info) };
        if ret == api.value_exception {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(ret))
        }
    }

    /// High-level one-shot async: run `work` on a dedicated thread, deliver
    /// its result to `convert` (JS thread) and settle the returned Promise.
    ///
    /// * `work` — the blocking/long computation; must not touch JS values.
    /// * `convert` — runs on the JS thread; builds the JS value from the
    ///   result. Return Err to reject the Promise.
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

        let promise = unsafe { (api.new_promise)(self.raw) };
        let (tx, rx) = self.make_wake_channel::<T>();

        std::thread::spawn(move || {
            let t = work();
            let _ = tx.send(t); // WakingSender wakes the parked loop automatically.
        });

        let resolve_promise = api.promise_resolve;
        let reject_promise = api.promise_reject;
        let mut convert = Some(convert);

        self.add_polling_job(move |cx| match rx.try_recv() {
            Ok(t) => {
                let c = convert
                    .take()
                    .expect("spawn_promise: convert consumed twice");

                // context must be fresh — it's transient across ticks.
                match c(cx, t) {
                    Ok(v) => unsafe { resolve_promise(cx.raw, promise, v.raw()) },
                    Err(_) => {
                        let reason = cx.new_string("async work failed").unwrap();
                        unsafe { reject_promise(cx.raw, promise, reason.raw()) };
                    }
                }
                ZqPoll::Done
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ZqPoll::Again,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Worker ended without delivering — deterministic reject.
                convert.take();

                let reason = cx
                    .new_string("async work dropped without a result")
                    .unwrap();
                unsafe { reject_promise(cx.raw, promise, reason.raw()) };
                ZqPoll::Done
            }
        });

        Ok(ZqValue::from(promise))
    }
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

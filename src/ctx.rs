// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::{marker::PhantomData, os::raw::c_void};

use crate::{
    __private::zapi_singleton,
    ZqEnv, ZqError, ZqStatus, ZqType, ZqValue,
    ffi::{ZqCallbackInfoS, ZqClassDef, ZqEnvS, ZqJobRunFn, ZqRawValue, ZqWakeHandleS},
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
    /// Captured at creation (on the JS thread): `zapi_singleton()` is
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
    pub(crate) env: *mut ZqEnvS,
    _marker: PhantomData<&'cx ()>,
}

impl<'cx> Clone for ZqCtx<'cx> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}
impl<'cx> Copy for ZqCtx<'cx> {}

/// Trampoline for `new_async_iterator` fetch callback.
/// Uses `&mut` reference — the callback is `FnMut` (called once per row).
/// The Box is freed by `zapi_fetch_drop` when the iterator is GC'd.
unsafe extern "C" fn zapi_fetch_trampoline<F>(env: ZqEnv, func_ptr: *mut c_void) -> ZqRawValue
where
    F: for<'a> FnMut(&mut ZqCtx<'a>) -> Result<Option<ZqValue<'a>>, ZqError> + 'static,
{
    let fetch_fn = &mut *(func_ptr as *mut F);
    let mut ctx = ZqCtx::from_raw(env);
    match fetch_fn(&mut ctx) {
        Ok(Some(val)) => val.raw(),
        Ok(None) => ctx.undefined().raw(),
        Err(e) => {
            let _ = ctx.throw_error(&e);
            zapi_singleton().value_exception
        }
    }
}

/// Drop function for the fetch callback Box stored as opaque data.
unsafe extern "C" fn zapi_fetch_drop<F>(data: *mut c_void) {
    unsafe {
        drop(Box::from_raw(data as *mut F));
    }
}

impl<'cx> ZqCtx<'cx> {
    #[inline(always)]
    pub fn from_raw(env: ZqEnv) -> Self {
        Self {
            env,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    pub const fn raw(&self) -> ZqEnv {
        self.env
    }

    /// Capture a wake handle for this context's event loop. Call on the JS
    /// thread (inside a callback / job) and move the handle into whatever
    /// thread or task produces the async result.
    pub fn wake_handle(&self) -> ZqWakeHandle {
        let api = zapi_singleton();
        ZqWakeHandle {
            raw: unsafe { (api.wake_handle_new)(self.env) },
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
        F: FnMut(&mut ZqCtx<'_>) -> ZqPoll + 'static,
    {
        let trampoline: ZqJobRunFn = zq_poll_trampoline::<F>;
        unsafe {
            (zapi_singleton().add_polling_job)(
                self.env,
                trampoline,
                None,
                Box::into_raw(Box::new(job)).cast::<c_void>(),
            );
        }
    }

    pub fn string(&self, s: &str) -> ZqValue<'cx> {
        let api = zapi_singleton();
        unsafe { ZqValue::from((api.new_string_utf8)(self.env, s.as_ptr(), s.len())) }
    }

    pub fn number(&self, n: f64) -> ZqValue<'cx> {
        let api = zapi_singleton();
        // Common fixed numbers come straight from the table constants
        // (identical handles to `new_number`, minus the FFI call).
        // Note `-0.0` is a distinct float and must fall through to
        // `new_number` to keep its sign.
        let num = if n.is_nan() {
            api.value_nan
        } else if n == 1.0 {
            api.value_one
        } else if n == -1.0 {
            api.value_negative_one
        } else if n == 0.0 && n.is_sign_positive() {
            api.value_zero
        } else if n.is_infinite() {
            if n.is_sign_positive() {
                api.value_infinity
            } else {
                api.value_minus_infinity
            }
        } else {
            unsafe { (api.new_number)(self.env, n) }
        };

        ZqValue::from(num)
    }

    pub fn bool(&self, b: bool) -> ZqValue<'cx> {
        let api = zapi_singleton();
        ZqValue::from(if b { api.value_true } else { api.value_false })
    }

    pub fn undefined(&self) -> ZqValue<'cx> {
        let api = zapi_singleton();
        ZqValue::from(api.value_undefined)
    }

    pub fn null(&self) -> ZqValue<'cx> {
        let api = zapi_singleton();
        ZqValue::from(api.value_null)
    }

    pub fn object(&self) -> ZqValue<'cx> {
        let api = zapi_singleton();
        ZqValue::from(unsafe { (api.new_object)(self.env) })
    }

    pub fn array(&self, vals: &[ZqValue<'cx>]) -> ZqValue<'cx> {
        let api = zapi_singleton();
        let arr = unsafe { (api.new_array)(self.env, vals.len()) };
        for (i, v) in vals.iter().enumerate() {
            let key = self.number(i as f64);
            unsafe { (api.array_set)(self.env, arr, key.raw(), v.raw()) };
        }
        ZqValue::from(arr)
    }

    /// Create a JS `Symbol(desc)`.
    pub fn symbol(&self, desc: &str) -> ZqValue<'cx> {
        let api = zapi_singleton();
        let d = self.string(desc);
        ZqValue::from(unsafe { (api.new_symbol)(self.env, d.raw()) })
    }

    /// The type of a JS value (`ZqType`).
    pub fn get_value_type(&self, val: ZqValue<'cx>) -> ZqType {
        let api = zapi_singleton();
        unsafe { (api.get_value_type)(self.env, val.raw()) }
    }

    /// Read a JS value as `f64` (ToNumber).
    pub fn value_to_float64(&self, val: ZqValue<'cx>) -> f64 {
        let api = zapi_singleton();
        unsafe { (api.value_to_float64)(self.env, val.raw()) }
    }

    /// Read a JS value as bool (ToBoolean).
    pub fn value_to_bool(&self, val: ZqValue<'cx>) -> bool {
        let api = zapi_singleton();
        unsafe { (api.value_to_bool)(self.env, val.raw()) != 0 }
    }

    /// Read the UTF-8 contents of a JS string value.
    pub fn read_string(&self, val: ZqRawValue) -> String {
        let api = zapi_singleton();
        // Measure-then-copy: a null buf returns the byte count; -1 = not a string.
        let len = unsafe { (api.get_string_utf8)(self.env, val, std::ptr::null_mut(), 0, 0) };
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u8; len as usize];
        unsafe { (api.get_string_utf8)(self.env, val, buf.as_mut_ptr(), len as usize, 0) };
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// Look up a property on the global object (`globalThis[name]`).
    pub fn get_global(&self, name: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = zapi_singleton();
        let key = self.string(name);
        Ok(ZqValue::from(unsafe {
            (api.get_global)(self.env, key.raw())
        }))
    }

    /// Invoke `target` as a constructor: `new target(...args)`.
    pub fn call_constructor(
        &self,
        target: ZqValue<'cx>,
        args: &[ZqValue<'cx>],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = zapi_singleton();
        let raw_args: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let mut info = ZqCallbackInfoS {
            target: target.raw(),
            this: 0,
            args: raw_args.as_ptr(),
            argc: raw_args.len() as u32,
            thrown: 0,
            magic: 0,
        };
        let ret = unsafe { (api.call_constructor)(self.env, &mut info) };
        if ret == api.value_exception {
            Err(ZqError::new(
                ZqStatus::Exception,
                Some("call_constructor threw".into()),
            ))
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
        let api = zapi_singleton();
        let r = unsafe { (api.set_gc_trace_func)(self.env, func, opaque) };
        if r < 0 {
            Err(ZqError::new(
                ZqStatus::Error,
                Some("set_gc_trace_func failed".into()),
            ))
        } else {
            Ok(())
        }
    }

    #[deprecated]
    pub fn rt(&self) -> *mut std::ffi::c_void {
        if self.env.is_null() {
            return std::ptr::null_mut();
        }
        unsafe { (*self.env).rt }
    }

    // ── class / instance helpers (requires GLOBAL_API) ──

    pub fn register_class(&self, def: &ZqClassDef) -> Result<i32, ZqError> {
        let api = zapi_singleton();
        let r = unsafe { (api.register_class)(self.env, def) };
        if r < 0 {
            Err(ZqError::new(
                ZqStatus::Error,
                Some("register_class failed".into()),
            ))
        } else {
            Ok(r)
        }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn new_instance(
        &self,
        def: &ZqClassDef,
        opaque: *mut std::ffi::c_void,
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = zapi_singleton();
        let r = unsafe { (api.new_instance)(self.env, def, opaque) };
        if r == 0 || r == api.value_exception {
            Err(ZqError::new(
                ZqStatus::Exception,
                Some("new_instance failed: class may not be registered or creation error".into()),
            ))
        } else {
            Ok(ZqValue::from(r))
        }
    }

    pub fn get_native_ptr(&self, val: ZqValue<'cx>) -> *mut std::ffi::c_void {
        let api = zapi_singleton();
        unsafe { (api.get_native_ptr)(self.env, val.raw()) }
    }

    /// Calls a JS function with `this` and arguments, returning its result.
    ///
    /// The host hands every argument (numbers included) to extension callbacks
    /// as proper engine JSValues, so they are forwarded unchanged.
    pub fn call_function(
        &self,
        target: ZqValue<'cx>,
        this: ZqValue<'cx>,
        args: &[ZqValue<'cx>],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = zapi_singleton();
        let raw_args: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let mut info = ZqCallbackInfoS {
            target: target.raw(),
            this: this.raw(),
            args: raw_args.as_ptr(),
            argc: raw_args.len() as u32,
            thrown: 0,
            magic: 0,
        };
        let ret = unsafe { (api.call_function)(self.env, &mut info) };
        if ret == api.value_exception {
            Err(ZqError::new(
                ZqStatus::Exception,
                Some("call_function threw".into()),
            ))
        } else {
            Ok(ZqValue::from(ret))
        }
    }

    /// Create a JS error object from a `ZqError` and return its raw handle.
    /// Used by macro trampolines to throw proper JS errors when `ZqError`
    /// carries a message but no pre-built exception handle.
    pub fn throw_error(&self, err: &ZqError) -> ZqRawValue {
        let name = match err.status() {
            ZqStatus::TypeError => "TypeError",
            ZqStatus::RangeError => "RangeError",
            _ => "Error",
        };
        let msg = err.message().unwrap_or("");

        if let Ok(ctor) = self.get_global(name) {
            let msg_val = self.string(msg);
            if let Ok(obj) = self.call_constructor(ctor, &[msg_val]) {
                return obj.raw();
            }
        }

        // Fallback: return exception sentinel
        zapi_singleton().value_exception
    }

    /// Peek the pending JS exception without consuming it.
    /// Returns the exception value, or `undefined` if none is pending.
    pub fn exception(&self) -> ZqValue<'cx> {
        let api = zapi_singleton();
        ZqValue::from(unsafe { (api.exception)(self.env) })
    }

    /// Take the pending JS exception, clearing `cur_exception` and returning
    /// the exception value (GC-rooted until scope ends).
    /// Returns `undefined` if no exception was pending.
    pub fn take_exception(&self) -> ZqValue<'cx> {
        let api = zapi_singleton();
        ZqValue::from(unsafe { (api.take_exception)(self.env) })
    }

    /// Create an async iterable iterator backed by a Rust fetch callback.
    ///
    /// Returns a proper `AsyncIterableIterator` that implements the async
    /// iterator protocol (`for await...of`). The `fetch_fn` callback is
    /// invoked on the JS thread to produce each value.
    ///
    /// Callback convention (matches `AsyncIteratorFetcher`):
    /// - Return a normal value → yield `{value, done: false}`
    /// - Return `value_undefined` → done (iteration complete)
    /// - Return `value_exception` → reject the promise
    pub fn new_async_iterator<F>(&self, fetch_fn: F) -> Result<ZqValue<'cx>, ZqError>
    where
        F: for<'a> FnMut(&mut ZqCtx<'a>) -> Result<Option<ZqValue<'a>>, ZqError> + 'static,
    {
        let api = zapi_singleton();

        // We need to store the closure in a way that the FFI callback can access it.
        // Store it as a Box<F> cast to opaque pointer, same pattern as add_polling_job.
        let boxed = Box::new(fetch_fn);
        let data_ptr = Box::into_raw(boxed) as *mut c_void;

        let raw = unsafe {
            (api.new_async_iterator)(
                self.env,
                zapi_fetch_trampoline::<F>,
                None,                       // trace_fn: the closure doesn't hold JS values
                Some(zapi_fetch_drop::<F>), // drop_fn: free the Box when iterator is GC'd
                data_ptr,
            )
        };
        if raw == api.value_exception {
            Err(ZqError::type_error("new_async_iterator failed"))
        } else {
            Ok(ZqValue::from(raw))
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
        C: for<'a> FnOnce(&mut ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
    {
        let api = zapi_singleton();

        let promise = unsafe { (api.new_promise)(self.env) };
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
                // env must be fresh — the context is transient across ticks.
                let env = cx.raw();
                match c(cx, t) {
                    Ok(v) => unsafe { resolve_promise(env, promise, v.raw()) },
                    Err(e) => {
                        let reason = cx.string(&e.to_string());
                        unsafe { reject_promise(env, promise, reason.raw()) };
                    }
                }
                ZqPoll::Done
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ZqPoll::Again,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                // Worker ended without delivering — deterministic reject.
                convert.take();
                let env = cx.raw();
                let reason = cx.string("async work dropped without a result");
                unsafe { reject_promise(env, promise, reason.raw()) };
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
unsafe extern "C" fn zq_poll_trampoline<F>(data: *mut c_void, env: ZqEnv) -> i32
where
    F: FnMut(&mut ZqCtx<'_>) -> ZqPoll + 'static,
{
    let job = unsafe { &mut *(data as *mut F) };
    let mut ctx = ZqCtx::from_raw(env);

    match job(&mut ctx) {
        ZqPoll::Done => {
            // Done: the closure is consumed — free it (host does not free data).
            drop(unsafe { Box::from_raw(data as *mut F) });
            0
        }
        ZqPoll::Again => 1,
        ZqPoll::AgainNoHold => 2,
    }
}

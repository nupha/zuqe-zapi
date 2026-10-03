// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::{ffi::c_void, future::Future, pin::Pin, sync::OnceLock};

use crate::{
    ZqError, ZqGcTraceCtx, ZqValue,
    channel::ZqWakeSender,
    ffi::{ZqApi, ZqGcTraceCtxS, ZqPromiseCapabilityS, ZqRawCtx, ZqRawValue},
};

use super::ZqCtx;

/// Submits a boxed future to the extension's async executor.
///
/// `zuqe-zapi` is deliberately runtime-agnostic, so the *shape* is
/// caller-supplied; [`ZqCtx::set_default_executor`] lets an extension name its
/// executor once (module init) instead of threading it through every call.
pub type ZqExecutor = fn(Pin<Box<dyn Future<Output = ()> + Send + 'static>>);

/// The executor registered by [`set_default_executor`], if any.
static DEFAULT_EXECUTOR: OnceLock<ZqExecutor> = OnceLock::new();

/// Naming: the root says what the caller gets, the suffix says how the work runs.
///
/// | root | JS receives | work is |
/// |---|---|---|
/// | `promise_threaded*` | a Promise | a blocking `FnOnce` on a fresh OS thread |
/// | `promise_fut*` | a Promise | a `Future` driven by an async executor |
/// | `spawn_*` (channel / event jobs) | *nothing* — a native handle | a long-lived job on the event loop |
///
/// The `promise_` prefix is the JS-side contract, so it never mixes into the
/// `spawn_*` family: those return a `ZqWakeSender`, and calling that a promise
/// would be a lie. (`new_async_iterator` is the one exception — it also hands
/// JS a deferred object, but is an `AsyncIterable` rather than a Promise.)
///
/// Within `promise_fut*`, two optional suffixes in this fixed order:
///
/// | suffix | meaning |
/// |---|---|
/// | `_result` | the work yields `Result<T, E>`; `Err` rejects the Promise |
/// | `_on` | the executor is passed per call instead of taken from [`ZqCtx::set_default_executor`] |
///
/// So `promise_fut_void` is "future, registered executor, resolves to
/// `undefined`" and `promise_fut_result_on` is "future, explicit executor, work
/// may fail". A bare `promise_fut*` always means the registered executor, which
/// keeps the common path shortest.
impl<'cx> ZqCtx<'cx> {
    /// Name this extension's async executor, once, at module init.
    ///
    /// Idempotent by intent but *first call wins*: a second registration is
    /// ignored (reported under `debug_assertions`), because silently swapping
    /// the executor under in-flight promises would strand them.
    ///
    /// The handle is a plain `fn` pointer rather than a closure, since the
    /// executor has no per-call state — an extension that owns a runtime keeps
    /// it in a `static` and submits from there.
    pub fn set_default_executor(&self, executor: ZqExecutor) {
        let _ = DEFAULT_EXECUTOR.set(executor);
        debug_assert!(
            DEFAULT_EXECUTOR
                .get()
                .is_some_and(|e| std::ptr::eq(*e as *const (), executor as *const ())),
            "set_default_executor: an executor is already registered; the first one wins"
        );
    }

    /// The registered executor, or an internal error when none was registered.
    fn default_executor(&self) -> Result<ZqExecutor, ZqError> {
        DEFAULT_EXECUTOR.get().copied().ok_or_else(|| {
            self.throw_internal(
                "no default executor: call ctx.set_default_executor() at module init",
            )
        })
    }
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

impl<'cx> ZqCtx<'cx> {
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

    /// Run `work` on a dedicated blocked thread, deliver its result to
    /// `convert` (JS thread) and settle the returned Promise.
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
    /// env is re-read from the current `cx` on **each tick** (never captured
    /// across ticks), which avoids a dangling-context UB (a `ScopedContext` is
    /// transient across `with_new_scope` ticks).
    ///
    /// If the work thread finishes without delivering (drop), the Promise
    /// rejects with an internal error — forgetting to deliver cannot hang
    /// the event loop.
    ///
    /// For async runtimes that own an executor (e.g. tokio), prefer
    /// [`promise_fut`](Self::promise_fut), which runs the work on that executor
    /// instead of spawning one OS thread per call.
    pub fn promise_threaded<T, F, C>(&self, work: F, convert: C) -> Result<ZqValue<'cx>, ZqError>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
        C: for<'a> FnOnce(ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
    {
        self.settle_promise(
            move |tx| {
                std::thread::spawn(move || {
                    let t = work();
                    let _ = tx.send(t); // WakingSender wakes the parked loop automatically.
                });
            },
            convert,
        )
    }

    /// `promise_threaded` resolving to `undefined`.
    pub fn promise_threaded_void<F>(&self, work: F) -> Result<ZqValue<'cx>, ZqError>
    where
        F: FnOnce() + Send + 'static,
    {
        self.promise_threaded(work, |_cx, ()| Ok(ZqValue::undefined()))
    }

    /// Run an async [`Future`] on the executor registered by
    /// [`set_default_executor`](Self::set_default_executor) — no OS thread and
    /// no executor argument. The common shape for async work.
    ///
    /// The work cannot fail; if `convert` reports a JS exception, that
    /// exception rejects the Promise.
    pub fn promise_fut<T, F, C>(&self, fut: F, convert: C) -> Result<ZqValue<'cx>, ZqError>
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
        C: for<'a> FnOnce(ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
    {
        self.promise_fut_on(fut, self.default_executor()?, convert)
    }

    /// Like [`promise_fut`](Self::promise_fut), but on a caller-supplied executor
    /// via `spawn` — no OS thread is created per call, and the executor is named
    /// per call rather than registered once. This keeps `zuqe-zapi`
    /// runtime-agnostic: pass, for example, `|fut| { tokio_handle.spawn(fut); }`
    /// to run the work on an existing tokio runtime.
    ///
    /// `spawn` receives an erased `Send` future and is expected to drive it to
    /// completion. Completion is delivered to `convert` on the JS thread and
    /// settles the returned Promise, exactly as in [`promise_threaded`](Self::promise_threaded).
    ///
    /// Reach for [`promise_fut`](Self::promise_fut) instead when the extension has
    /// called `set_default_executor` — the argument is noise there.
    pub fn promise_fut_on<T, F, S, C>(
        &self,
        fut: F,
        spawn_fn: S,
        convert_fn: C,
    ) -> Result<ZqValue<'cx>, ZqError>
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
        S: FnOnce(Pin<Box<dyn Future<Output = ()> + Send + 'static>>),
        C: for<'a> FnOnce(ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
    {
        self.settle_promise(
            move |tx| {
                spawn_fn(Box::pin(async move {
                    let t = fut.await;
                    let _ = tx.send(t); // WakingSender wakes the parked loop automatically.
                }));
            },
            convert_fn,
        )
    }

    /// Like [`promise_fut_on`](Self::promise_fut_on), but the work is an async
    /// `Result<T, E>` and a failed result rejects the Promise with `E`'s
    /// `Display` rendering.
    ///
    /// This is the type-safe spelling for the common async-IO shape. Pinning
    /// `E` here avoids the inference collapse that plagues a bare
    /// `Future<Output = Result<T, E>>` handed to `promise_fut_on` (where `E` never
    /// appears in `convert`, so the compiler cannot deduce it).
    ///
    /// Rejection prefers a JS exception set by `convert` (via `cx.throw_*`);
    /// otherwise it uses `E`. A dropped worker rejects with an internal error.
    pub fn promise_fut_result_on<T, E, F, S, C>(
        &self,
        fut: F,
        spawn_fn: S,
        convert_fn: C,
    ) -> Result<ZqValue<'cx>, ZqError>
    where
        T: Send + 'static,
        E: std::fmt::Display + Send + 'static,
        F: Future<Output = Result<T, E>> + Send + 'static,
        S: FnOnce(Pin<Box<dyn Future<Output = ()> + Send + 'static>>),
        C: for<'a> FnOnce(ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
    {
        self.settle_promise(
            move |tx| {
                spawn_fn(Box::pin(async move {
                    let t = fut.await;
                    // Convert the error to a String before crossing back so the
                    // job only ever carries `Result<T, String>`.
                    let _ = tx.send(t.map_err(|e| e.to_string()));
                }));
            },
            move |cx, r: Result<T, String>| match r {
                Ok(v) => convert_fn(cx, v),
                Err(e) => Err(cx.throw_internal(&e)),
            },
        )
    }

    /// [`promise_fut_result_on`](Self::promise_fut_result_on) using the executor
    /// registered by [`set_default_executor`](Self::set_default_executor)
    /// instead of taking one per call.
    ///
    /// The error is pinned to `String` on purpose: a bare
    /// `Future<Output = Result<T, E>>` leaves `E` unconstrained here (it never
    /// appears in `convert_fn`), so a generic version could not infer its own
    /// type argument. Reach for `promise_fut_result_on` when you need a typed
    /// error.
    pub fn promise_fut_result<T, F, C>(
        &self,
        fut: F,
        convert_fn: C,
    ) -> Result<ZqValue<'cx>, ZqError>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, String>> + Send + 'static,
        C: for<'a> FnOnce(ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
    {
        self.promise_fut_result_on(fut, self.default_executor()?, convert_fn)
    }

    /// `promise_fut_result` resolving to `undefined`.
    pub fn promise_fut_void<F>(&self, fut: F) -> Result<ZqValue<'cx>, ZqError>
    where
        F: Future<Output = Result<(), String>> + Send + 'static,
    {
        self.promise_fut_result(fut, |_cx, ()| Ok(ZqValue::undefined()))
    }

    /// `promise_fut_result` resolving to a JS string.
    pub fn promise_fut_string<F>(&self, fut: F) -> Result<ZqValue<'cx>, ZqError>
    where
        F: Future<Output = Result<String, String>> + Send + 'static,
    {
        self.promise_fut_result(fut, |cx, s| ZqValue::new_string(cx, &s))
    }

    /// `promise_fut_result` resolving to a `serde_json::Value` built into a JS
    /// value through `JSON.parse` (feature `serde_json_support`).
    #[cfg(feature = "serde_json_support")]
    pub fn promise_fut_json<F>(&self, fut: F) -> Result<ZqValue<'cx>, ZqError>
    where
        F: Future<Output = Result<serde_json::Value, String>> + Send + 'static,
    {
        self.promise_fut_result(fut, |cx, v| ZqValue::from_json(cx, &v))
    }

    /// Shared plumbing for every spawning method above: create the capability +
    /// wake channel, hand the sender to `launch` (which starts the work), and
    /// register the polling job that settles the Promise.
    fn settle_promise<T, L, C>(&self, launch: L, convert: C) -> Result<ZqValue<'cx>, ZqError>
    where
        T: Send + 'static,
        L: FnOnce(ZqWakeSender<T>),
        C: for<'a> FnOnce(ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
    {
        let api = ZqApi::singleton();

        // NewPromiseCapability(%Promise%): the promise is rooted by the host
        // while pending; the resolving functions are kept alive by `spawn_trace`
        // (they are not reachable from the promise).
        let cap = self.new_promise_capability()?;
        let (tx, rx) = self.make_wake_channel::<T>();

        let state = Box::new(PromiseState {
            rx,
            convert: Some(convert),
            promise: cap.promise.raw(),
            resolve: cap.resolve.raw(),
            reject: cap.reject.raw(),
        });
        launch(tx);
        unsafe {
            (api.add_job)(
                self.raw,
                promise_run::<T, C>,
                Some(promise_trace::<T, C>),
                promise_drop::<T, C>,
                Box::into_raw(state).cast::<c_void>(),
            );
        }

        Ok(cap.promise)
    }
}

/// State for the promise polling job: the
/// channel, the convert closure, and the capability's raw handles (so
/// `spawn_trace` can keep the resolving functions alive while the job is
/// pending).
struct PromiseState<T, C> {
    rx: std::sync::mpsc::Receiver<T>,
    convert: Option<C>,
    promise: ZqRawValue,
    resolve: ZqRawValue,
    reject: ZqRawValue,
}

impl<T, C> PromiseState<T, C> {
    fn gc_trace(&self, ctx: ZqGcTraceCtx) {
        let _ = ctx.mark(self.promise);
        let _ = ctx.mark(self.resolve);
        let _ = ctx.mark(self.reject);
    }
}

/// Polling-job body: settle the capability's promise via
/// its resolving functions, then free `data` and report `Done`.
unsafe extern "C" fn promise_run<T, C>(data: *mut c_void, raw_ctx: ZqRawCtx) -> i32
where
    T: 'static,
    C: for<'a> FnOnce(ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
{
    let state = unsafe { &mut *(data as *mut PromiseState<T, C>) };
    let cx = ZqCtx::from(raw_ctx);

    match state.rx.try_recv() {
        Ok(t) => {
            let convert = state
                .convert
                .take()
                .expect("promise job: convert consumed twice");
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
                        ZqValue::new_string(cx, "async work failed").unwrap()
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
            0 // Done
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => 1, // Block
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            // Worker ended without delivering — deterministic reject.
            state.convert.take();
            let reason = ZqValue::new_string(cx, "async work dropped without a result").unwrap();
            let _ = cx.call_function(ZqValue::from(state.reject), ZqValue::undefined(), &[reason]);
            0 // Done
        }
    }
}

/// Drop callback for the promise job: the host frees the boxed state when
/// the job leaves the queue, whether it settled normally or the runtime tore
/// the queue down. `spawn_run` must not free it.
unsafe extern "C" fn promise_drop<T, C>(data: *mut c_void)
where
    T: 'static,
    C: 'static,
{
    drop(unsafe { Box::from_raw(data as *mut PromiseState<T, C>) });
}

/// GC trace for the promise job: mark the capability's promise and
/// resolving functions so they survive until the job settles it.
unsafe extern "C" fn promise_trace<T, C>(data: *mut c_void, gcx: *mut ZqGcTraceCtxS) {
    let state = unsafe { &*(data as *const PromiseState<T, C>) };
    let ctx = ZqGcTraceCtx::from(gcx);
    state.gc_trace(ctx);
}

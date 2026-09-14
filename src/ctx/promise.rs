// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::{ffi::c_void, future::Future, pin::Pin};

use crate::{
    ZqError, ZqGcTraceCtx, ZqValue,
    channel::ZqWakeSender,
    ffi::{ZqApi, ZqGcTraceCtxS, ZqPromiseCapabilityS, ZqRawCtx, ZqRawValue},
};

use super::ZqCtx;

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

    /// High-level one-shot async: run `work` on a dedicated blocked thread,
    /// deliver its result to `convert` (JS thread) and settle the returned
    /// Promise.
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
    ///
    /// For async runtimes that own an executor (e.g. tokio), prefer
    /// [`spawn_promise_async`](Self::spawn_promise_async), which runs the
    /// work on that executor instead of spawning one OS thread per call.
    pub fn spawn_promise<T, F, C>(&self, work: F, convert: C) -> Result<ZqValue<'cx>, ZqError>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
        C: for<'a> FnOnce(ZqCtx<'a>, T) -> Result<ZqValue<'a>, ZqError> + 'static,
    {
        self.spawn_promise_inner(
            move |tx| {
                std::thread::spawn(move || {
                    let t = work();
                    let _ = tx.send(t); // WakingSender wakes the parked loop automatically.
                });
            },
            convert,
        )
    }

    /// Like [`spawn_promise`](Self::spawn_promise), but the work is an async
    /// [`Future`] submitted to a caller-supplied executor via `spawn` — no OS
    /// thread is created per call. This keeps `zuqe-zapi` runtime-agnostic:
    /// pass, for example, `|fut| { tokio_handle.spawn(fut); }` to run the work
    /// on an existing tokio runtime.
    ///
    /// `spawn` receives an erased `Send` future and is expected to drive it to
    /// completion. Completion is delivered to `convert` on the JS thread and
    /// settles the returned Promise, exactly as in `spawn_promise`.
    pub fn spawn_promise_async<T, F, S, C>(
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
        self.spawn_promise_inner(
            move |tx| {
                spawn_fn(Box::pin(async move {
                    let t = fut.await;
                    let _ = tx.send(t); // WakingSender wakes the parked loop automatically.
                }));
            },
            convert_fn,
        )
    }

    /// Shared plumbing for both `spawn_promise` and `spawn_promise_async`:
    /// create the capability + wake channel, hand the sender to `launch` (which
    /// starts the work), and register the polling job that settles the Promise.
    fn spawn_promise_inner<T, L, C>(&self, launch: L, convert: C) -> Result<ZqValue<'cx>, ZqError>
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

        let state = Box::new(SpawnState {
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
                spawn_run::<T, C>,
                Some(spawn_trace::<T, C>),
                Box::into_raw(state).cast::<c_void>(),
            );
        }

        Ok(cap.promise)
    }
}

/// State for the `spawn_promise` / `spawn_promise_async` polling job: the
/// channel, the convert closure, and the capability's raw handles (so
/// `spawn_trace` can keep the resolving functions alive while the job is
/// pending).
struct SpawnState<T, C> {
    rx: std::sync::mpsc::Receiver<T>,
    convert: Option<C>,
    promise: ZqRawValue,
    resolve: ZqRawValue,
    reject: ZqRawValue,
}

/// Polling-job body for the async bridge: settle the capability's promise via
/// its resolving functions, then free `data` and report `Done`.
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
            drop(unsafe { Box::from_raw(data as *mut SpawnState<T, C>) });
            0 // Done
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => 1, // Block
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            // Worker ended without delivering — deterministic reject.
            state.convert.take();
            let reason = ZqValue::new_string(cx, "async work dropped without a result").unwrap();
            let _ = cx.call_function(ZqValue::from(state.reject), ZqValue::undefined(), &[reason]);
            drop(unsafe { Box::from_raw(data as *mut SpawnState<T, C>) });
            0 // Done
        }
    }
}

/// GC trace for the async-bridge job: mark the capability's promise and
/// resolving functions so they survive until the job settles it.
unsafe extern "C" fn spawn_trace<T, C>(data: *mut c_void, gcx: *mut ZqGcTraceCtxS) {
    let state = unsafe { &*(data as *const SpawnState<T, C>) };
    let ctx = ZqGcTraceCtx::from(gcx);
    let _ = ctx.mark(state.promise);
    let _ = ctx.mark(state.resolve);
    let _ = ctx.mark(state.reject);
}

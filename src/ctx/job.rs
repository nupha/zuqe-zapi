// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::ffi::c_void;

use crate::{
    ZqCtx, ZqGcTrace, ZqGcTraceCtx,
    ffi::{ZqApi, ZqGcTraceCtxS, ZqRawCtx},
};

impl<'cx> ZqCtx<'cx> {
    /// Register a job on the event loop: `job.run` runs once per event-loop
    /// tick until it returns [`ZqPoll::Done`]. Return [`ZqPoll::Again`] to
    /// stay registered (the loop parks until woken — pair with
    /// `make_wake_channel` / `promise_fut*` so the wake actually arrives).
    /// Return [`ZqPoll::AgainNoHold`] for fire-and-forget jobs that must not
    /// keep the process alive.
    ///
    /// The payload may hold GC-managed JS values: [`ZqGcTrace::trace`] marks
    /// them every GC cycle. Implement `ZqJob` (empty `trace` when the payload
    /// holds no JS values). The payload is dropped once the job leaves the
    /// queue — after [`ZqPoll::Done`], or when the runtime cancels it (queue
    /// torn down at teardown / interrupt) without a normal completion. The
    /// queue owns the payload in both cases, so the host always frees it.
    pub fn add_job<T: ZqJob + 'static>(&self, job: T) {
        let opaque = Box::into_raw(Box::new(job)).cast::<c_void>();
        unsafe {
            (ZqApi::singleton().add_job)(
                self.raw,
                traced_job_run::<T>,
                Some(traced_job_trace::<T>),
                traced_job_drop::<T>,
                opaque,
            );
        }
    }

    /// Shortcut for [`ZqCtx::add_job`] that wraps a plain polling closure into
    /// a [`ZqJob`] with an empty GC trace — only for jobs that hold **no**
    /// GC-managed JS values. Use [`ZqCtx::add_job`] directly for payloads that
    /// need tracing.
    pub fn add_job_no_gc<F>(&self, job: F)
    where
        F: FnMut(ZqCtx<'_>) -> ZqPoll + 'static,
    {
        /// Adapter turning a polling closure into a [`ZqJob`] (empty GC trace).
        struct NoGcJob<F>(F);

        impl<F> ZqGcTrace for NoGcJob<F> {
            #[inline(always)]
            fn trace(&self, _: &mut ZqGcTraceCtx) {}
        }

        impl<F> ZqJob for NoGcJob<F>
        where
            F: FnMut(ZqCtx<'_>) -> ZqPoll + 'static,
        {
            #[inline(always)]
            fn run(&mut self, ctx: ZqCtx<'_>) -> ZqPoll {
                (self.0)(ctx)
            }
        }

        self.add_job(NoGcJob(job));
    }
}

/// Return value of a [`ZqCtx::add_job`] job.
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

/// An event-loop job payload, submitted via [`ZqCtx::add_job`].
///
/// Implement [`ZqGcTrace::trace`] for every GC-managed JS value the payload
/// holds (leave empty when it holds none); `run` polls once per event-loop
/// tick until it returns [`ZqPoll::Done`].
pub trait ZqJob: ZqGcTrace {
    /// Poll once per event-loop tick.
    fn run(&mut self, ctx: ZqCtx<'_>) -> ZqPoll;
}

/// Trampoline: host calls this per tick with the boxed job.
unsafe extern "C" fn traced_job_run<T: ZqJob>(data: *mut c_void, raw_ctx: ZqRawCtx) -> i32 {
    let ctx = ZqCtx::from(raw_ctx);
    let poll = {
        let job = unsafe { &mut *(data as *mut T) };
        job.run(ctx)
    };
    match poll {
        ZqPoll::Done => 0,
        ZqPoll::Again => 1,
        ZqPoll::AgainNoHold => 2,
    }
}

/// Trampoline: host calls this to release the boxed job when it leaves the
/// queue, whether it ran to completion or was cancelled by runtime teardown.
unsafe extern "C" fn traced_job_drop<T: ZqJob>(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data as *mut T) });
}

/// Trampoline: host calls this every GC cycle with the boxed job.
unsafe extern "C" fn traced_job_trace<T: ZqJob>(data: *mut c_void, gcx: *mut ZqGcTraceCtxS) {
    let job = unsafe { &*(data as *const T) };
    let mut gcx = ZqGcTraceCtx::from(gcx);
    job.trace(&mut gcx);
}

use std::ffi::c_void;

use crate::{
    ZqCtx,
    ffi::{ZqApi, ZqJobRunFn, ZqRawCtx},
};

impl<'cx> ZqCtx<'cx> {
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

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::sync::Arc;

use crate::ffi::ZqWakeHandleS;

/// Thread-safe handle that wakes the JS event loop from any thread.
///
/// Create during a callback (`ctx.wake_handle()`) and store it in the
/// worker/async state that produces the result; call `wake()` after pushing
/// the result so a parked event loop re-ticks immediately. Without this, a
/// pure event-driven host (quantum poll off) would hang until some other
/// event happens to fire.
///
/// Wake is idempotent and cheap: call it as often as you like, from any
/// thread. It is a no-op when the loop is running or already gone, and a
/// handle may legally outlive its loop.
///
/// # Ownership — one token, one owner
///
/// Each `ZqWakeHandle` owns exactly one token minted by the host
/// (`new_waker` … `drop_waker`). It is **deliberately not `Clone`**: that is
/// what makes the 1:1 mint/release contract a *compile-time* property. A
/// derived `Clone` would copy the raw token without owning it, so the two
/// handles would free the same token and the second `wake()` would be a
/// use-after-free (symptom: the loop works once, then parks forever).
///
/// A wake *capability* is naturally shared, so there are exactly two
/// supported ways to reach more than one waker:
///
/// * [`ZqWakeHandle::share`] → `Arc<ZqWakeHandle>`, when your data path is not
///   a zapi channel (your own queue, a flag behind a mutex, …);
/// * clone a [`ZqWakeSender`](crate::ZqWakeSender) — the blessed carrier for
///   the common "N producers → one event loop" case, because a channel is
///   already the multi-producer thing.
///
/// Rule of thumb: **one handle = one owner; to have several wakers, clone the
/// sender (or `share()` the handle).**
#[derive(Debug)]
pub struct ZqWakeHandle {
    raw: *mut ZqWakeHandleS,
    /// Captured at creation (on the JS thread): `ZqApi::singleton()` is
    /// thread-local, so `wake()` from worker threads must not touch it.
    wake_fn: unsafe extern "C" fn(*mut ZqWakeHandleS),
    free_fn: unsafe extern "C" fn(*mut ZqWakeHandleS),
}

impl Drop for ZqWakeHandle {
    fn drop(&mut self) {
        unsafe { (self.free_fn)(self.raw) };
    }
}

unsafe impl Send for ZqWakeHandle {}
unsafe impl Sync for ZqWakeHandle {}

impl ZqWakeHandle {
    /// Build from raw parts captured on the JS thread.
    pub(super) fn from_raw_parts(
        raw: *mut ZqWakeHandleS,
        wake_fn: unsafe extern "C" fn(*mut ZqWakeHandleS),
        free_fn: unsafe extern "C" fn(*mut ZqWakeHandleS),
    ) -> Self {
        Self {
            raw,
            wake_fn,
            free_fn,
        }
    }

    /// Wake the parked JS event loop. Cheap no-op when not parked.
    /// Thread-safe (uses captured fn pointers, no thread-local access).
    #[inline(always)]
    pub fn wake(&self) {
        unsafe { (self.wake_fn)(self.raw) };
    }

    /// Turn this exclusive token into a shared capability.
    ///
    /// This is the explicit spelling of "several producers may wake this
    /// loop". Prefer cloning a [`ZqWakeSender`](crate::ZqWakeSender) when the
    /// data path *is* a zapi channel; use this when it is not (your own
    /// channel flavor, a shared queue, a flag the polling job re-reads).
    #[inline(always)]
    pub fn share(self) -> Arc<Self> {
        Arc::new(self)
    }
}

/// Type-level regression guard: `ZqWakeHandle` must never become `Clone`
/// again. If it does, both `AmbiguousIfImpl` impls below apply and the
/// inference of `_` becomes ambiguous — the crate stops compiling with
/// "type annotations needed", which is the intended failure.
const _: fn() = || {
    trait AmbiguousIfImpl<A> {
        fn some_item() {}
    }
    impl<T: ?Sized> AmbiguousIfImpl<()> for T {}
    struct Invalid;
    impl<T: ?Sized + Clone> AmbiguousIfImpl<Invalid> for T {}
    let _ = <ZqWakeHandle as AmbiguousIfImpl<_>>::some_item;
};

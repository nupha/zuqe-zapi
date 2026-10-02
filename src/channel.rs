// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025-2026 John Ray <996351336@qq.com>

//! Wake-aware channel sender for the standard zapi async pattern:
//! JS creates the channel → an OS worker thread produces results → a
//! `queue_job` polling job drains the receiver. The sender wakes the parked
//! event loop on every send and on drop (all senders gone → the polling job
//! can settle on `Disconnected`), so producers cannot forget to wake.
//!
//! Backed by `std::sync::mpsc` (unbounded) — no extra dependency.
//!
//! This type is also **the** carrier for a shared wake capability: cloning it
//! is correct and cheap (one `Sender` clone + one `Arc` bump), so
//! "N producers, one event loop" is just `let tx2 = tx.clone()`. The
//! underlying [`ZqWakeHandle`] is an exclusive token on purpose — see its
//! docs for the ownership rule.

use std::sync::{Arc, mpsc};

use crate::{ZqCtx, ctx::ZqWakeHandle};

/// Channel sender that wakes the JS event loop automatically.
///
/// * every successful [`send`](ZqWakeSender::send) wakes the loop that created
///   this sender;
/// * dropping the sender wakes it too, so a polling job observes the
///   disconnect promptly;
/// * [`Clone`] is *shared*, not duplicated: it clones the channel sender and
///   bumps one `Arc`, so every clone wakes the same loop exactly once.
///
/// Thread-safe; cheap when the loop is not parked. Create one via
/// [`ZqCtx::make_wake_channel`], or put the same capability on your own
/// sender with [`ZqWakeSender::new`] / [`ZqWakeSender::from_shared`].
pub struct ZqWakeSender<T> {
    tx: mpsc::Sender<T>,
    /// Shared, never owned: sharing lives here and only here. The
    /// [`ZqWakeHandle`] itself stays an exclusive token (1 mint = 1 release).
    waker: Arc<ZqWakeHandle>,
}

impl<T> Clone for ZqWakeSender<T> {
    /// Hand-written on purpose: a **derived** `Clone` would add a `T: Clone`
    /// bound, so a channel carrying non-`Clone` messages could no longer be
    /// handed to a second producer — exactly what this type exists for.
    /// A compile-time assertion below pins that down.
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            waker: Arc::clone(&self.waker),
        }
    }
}

impl<T> std::fmt::Debug for ZqWakeSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZqWakeSender")
            .field("shares", &Arc::strong_count(&self.waker))
            .finish()
    }
}

impl<T> ZqWakeSender<T> {
    /// Pair an existing sender with a wake token, taking ownership of it.
    ///
    /// Use this when you need your own channel flavor (e.g. a bounded
    /// `crossbeam` channel for backpressure) but still want the
    /// "every send wakes" contract. The token is shared from here on, so
    /// cloning the returned sender is free.
    pub fn new(tx: mpsc::Sender<T>, waker: ZqWakeHandle) -> Self {
        Self {
            tx,
            waker: waker.share(),
        }
    }

    /// Join an existing sharing group: reuse the same `Arc<ZqWakeHandle>` as
    /// other senders/wrappers for the same loop.
    pub fn from_shared(tx: mpsc::Sender<T>, waker: Arc<ZqWakeHandle>) -> Self {
        Self { tx, waker }
    }

    /// The underlying raw sender, for paths that must not wake (e.g. a POSIX
    /// signal handler, which must use async-signal-safe wakeups instead).
    /// A silent send can park the loop forever — wake it explicitly after.
    pub fn tx(&self) -> &mpsc::Sender<T> {
        &self.tx
    }

    /// The shared wake handle, for waking without sending (e.g. the consumer
    /// polls state you own, so there is no message to carry).
    pub fn waker(&self) -> &Arc<ZqWakeHandle> {
        &self.waker
    }

    /// Wake the parked loop without sending anything.
    pub fn wake(&self) {
        self.waker.wake();
    }

    /// Send and wake the parked event loop. The channel is unbounded, so
    /// this never blocks; it fails only if the receiver is gone.
    pub fn send(&self, value: T) -> Result<(), mpsc::SendError<T>> {
        let r = self.tx.send(value);
        if r.is_ok() {
            self.waker.wake();
        }
        r
    }
}

impl<T> Drop for ZqWakeSender<T> {
    fn drop(&mut self) {
        // Sender gone: a `Disconnected` can only be discovered by the next
        // poll — wake the parked loop so it happens now.
        self.waker.wake();
    }
}

impl<'cx> ZqCtx<'cx> {
    /// Create an unbounded channel whose sender wakes this event loop
    /// automatically on every send and on drop. Pair the receiver with a
    /// `queue_job` polling job: the worker's sends wake the parked loop, so
    /// producers cannot forget to wake. The returned sender is `Clone` — hand
    /// one clone to each producer. See [`crate::channel::ZqWakeSender`].
    pub fn make_wake_channel<T: 'static>(&self) -> (ZqWakeSender<T>, mpsc::Receiver<T>) {
        let (tx, rx) = mpsc::channel();
        (ZqWakeSender::new(tx, self.wake_handle()), rx)
    }

    /// Sugar over [`make_wake_channel`](Self::make_wake_channel) +
    /// [`add_job`](crate::ZqCtx::add_job): install a persistent polling job that
    /// drains the channel into `handler`, and return the sender to hand to
    /// producers.
    ///
    /// `handler` returns [`ZqChannelPoll::Continue`] to keep the job registered
    /// (it re-blocks until woken) or [`ZqChannelPoll::Done`] to release it. The
    /// job also terminates when the channel disconnects (all senders dropped).
    /// A JS exception left by `handler` is cleared automatically, mirroring an
    /// event callback that cannot propagate a throw.
    ///
    /// This is the standard "native event stream → JS callback" shape; it has
    /// long-term value and is the recommended spelling over the raw pieces.
    pub fn spawn_channel_job<T, H>(&self, label: &'static str, handler: H) -> ZqWakeSender<T>
    where
        T: 'static,
        H: FnMut(ZqCtx<'_>, T) -> ZqChannelPoll + 'static,
    {
        let (tx, rx) = self.make_wake_channel::<T>();

        struct ChannelJob<T, H> {
            label: &'static str,
            rx: mpsc::Receiver<T>,
            handler: H,
        }

        impl<T, H> crate::gc::ZqGcTrace for ChannelJob<T, H> {
            // Payload holds no GC-managed JS values; callbacks live wherever the
            // extension traces them.
            fn trace(&self, _: &mut crate::gc::ZqGcTraceCtx) {}
        }

        impl<T, H> crate::ctx::ZqJob for ChannelJob<T, H>
        where
            T: 'static,
            H: FnMut(ZqCtx<'_>, T) -> ZqChannelPoll + 'static,
        {
            fn run(&mut self, cx: ZqCtx<'_>) -> crate::ctx::ZqPoll {
                match self.rx.try_recv() {
                    Ok(msg) => {
                        let cont = (self.handler)(cx, msg) == ZqChannelPoll::Continue;
                        if !cx.take_exception().is_undefined() {
                            #[cfg(debug_assertions)]
                            eprintln!("[channel job:{}] handler left exception", self.label);
                        }
                        if cont {
                            crate::ctx::ZqPoll::Again
                        } else {
                            crate::ctx::ZqPoll::Done
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => crate::ctx::ZqPoll::Again,
                    Err(mpsc::TryRecvError::Disconnected) => crate::ctx::ZqPoll::Done,
                }
            }
        }

        self.add_job(ChannelJob { label, rx, handler });
        tx
    }
}

/// Control returned by a [`ZqCtx::spawn_channel_job`] handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZqChannelPoll {
    /// Keep the polling job registered (it re-blocks until woken).
    Continue,
    /// Release the polling job.
    Done,
}

/// Compile-time assertions on the sharing model.
///
/// * `ZqWakeSender<T>: Clone` must hold for **every** `T`, including a message
///   type that is neither `Clone` nor `Send`: cloning the sender is about the
///   wake capability, never about the payload. (A `#[derive(Clone)]` on the
///   struct would silently add `T: Clone` and break exactly this — which is
///   why the `Clone` impl above is hand-written.)
/// * It must also be `Send`, so one sender clone can be moved to each producer
///   thread.
const _: fn() = || {
    /// Deliberately implements nothing.
    struct Plain;

    fn assert_clone<T: Clone>() {}
    fn assert_send<T: Send>() {}

    assert_clone::<ZqWakeSender<Plain>>();
    assert_send::<ZqWakeSender<Plain>>();
};

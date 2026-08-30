// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2025-2026 John Ray <996351336@qq.com>

//! Wake-aware channel for the standard zapi async pattern:
//! JS creates the channel → an OS worker thread produces results → a
//! `queue_job` polling job drains the receiver. The sender wakes the parked
//! event loop on every send and on drop (all senders gone → the polling job
//! can settle on `Disconnected`), so producers cannot forget to wake.
//!
//! Backed by `std::sync::mpsc` (unbounded) — no extra dependency.

use std::sync::mpsc::{SendError, Sender};

use crate::ctx::ZqWakeHandle;

/// Channel sender that wakes the JS event loop automatically.
///
/// * every successful [`send`](WakeSender::send) / [`try_send`](WakeSender::try_send)
///   wakes the loop that created this sender;
/// * dropping the sender wakes it too, so the polling job observes the
///   disconnect promptly.
///
/// Thread-safe; cheap when the loop is not parked. Create via
/// [`ZqCtx::make_wake_channel`](crate::ctx::ZqCtx::make_wake_channel) or
/// [`WakeSender::wrap`] around an existing crossbeam sender.
pub struct WakeSender<T> {
    tx: Sender<T>,
    wake: Option<ZqWakeHandle>,
}

impl<T> Clone for WakeSender<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            wake: self.wake.clone(),
        }
    }
}

impl<T> std::fmt::Debug for WakeSender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WakeSender")
            .field("has_wake", &self.wake.is_some())
            .finish()
    }
}

impl<T> WakeSender<T> {
    /// Attach a wake handle to an existing raw sender. `None` disables
    /// waking (plain passthrough).
    pub fn wrap(tx: Sender<T>, wake: Option<ZqWakeHandle>) -> Self {
        Self { tx, wake }
    }

    pub(crate) fn new(tx: Sender<T>, wake: Option<ZqWakeHandle>) -> Self {
        Self { tx, wake }
    }

    /// The underlying raw sender, for paths that must not wake.
    pub fn raw(&self) -> &Sender<T> {
        &self.tx
    }

    /// Send and wake the parked event loop. The channel is unbounded, so
    /// this never blocks; it fails only if the receiver is gone.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let r = self.tx.send(value);
        if r.is_ok() {
            if let Some(w) = &self.wake {
                w.wake();
            }
        }
        r
    }
}

impl<T> Drop for WakeSender<T> {
    fn drop(&mut self) {
        // Sender gone: a `Disconnected` can only be discovered by the next
        // poll — wake the parked loop so it happens now.
        if let Some(w) = &self.wake {
            w.wake();
        }
    }
}

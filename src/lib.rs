// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

pub mod channel;
pub mod class;
pub mod ctx;
pub mod error;
pub mod ffi;
pub mod gc;
pub mod iter;
pub mod traits;
pub mod value;

// Internal module for proc-macro generated code.
#[doc(hidden)]
pub mod __private;

pub use {
    __private::module_path,
    channel::ZqWakeSender,
    class::Class,
    ctx::{ZqCtx, ZqPromiseCapability, ZqWakeHandle},
    error::{ZqError, ZqErrorType},
    ffi::{ZqApi, ZqFieldDef, ZqRawCtx, ZqRawValue},
    gc::{ZqGcTrace, ZqGcTraceCtx},
    iter::{AsyncIterResult, ZqAsyncIterable},
    traits::IntoZqValue,
    value::{ZqCallInfo, ZqValue, ZqValueType},
};

impl ZqApi {
    thread_local! {
        pub(crate) static ZAPI_SINGLETON: std::cell::Cell<*const ZqApi> = const { std::cell::Cell::new(std::ptr::null()) };
    }

    /// The process-wide `ZqApi` singleton, established by [`__private::zapi_init`]
    /// (the module entry `zq_module_register` runs it before any body executes).
    ///
    /// The caller **must** have called `zapi_init` first — i.e. only use this inside
    /// a body / module init. It is a programming error otherwise: `debug_assertions`
    /// builds panic via `unwrap`, release builds use `unwrap_unchecked` (UB).
    #[inline]
    pub fn singleton() -> &'static Self {
        let api = Self::ZAPI_SINGLETON.with(|g| g.get());

        // SAFETY: the caller guarantees `zapi_init` already ran
        #[cfg(debug_assertions)]
        unsafe {
            debug_assert!(!api.is_null());
            api.as_ref().unwrap()
        }
        #[cfg(not(debug_assertions))]
        unsafe {
            api.as_ref().unwrap_unchecked()
        }
    }
}

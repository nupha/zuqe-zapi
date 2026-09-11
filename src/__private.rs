// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

//! Internal types used by `zapi-macros`. Not part of the public API.

use std::{
    ffi::{CStr, c_char},
    sync::atomic::{AtomicPtr, Ordering},
};

pub use crate::{
    ctx::ZqCtx,
    error::ZqError,
    ffi::{
        ZQ_VERSION_PACKED, ZqApi, ZqCallInfoS, ZqClassDef, ZqFieldDef, ZqFieldValueFn,
        ZqModuleInfo, ZqRawCtx, ZqRawValue,
    },
    traits::IntoZqValue,
    value::ZqValue,
};

/// C-compatible callback function pointer (ABI-compatible with ZqApi body functions).
/// Returns the JS result value, or `value_exception` on error.
pub type ZqCallbackFn = unsafe extern "C" fn(env: ZqRawCtx, cc: *mut ZqCallInfoS) -> ZqRawValue;

/// Establish the process-wide `ZqApi` singleton. Idempotent: sets it only on
/// the first call. A later call with a *different* pointer means two hosts or a
/// re-registered module — checked in debug builds (`debug_assert_eq!`), and
/// silently ignored in release.
pub fn zapi_init(api: *const ZqApi) {
    ZqApi::ZAPI_SINGLETON.with(|g| {
        let curr = g.get();
        if curr.is_null() {
            g.set(api);
        } else {
            debug_assert_eq!(
                curr, api,
                "zapi_init called twice with different api pointers"
            );
        }
    });
}

/// Host-provided canonical dylib path (NUL-terminated), stashed by the
/// generated `zq_module_register` from `ZqModuleInfo.path`. Atomic since
/// registration and later method/init calls may run on different threads.
/// The host leaks the string, so the pointer is valid for the process
/// lifetime. Statics are per-dylib (each dlopen'd library has its own copy
/// of this crate), so concurrent modules don't collide.
static MODULE_PATH: AtomicPtr<c_char> = AtomicPtr::new(std::ptr::null_mut());

/// Called by the generated `zq_module_register` with `ZqModuleInfo.path`.
pub fn path_init(ptr: *const c_char) {
    MODULE_PATH.store(ptr as *mut c_char, Ordering::Release);
}

/// Canonical filesystem path of the dylib this extension was loaded from,
/// as provided by the host at registration (`None` when the host predates
/// the field or no path was given).
pub fn module_path() -> Option<&'static str> {
    let ptr = MODULE_PATH.load(Ordering::Acquire);
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok()
}

//! Internal types used by `zapi-macros`. Not part of the public API.

use std::cell::Cell;
use std::ffi::{CStr, c_char};
use std::sync::atomic::{AtomicPtr, Ordering};

pub use crate::ctx::ZqCtx;
pub use crate::error::ZqError;
pub use crate::ffi::{
    ZQ_ABI_VERSION, ZQ_API_VERSION, ZqApi, ZqCallbackInfoS, ZqClassDef, ZqEnv, ZqEnvS, ZqFieldDef,
    ZqFieldValueFn, ZqModuleInfo, ZqRawValue,
};
pub use crate::traits::{FromZqValue, IntoZqValue};
pub use crate::value::ZqValue;
pub use std::mem::size_of;

/// C-compatible callback function pointer (ABI-compatible with ZqApi body functions).
/// Returns the JS result value, or `value_exception` on error.
pub type ZqCallback = unsafe extern "C" fn(env: ZqEnv, info: ZqCallbackInfo) -> ZqRawValue;

/// The host's exception sentinel handle (`ZqApi.value_exception`). Trampolines
/// return it to signal "this call threw".
pub fn exception_value() -> ZqRawValue {
    zapi_singleton().value_exception
}
/// Opaque callback-info handle (passed to callback functions).
pub type ZqCallbackInfo = *mut ZqCallbackInfoS;

thread_local! {
    static ZAPI_SINGLETON: Cell<*const ZqApi> = const { Cell::new(std::ptr::null()) };
}

/// The process-wide `ZqApi` singleton, established by [`zapi_init`] (the module
/// entry `zq_module_register` runs it before any body executes).
///
/// The caller **must** have called `zapi_init` first — i.e. only use this inside
/// a body / module init. It is a programming error otherwise: `debug_assertions`
/// builds panic via `unwrap`, release builds use `unwrap_unchecked` (UB).
#[inline]
pub fn zapi_singleton() -> &'static ZqApi {
    let api = ZAPI_SINGLETON.with(|g| g.get());

    // SAFETY: the caller guarantees `zapi_init` already ran
    #[cfg(debug_assertions)]
    unsafe {
        api.as_ref().unwrap()
    }
    #[cfg(not(debug_assertions))]
    unsafe {
        api.as_ref().unwrap_unchecked()
    }
}

/// Establish the process-wide `ZqApi` singleton. Idempotent: sets it only on
/// the first call. A later call with a *different* pointer means two hosts or a
/// re-registered module — checked in debug builds (`debug_assert_eq!`), and
/// silently ignored in release.
pub fn zapi_init(api: *const ZqApi) {
    ZAPI_SINGLETON.with(|g| {
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

//! Internal types used by `zapi-macros`. Not part of the public API.

use std::cell::Cell;

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

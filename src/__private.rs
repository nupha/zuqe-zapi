//! Internal types used by `zapi-macros`. Not part of the public API.

use std::cell::Cell;

pub use crate::ctx::ZqCtx;
pub use crate::error::ZqError;
pub use crate::ffi::{
    ZqApi, ZqCallbackInfoS, ZqClassDef, ZqEnv, ZqEnvS, ZqExportValueFn, ZqFieldDef, ZqModuleInfo,
    ZqRawValue, ZQ_ABI_VERSION, ZQ_API_VERSION,
};
pub use crate::traits::{FromZqValue, IntoZqValue};
pub use crate::value::ZqValue;
pub use std::mem::size_of;

/// C-compatible callback function pointer (ABI-compatible with ZqApi body functions).
pub type ZqCallback = unsafe extern "C" fn(env: ZqEnv, info: ZqCallbackInfo) -> ZqRawValue;
/// Opaque callback-info handle (passed to callback functions).
pub type ZqCallbackInfo = *mut ZqCallbackInfoS;

thread_local! {
    static GLOBAL_API: Cell<*const ZqApi> = const { Cell::new(std::ptr::null()) };
}

pub fn global_api() -> Option<&'static ZqApi> {
    GLOBAL_API.with(|g| {
        let p = g.get();
        if p.is_null() {
            None
        } else {
            unsafe { Some(&*p) }
        }
    })
}

pub fn init_api(api: *const ZqApi) {
    GLOBAL_API.with(|g| g.set(api));
}

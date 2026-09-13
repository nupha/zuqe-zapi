// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::marker::PhantomData;

use crate::{
    ZqError, ZqRawCtx, ZqValue,
    class::Class,
    ffi::{ZQ_CONST_VALUE_SLOT_EXCEPTION, ZqApi, ZqCallInfoS, ZqClassDef, ZqRawValue},
};

mod job;
mod promise;
mod wake;

pub use promise::ZqPromiseCapability;
pub use wake::ZqWakeHandle;

#[derive(Debug)]
#[repr(transparent)]
pub struct ZqCtx<'cx> {
    pub(crate) raw: ZqRawCtx,
    _marker: PhantomData<&'cx ()>,
}

impl<'cx> Clone for ZqCtx<'cx> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}
impl<'cx> Copy for ZqCtx<'cx> {}

impl<'cx> From<ZqRawCtx> for ZqCtx<'cx> {
    #[inline(always)]
    fn from(raw: ZqRawCtx) -> Self {
        Self {
            raw,
            _marker: PhantomData,
        }
    }
}

impl<'cx> From<ZqCtx<'cx>> for ZqRawCtx {
    #[inline(always)]
    fn from(ctx: ZqCtx<'cx>) -> Self {
        ctx.raw
    }
}

impl<'cx> ZqCtx<'cx> {
    /// Capture a wake handle for this context's event loop. Call on the JS
    /// thread (inside a callback / job) and move the handle into whatever
    /// thread or task produces the async result.
    pub fn wake_handle(&self) -> ZqWakeHandle {
        let api = ZqApi::singleton();
        ZqWakeHandle::from_parts(
            unsafe { (api.new_waker)(self.raw) },
            api.waker_wake,
            api.waker_free,
        )
    }

    /// `val instanceof class` for a native class (honours `Symbol.hasInstance`).
    pub fn is_instance_of_class<T>(
        &self,
        val: impl Into<ZqValue<'cx>>,
        class: &Class<'_, T>,
    ) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let val = val.into();
        let r = unsafe { (api.instanceof_class)(self.raw, val.raw(), class.def()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// `val instanceof target` for an arbitrary constructor value (honours
    /// `Symbol.hasInstance`).
    pub fn is_instance_of(
        &self,
        val: impl Into<ZqValue<'cx>>,
        target: impl Into<ZqValue<'cx>>,
    ) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let val = val.into();
        let target = target.into();
        let r = unsafe { (api.instanceof)(self.raw, val.raw(), target.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Look up a property on the global object (`globalThis[name]`).
    pub fn get_global_str(&self, name: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let v = unsafe { (api.get_global)(self.raw, name.as_ptr(), name.len()) };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Assign a property on the global object (`globalThis[name] = val`).
    /// Returns whether the assignment applied (false on a silent no-op).
    pub fn set_global_str(
        &self,
        name: &str,
        val: impl Into<ZqValue<'cx>>,
    ) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let val = val.into();
        let r = unsafe { (api.set_global)(self.raw, name.as_ptr(), name.len(), val.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Calls a JS function with `this` and arguments, returning its result.
    ///
    /// The host hands every argument (numbers included) to extension callbacks
    /// as proper engine JSValues, so they are forwarded unchanged.
    pub fn call_function(
        &self,
        target: impl Into<ZqValue<'cx>>,
        this: impl Into<ZqValue<'cx>>,
        args: &[ZqValue],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let target = target.into();
        let this = this.into();
        let raw_args: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let mut info = ZqCallInfoS {
            target: target.raw(),
            this: this.raw(),
            argv: raw_args.as_ptr(),
            argc: raw_args.len(),
            magic: 0,
        };
        let ret = unsafe { (api.call)(self.raw, &mut info) };
        if ret == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(ret))
        }
    }

    /// Invoke `target` as a constructor: `new target(...args)`.
    pub fn call_constructor(
        &self,
        target: impl Into<ZqValue<'cx>>,
        args: &[ZqValue],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let target = target.into();
        let raw_args: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let mut info = ZqCallInfoS {
            target: target.raw(),
            this: 0,
            argv: raw_args.as_ptr(),
            argc: raw_args.len(),
            magic: 0,
        };
        let ret = unsafe { (api.construct)(self.raw, &mut info) };
        if ret == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(ret))
        }
    }

    /// `JSON.parse(text)`.
    pub fn json_parse(&self, text: &str) -> Result<ZqValue<'cx>, ZqError> {
        let json = self.get_global_str("JSON")?;
        let arg = ZqValue::new_string(*self, text)?;
        json.invoke_str(*self, "parse", &[arg])
    }

    /// `JSON.stringify(val)`; `None` for values with no JSON form.
    pub fn json_stringify(&self, val: impl Into<ZqValue<'cx>>) -> Result<Option<String>, ZqError> {
        let json = self.get_global_str("JSON")?;
        let r = json.invoke_str(*self, "stringify", &[val.into()])?;
        if r.is_undefined() {
            Ok(None)
        } else {
            r.to_utf8_string(*self).map(Some)
        }
    }

    /// Register a global GC trace callback for values the extension holds
    /// outside the GC heap (thread_locals, static natives). `cb` runs at every
    /// GC and should `mark()` every such value via `ZqGcTraceCtx`.
    ///
    /// Typically called from the module `init` hook.
    ///
    /// # Safety
    ///
    /// `opaque` must point to valid extension state for the lifetime of the
    /// registration, and `cb` must be a valid function pointer that can safely
    /// access that state. The callback must not panic or perform any operation
    /// that could invalidate the JS heap while tracing is active.
    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn gc_set_trace_func(
        &self,
        func: Option<crate::ffi::ZqGcTraceFn>,
        opaque: *mut std::ffi::c_void,
    ) -> Result<(), ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.gc_set_trace_func)(self.raw, func, opaque) };
        if r < 0 { Err(ZqError) } else { Ok(()) }
    }

    // ── class / instance helpers (requires GLOBAL_API) ──

    pub fn register_class(&self, def: &ZqClassDef) -> Result<i32, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.register_class)(self.raw, def) };
        if r < 0 { Err(ZqError) } else { Ok(r) }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn new_instance(
        &self,
        def: &ZqClassDef,
        payload: *mut std::ffi::c_void,
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.new_instance)(self.raw, def, payload) };
        if r == 0 || r == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(r))
        }
    }
}

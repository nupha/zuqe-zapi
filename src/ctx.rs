// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::marker::PhantomData;

use crate::{
    __private::zapi_singleton,
    ZqEnv, ZqError, ZqStatus, ZqType, ZqValue,
    ffi::{ZqCallbackInfoS, ZqClassDef, ZqRawValue},
};

#[derive(Debug)]
#[repr(transparent)]
pub struct ZqCtx<'cx> {
    pub(crate) env: ZqEnv,
    pub(crate) _marker: PhantomData<&'cx ()>,
}

impl<'cx> ZqCtx<'cx> {
    #[inline]
    pub unsafe fn from_raw(env: ZqEnv) -> Self {
        Self {
            env,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn env_raw(&self) -> ZqEnv {
        self.env
    }

    // ── value constructors ──

    pub fn string(&self, s: &str) -> ZqValue<'cx> {
        let api = zapi_singleton();
        unsafe {
                ZqValue::from_raw(self, (api.new_string_from_utf8)(self.env, s.as_ptr(), s.len()))
            }
    }

    pub fn number(&self, n: f64) -> ZqValue<'cx> {
        let api = zapi_singleton();
        // Common fixed numbers come straight from the table constants
        // (identical handles to `new_number`, minus the FFI call).
        // Note `-0.0` is a distinct float and must fall through to
        // `new_number` to keep its sign.
        let raw = if n.is_nan() {
            api.value_nan
        } else if n == 1.0 {
            api.value_one
        } else if n == -1.0 {
            api.value_negative_one
        } else if n == 0.0 && n.is_sign_positive() {
            api.value_zero
        } else if n.is_infinite() {
            if n.is_sign_positive() {
                api.value_infinity
            } else {
                api.value_minus_infinity
            }
        } else {
            unsafe { (api.new_number)(self.env, n) }
        };
        unsafe { ZqValue::from_raw(self, raw) }
    }

    pub fn bool(&self, v: bool) -> ZqValue<'cx> {
        let api = zapi_singleton();
        // Fixed booleans come straight from the table constants.
        unsafe { ZqValue::from_raw(self, if v { api.value_true } else { api.value_false }) }
    }

    pub fn undefined(&self) -> ZqValue<'cx> {
        let api = zapi_singleton();
        unsafe { ZqValue::from_raw(self, api.value_undefined) }
    }

    pub fn null(&self) -> ZqValue<'cx> {
        let api = zapi_singleton();
        unsafe { ZqValue::from_raw(self, api.value_null) }
    }

    pub fn object(&self) -> ZqValue<'cx> {
        let api = zapi_singleton();
        unsafe { ZqValue::from_raw(self, (api.new_object)(self.env)) }
    }

    pub fn array(&self, vals: &[ZqValue<'cx>]) -> ZqValue<'cx> {
        let api = zapi_singleton();
        let arr = unsafe { (api.new_array)(self.env, vals.len()) };
        for (i, v) in vals.iter().enumerate() {
            let key = self.number(i as f64);
            unsafe { (api.array_set)(self.env, arr, key.raw(), v.raw()) };
        }
        unsafe { ZqValue::from_raw(self, arr) }
    }

    /// Create a JS `Symbol(desc)`.
    pub fn symbol(&self, desc: &str) -> ZqValue<'cx> {
        let api = zapi_singleton();
        let d = self.string(desc);
        unsafe { ZqValue::from_raw(self, (api.new_symbol)(self.env, d.raw())) }
    }

    /// The type of a JS value (`ZqType`).
    pub fn get_value_type(&self, val: ZqValue<'cx>) -> ZqType {
        let api = zapi_singleton();
        unsafe { (api.get_value_type)(self.env, val.raw()) }
    }

    /// Read a JS value as `f64` (ToNumber).
    pub fn value_to_float64(&self, val: ZqValue<'cx>) -> f64 {
        let api = zapi_singleton();
        unsafe { (api.value_to_float64)(self.env, val.raw()) }
    }

    /// Read a JS value as bool (ToBoolean).
    pub fn value_to_bool(&self, val: ZqValue<'cx>) -> bool {
        let api = zapi_singleton();
        unsafe { (api.value_to_bool)(self.env, val.raw()) != 0 }
    }

    /// Read the UTF-8 contents of a JS string value.
    pub fn read_string(&self, val: ZqRawValue) -> String {
        let api = zapi_singleton();
        // Measure-then-copy: a null buf returns the byte count; -1 = not a string.
        let len = unsafe { (api.get_string_utf8)(self.env, val, std::ptr::null_mut(), 0, 0) };
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u8; len as usize];
        unsafe { (api.get_string_utf8)(self.env, val, buf.as_mut_ptr(), len as usize, 0) };
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// Look up a property on the global object (`globalThis[name]`).
    pub fn get_global(&self, name: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = zapi_singleton();
        let key = self.string(name);
        Ok(unsafe { ZqValue::from_raw(self, (api.get_global)(self.env, key.raw())) })
    }

    /// Invoke `target` as a constructor: `new target(...args)`.
    pub fn call_constructor(
        &self,
        target: ZqValue<'cx>,
        args: &[ZqValue<'cx>],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = zapi_singleton();
        let raw_args: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let mut info = ZqCallbackInfoS {
            target: target.raw(),
            this: 0,
            args: raw_args.as_ptr(),
            argc: raw_args.len() as u32,
            thrown: 0,
            magic: 0,
        };
        let ret = unsafe { (api.call_constructor)(self.env, &mut info) };
        if ret == api.value_exception {
            Err(ZqError::new(ZqStatus::Exception, Some("call_constructor threw".into())))
        } else {
            Ok(unsafe { ZqValue::from_raw(self, ret) })
        }
    }

    /// Register a global GC trace callback for values the extension holds
    /// outside the GC heap (thread_locals, static natives). `cb` runs at every
    /// GC and should `mark()` every such value via `ZqGcTraceCtx`.
    ///
    /// Typically called from the module `init` hook.
    pub fn set_gc_trace_func(
        &self,
        cb: Option<crate::ffi::ZqGcTraceFn>,
        opaque: *mut std::ffi::c_void,
    ) -> Result<(), ZqError> {
        let api = zapi_singleton();
        let r = unsafe { (api.set_gc_trace_func)(self.env, cb, opaque) };
        if r < 0 {
            Err(ZqError::new(
                ZqStatus::Error,
                Some("set_gc_trace_func failed".into()),
            ))
        } else {
            Ok(())
        }
    }

    pub fn wrap_value(&self, raw: ZqRawValue) -> ZqValue<'cx> {
        unsafe { ZqValue::from_raw(self, raw) }
    }

    pub fn rt(&self) -> *mut std::ffi::c_void {
        if self.env.is_null() {
            return std::ptr::null_mut();
        }
        unsafe { (*self.env).rt }
    }

    pub fn scope_arena(&self) -> *mut std::ffi::c_void {
        std::ptr::null_mut()
    }

    pub fn trace_ctx(&self) -> *mut crate::ffi::ZqGcTraceCtxS {
        if self.env.is_null() {
            return std::ptr::null_mut();
        }
        unsafe { (*self.env).trace }
    }

    // ── class / instance helpers (requires GLOBAL_API) ──

    pub fn register_class(&self, def: &ZqClassDef) -> Result<i32, ZqError> {
        let api = zapi_singleton();
        let r = unsafe { (api.register_class)(self.env, def) };
        if r < 0 {
            Err(ZqError::new(
                ZqStatus::Error,
                Some("register_class failed".into()),
            ))
        } else {
            Ok(r)
        }
    }

    pub fn new_instance(
        &self,
        def: &ZqClassDef,
        opaque: *mut std::ffi::c_void,
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = zapi_singleton();
        let r = unsafe { (api.new_instance)(self.env, def, opaque) };
        if r == 0 {
            Err(ZqError::new(
                ZqStatus::Error,
                Some("new_instance failed".into()),
            ))
        } else {
            Ok(unsafe { ZqValue::from_raw(self, r) })
        }
    }

    pub fn get_native_ptr(&self, val: ZqValue<'cx>) -> *mut std::ffi::c_void {
        let api = zapi_singleton();
        unsafe { (api.get_native_ptr)(self.env, val.raw()) }
    }

    /// Calls a JS function with `this` and arguments, returning its result.
    ///
    /// The host hands every argument (numbers included) to extension callbacks
    /// as proper engine JSValues, so they are forwarded unchanged.
    pub fn call_function(
        &self,
        target: ZqValue<'cx>,
        this: ZqValue<'cx>,
        args: &[ZqValue<'cx>],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = zapi_singleton();
        let raw_args: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let mut info = ZqCallbackInfoS {
            target: target.raw(),
            this: this.raw(),
            args: raw_args.as_ptr(),
            argc: raw_args.len() as u32,
            thrown: 0,
            magic: 0,
        };
        let ret = unsafe { (api.call_function)(self.env, &mut info) };
        if ret == api.value_exception {
            Err(ZqError::new(ZqStatus::Exception, Some("call_function threw".into())))
        } else {
            Ok(unsafe { ZqValue::from_raw(self, ret) })
        }
    }

    /// Create a JS error object from a `ZqError` and return its raw handle.
    /// Used by macro trampolines to throw proper JS errors when `ZqError`
    /// carries a message but no pre-built exception handle.
    pub fn throw_error(&self, err: &ZqError) -> ZqRawValue {
        let name = match err.status() {
            ZqStatus::TypeError => "TypeError",
            ZqStatus::RangeError => "RangeError",
            _ => "Error",
        };
        let msg = err.message().unwrap_or("");

        if let Ok(ctor) = self.get_global(name) {
            let msg_val = self.string(msg);
            if let Ok(obj) = self.call_constructor(ctor, &[msg_val]) {
                return obj.raw();
            }
        }

        // Fallback: return exception sentinel
        zapi_singleton().value_exception
    }
}

impl<'cx> Clone for ZqCtx<'cx> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'cx> Copy for ZqCtx<'cx> {}

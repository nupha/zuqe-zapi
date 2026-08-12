use std::marker::PhantomData;

use crate::{
    __private::global_api,
    ffi::{ZqCallbackInfoS, ZqClassDef, ZqRawValue},
    ZqEnv, ZqError, ZqStatus, ZqValue,
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
        match global_api() {
            Some(api) => unsafe {
                ZqValue::from_raw(self, (api.string_raw)(self.env, s.as_ptr(), s.len()))
            },
            None => unsafe { ZqValue::from_raw(self, 0) },
        }
    }

    pub fn number(&self, n: f64) -> ZqValue<'cx> {
        match global_api() {
            Some(api) => unsafe { ZqValue::from_raw(self, (api.number_raw)(self.env, n)) },
            None => unsafe { ZqValue::from_raw(self, 0) },
        }
    }

    pub fn bool(&self, v: bool) -> ZqValue<'cx> {
        match global_api() {
            Some(api) => unsafe { ZqValue::from_raw(self, (api.bool_raw)(self.env, v)) },
            None => unsafe { ZqValue::from_raw(self, v as u64) },
        }
    }

    pub fn undefined(&self) -> ZqValue<'cx> {
        match global_api() {
            Some(api) => unsafe { ZqValue::from_raw(self, (api.undefined_raw)(self.env)) },
            None => unsafe { ZqValue::from_raw(self, 0) },
        }
    }

    pub fn null(&self) -> ZqValue<'cx> {
        match global_api() {
            Some(api) => unsafe { ZqValue::from_raw(self, (api.null_raw)(self.env)) },
            None => unsafe { ZqValue::from_raw(self, 0) },
        }
    }

    pub fn object(&self) -> ZqValue<'cx> {
        match global_api() {
            Some(api) => unsafe { ZqValue::from_raw(self, (api.new_object)(self.env)) },
            None => unsafe { ZqValue::from_raw(self, 0) },
        }
    }

    pub fn array(&self, vals: &[ZqValue<'cx>]) -> ZqValue<'cx> {
        let api = match global_api() {
            Some(api) => api,
            None => return unsafe { ZqValue::from_raw(self, 0) },
        };
        let arr = unsafe { (api.new_array)(self.env, vals.len()) };
        for (i, v) in vals.iter().enumerate() {
            let key = self.number(i as f64);
            unsafe { (api.array_set)(self.env, arr, key.raw(), v.raw()) };
        }
        unsafe { ZqValue::from_raw(self, arr) }
    }

    /// Create a JS `Symbol(desc)`.
    pub fn symbol(&self, desc: &str) -> ZqValue<'cx> {
        let api = match global_api() {
            Some(api) => api,
            None => return unsafe { ZqValue::from_raw(self, 0) },
        };
        let d = self.string(desc);
        unsafe { ZqValue::from_raw(self, (api.new_symbol)(self.env, d.raw())) }
    }

    /// JS `typeof`-style type code (see `ZQ_TYPE_*`).
    pub fn type_of(&self, val: ZqValue<'cx>) -> i32 {
        match global_api() {
            Some(api) => unsafe { (api.type_of)(self.env, val.raw()) },
            None => crate::ffi::ZQ_TYPE_UNDEFINED,
        }
    }

    /// Read a JS value as `f64` (ToNumber).
    pub fn number_value(&self, val: ZqValue<'cx>) -> f64 {
        match global_api() {
            Some(api) => unsafe { (api.number_value)(self.env, val.raw()) },
            None => f64::NAN,
        }
    }

    /// Read a JS value as bool (ToBoolean).
    pub fn bool_value(&self, val: ZqValue<'cx>) -> bool {
        match global_api() {
            Some(api) => unsafe { (api.bool_value)(self.env, val.raw()) != 0 },
            None => false,
        }
    }

    /// Read the UTF-8 contents of a JS string value.
    pub fn read_string(&self, val: ZqRawValue) -> String {
        let api = match global_api() {
            Some(api) => api,
            None => return String::new(),
        };
        let len = unsafe { (api.string_len)(self.env, val) };
        let mut buf = vec![0u8; len];
        if len > 0 {
            unsafe { (api.get_string_utf8)(self.env, val, buf.as_mut_ptr(), len, 0) };
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// Look up a property on the global object (`globalThis[name]`).
    pub fn get_global(&self, name: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = global_api().ok_or(ZqError::new(ZqStatus::Error, Some("no api".into())))?;
        let key = self.string(name);
        Ok(unsafe { ZqValue::from_raw(self, (api.get_global)(self.env, key.raw())) })
    }

    /// Invoke `target` as a constructor: `new target(...args)`.
    pub fn call_constructor(
        &self,
        target: ZqValue<'cx>,
        args: &[ZqValue<'cx>],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = global_api().ok_or(ZqError::new(ZqStatus::Error, Some("no api".into())))?;
        let raw_args: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let mut info = ZqCallbackInfoS {
            target: target.raw(),
            this: 0,
            args: raw_args.as_ptr(),
            argc: raw_args.len() as u32,
            ret: 0,
            threw: 0,
            magic: 0,
        };
        let ret = unsafe { (api.call_constructor)(self.env, &mut info) };
        Ok(unsafe { ZqValue::from_raw(self, ret) })
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
        let api = global_api().ok_or(ZqError::new(ZqStatus::Error, Some("no api".into())))?;
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

    pub fn rt_ptr(&self) -> *mut std::ffi::c_void {
        if self.env.is_null() {
            return std::ptr::null_mut();
        }
        unsafe { (*self.env).rt }
    }

    pub fn rt(&self) -> *mut std::ffi::c_void {
        self.rt_ptr()
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
        let api = global_api().ok_or(ZqError::new(ZqStatus::Error, Some("no api".into())))?;
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
        let api = global_api().ok_or(ZqError::new(ZqStatus::Error, Some("no api".into())))?;
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
        let api = match global_api() {
            Some(a) => a,
            None => return std::ptr::null_mut(),
        };
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
        let api = global_api().ok_or(ZqError::new(ZqStatus::Error, Some("no api".into())))?;
        let raw_args: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let mut info = ZqCallbackInfoS {
            target: target.raw(),
            this: this.raw(),
            args: raw_args.as_ptr(),
            argc: raw_args.len() as u32,
            ret: 0,
            threw: 0,
            magic: 0,
        };
        let ret = unsafe { (api.call_function)(self.env, &mut info) };
        Ok(unsafe { ZqValue::from_raw(self, ret) })
    }
}

impl<'cx> Clone for ZqCtx<'cx> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'cx> Copy for ZqCtx<'cx> {}

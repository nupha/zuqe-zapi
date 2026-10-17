// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

mod job;
mod promise;
mod state;
mod wake;

pub use {
    job::{ZqJob, ZqPoll},
    promise::{ZqExecutor, ZqPromiseCapability},
    state::ZqRuntimeState,
    wake::ZqWakeHandle,
};

use std::ffi::CString;
use std::marker::PhantomData;

use crate::{
    ZqError, ZqMember, ZqRawCtx, ZqRuntime, ZqValue,
    ffi::{ZQ_CONST_VALUE_SLOT_EXCEPTION, ZqApi, ZqCallInfoS, ZqClassDef, ZqMemberDef, ZqRawValue},
};

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
        ZqWakeHandle::from_raw_parts(
            unsafe { (api.new_waker)(self.raw) },
            api.wake,
            api.drop_waker,
        )
    }

    /// Identity of the runtime this context belongs to — the same handle passed
    /// to a module's runtime-finalize hook (`#[zq_module(finalize = ...)]`).
    ///
    /// Use [`ZqRuntime::id`] as a stable key for extension-owned per-runtime
    /// state (e.g. a thread_local map): create the entry in the module `init`
    /// hook and remove it in the `finalize` hook. Valid only inside a body /
    /// init / finalize callback.
    pub fn runtime(&self) -> &ZqRuntime {
        let api = ZqApi::singleton();
        unsafe { &*(api.get_runtime)(self.raw) }
    }

    /// Look up a property on the global object (`globalThis[name]`).
    pub fn get_global_str(&self, name: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let v = unsafe { (api.get_global_str)(self.raw, name.as_ptr(), name.len()) };
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
        let r = unsafe { (api.set_global_str)(self.raw, name.as_ptr(), name.len(), val.raw()) };
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

    /// The prototype object of a registered class. `class_id` is what
    /// [`register_class`] returned.
    ///
    /// Needed to add members to a class prototype at runtime — for a surface
    /// whose names only exist at runtime, such as a reflection table. A class
    /// whose members are all written in Rust should declare them with
    /// `#[zq_method]` instead of reaching for this.
    pub fn class_prototype(&self, class_id: i32) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let raw = unsafe { (api.class_prototype)(self.raw, class_id) };
        if raw == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(raw))
        }
    }

    /// Install members on the prototype of a registered class.
    ///
    /// For a surface whose names are not written out in Rust — a data table, a
    /// reflection table. Members written in Rust belong in `#[zq_method]`.
    ///
    /// **Names collide by overwriting.** A later member replaces an earlier
    /// one, and replaces a `#[zq_method]` of the same name. Nothing here
    /// rejects a collision; picking the right names is the caller's job.
    ///
    /// **Not atomic.** If member *k* fails, members `0..k` stay installed and
    /// this call reports the error.
    pub fn class_install_prototype_members(
        &self,
        class_id: i32,
        members: &[ZqMember<'_>],
    ) -> Result<(), ZqError> {
        let api = ZqApi::singleton();

        // The wire form wants NUL-terminated names, and the members borrow
        // theirs from the caller. NUL-terminate onto the stack: the host
        // interns each name during the call, so nothing outlives it.
        let mut names: Vec<CString> = Vec::with_capacity(members.len());
        let mut values: Vec<CString> = Vec::with_capacity(members.len());
        let mut defs: Vec<ZqMemberDef> = Vec::with_capacity(members.len());
        for member in members {
            let name = CString::new(member.name)
                .map_err(|_| self.throw_type("a member name must not contain NUL"))?;
            let value = match member.value {
                crate::ZqMemberValue::Str(text) => CString::new(text)
                    .map_err(|_| self.throw_type("a member string must not contain NUL"))?,
                _ => CString::default(),
            };
            names.push(name);
            values.push(value);
        }
        for ((member, name), value) in members.iter().zip(&names).zip(&values) {
            defs.push(crate::member::to_def(member, name.as_ptr(), value.as_ptr()));
        }

        let r = unsafe {
            (api.class_install_prototype_members)(self.raw, class_id, defs.as_ptr(), defs.len())
        };
        if r < 0 { Err(ZqError) } else { Ok(()) }
    }

    /// Install methods on a class prototype, all sharing one body.
    ///
    /// `cc.magic()` in `body` is the index of the name in `names`, so the
    /// dispatcher reads the same table the names came from and there is no
    /// second numbering to keep in step.
    pub fn class_install_prototype_methods(
        &self,
        class_id: i32,
        names: &[&str],
        body: crate::ffi::ZqBodyFn,
    ) -> Result<(), ZqError> {
        let members: Vec<ZqMember<'_>> = names
            .iter()
            .enumerate()
            .map(|(index, name)| ZqMember::method(name, body, index as i32))
            .collect();
        self.class_install_prototype_members(class_id, &members)
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
            r.string_read_utf8(*self).map(Some)
        }
    }

    /// Call a JS callback whose throw cannot propagate — an event listener, a
    /// promise reaction, a native emitter subscriber.
    ///
    /// Always leaves **no** exception on the context: on failure it takes the
    /// pending exception and reports it (debug builds print it), so a throwing
    /// listener degrades to a diagnostic instead of aborting the tick.
    ///
    /// Returns whether the callback threw. A non-callable `cb` is reported the
    /// same way, so registering a non-function is loud rather than silent.
    pub fn call_event_handler(
        &self,
        cb: impl Into<ZqValue<'cx>>,
        this: impl Into<ZqValue<'cx>>,
        args: &[ZqValue<'cx>],
        label: &str,
    ) -> bool {
        let cb = cb.into();
        if !cb.is_callable() {
            self.report_cleared_exception(label, "callback is not a function");
            return true;
        }
        match self.call_function(cb, this, args) {
            Ok(_) => false,
            Err(_) => {
                let exc = self.take_exception();
                #[cfg(debug_assertions)]
                self.print_cleared_exception(label, exc);
                #[cfg(not(debug_assertions))]
                let _ = exc;
                true
            }
        }
    }

    /// Report a taken exception under debug builds instead of silently dropping
    /// it. Event callbacks cannot propagate a throw, so this is the only
    /// available diagnostic.
    #[cfg(debug_assertions)]
    fn print_cleared_exception(&self, label: &str, exc: ZqValue<'_>) {
        if exc.is_undefined() {
            return;
        }
        let msg = exc
            .get_property_str(*self, "message")
            .ok()
            .and_then(|v| v.string_read_utf8(*self).ok())
            .unwrap_or_else(|| "<unprintable exception>".to_string());
        eprintln!("[zapi:{label}] cleared {msg}");
    }

    /// Report a problem that is not a pending exception (e.g. a non-callable
    /// callback). Always visible: it means the extension is wired wrong.
    fn report_cleared_exception(&self, label: &str, msg: &str) {
        eprintln!("[zapi:{label}] {msg}");
    }
}

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::ffi::{c_char, c_void};

/// ABI (binary layout) contract version (high 16 bits of `ZqModuleInfo.version`).
///
/// Meaning: memory layout and function pointer signatures of the `ZqApi` table and structs.
pub const ZQ_ABI_VERSION: u16 = 1;

/// API (feature) version (low 16 bits of `ZqModuleInfo.version`).
///
/// Meaning: capability tier provided by the `ZqApi` table, independent of binary layout.
pub const ZQ_API_VERSION: u16 = 1;

/// Pack ABI and API versions into a single u32: high 16 bits = ABI, low 16 bits = API.
pub const ZQ_VERSION_PACKED: u32 = ((ZQ_ABI_VERSION as u32) << 16) | (ZQ_API_VERSION as u32);

/// `ZqFieldDef.kind` — a method (`FieldDef::FUNC`).
pub const ZQ_FIELD_FUNC: u8 = 0;
/// `ZqFieldDef.kind` — an accessor (getter/setter, `FieldDef::GETSET`).
pub const ZQ_FIELD_GETSET: u8 = 1;
/// `ZqFieldDef.kind` — a class constructor (`FieldDef::CLASS`).
pub const ZQ_FIELD_CLASS: u8 = 2;
/// `ZqFieldDef.kind` — a lazily-built value (`value_fn`, `FieldDef::LAZY_VALUE`).
pub const ZQ_FIELD_VALUE: u8 = 3;
/// `ZqFieldDef.kind` — a constant string (`FieldDef::STR`).
pub const ZQ_FIELD_STR: u8 = 4;
/// `ZqFieldDef.kind` — a constant integer (`FieldDef::INT32`).
pub const ZQ_FIELD_INT: u8 = 5;
/// `ZqFieldDef.kind` — a constant float (`FieldDef::FLOAT64`).
pub const ZQ_FIELD_FLOAT: u8 = 6;
/// `ZqFieldDef.kind` — a constant boolean (`FieldDef::BOOL`).
pub const ZQ_FIELD_BOOL: u8 = 7;
/// `ZqFieldDef.kind` — `undefined` (`FieldDef::UNDEFINED`).
pub const ZQ_FIELD_UNDEFINED: u8 = 8;
// reserved (phase 2): ZQ_FIELD_OBJECT = 9, ZQ_FIELD_ALIAS = 10

// ── property flags ──
pub const ZQ_PROP_CONFIGURABLE: u32 = 1;
pub const ZQ_PROP_WRITABLE: u32 = 1 << 1;
pub const ZQ_PROP_ENUMERABLE: u32 = 1 << 2;

pub const ZQ_VALUE_TYPE_UNDEFINED: i32 = 0;
pub const ZQ_VALUE_TYPE_NULL: i32 = 1;
pub const ZQ_VALUE_TYPE_BOOL: i32 = 2;
pub const ZQ_VALUE_TYPE_NUMBER: i32 = 3;
pub const ZQ_VALUE_TYPE_STRING: i32 = 4;
pub const ZQ_VALUE_TYPE_SYMBOL: i32 = 5;
pub const ZQ_VALUE_TYPE_FUNCTION: i32 = 6;
pub const ZQ_VALUE_TYPE_OBJECT: i32 = 7;
pub const ZQ_VALUE_TYPE_BIGINT: i32 = 8;
/// The exception sentinel (`value_exception`); not a normal JS value.
pub const ZQ_VALUE_TYPE_EXCEPTION: i32 = 9;

// ── error kinds (`ZqApi::throw` / `ZqApi::new_error` `err_type`) ──
pub const ZQ_ERR_TYPE_EVAL: i32 = 0;
pub const ZQ_ERR_TYPE_RANGE: i32 = 1;
pub const ZQ_ERR_TYPE_REFERENCE: i32 = 2;
pub const ZQ_ERR_TYPE_SYNTAX: i32 = 3;
pub const ZQ_ERR_TYPE_TYPE: i32 = 4;
pub const ZQ_ERR_TYPE_URI: i32 = 5;
pub const ZQ_ERR_TYPE_INTERNAL: i32 = 6;
pub const ZQ_ERR_TYPE_AGGREGATE: i32 = 7;

pub struct ZqRawCtxS([u8; 0]);

/// Raw FFI handle for execution context.
pub type ZqRawCtx = *mut ZqRawCtxS;

/// ABI representation of an engine value (GC node handle). **Not a newtype** — a raw `u64` type alias.
pub type ZqRawValue = u64;

/// Callback info containing target / this / args / magic.
///
/// Success values do **not** go through this struct — the `body` returns the
/// result directly as the return value. On error, return `value_exception`;
/// the pending exception is obtained via `take_exception` / `exception`.
#[repr(C)]
pub struct ZqCallInfoS {
    /// The called function object.
    pub target: ZqRawValue,
    /// The `this` value.
    pub this: ZqRawValue,
    /// Argument count.
    pub argc: usize,
    /// Argument array (raw handles).
    pub argv: *const ZqRawValue,
    /// Magic value set at registration (`cc.magic()`), used for dispatching one
    /// body across multiple methods.
    pub magic: i32,
}

/// A PromiseCapability Record (ECMA-262 §27.5.1.1), filled by
/// `new_promise_capability`.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ZqPromiseCapabilityS {
    /// `[[Promise]]` — the newly created pending Promise.
    pub promise: ZqRawValue,
    /// `[[Resolve]]` — the resolve function (call it to fulfill the promise).
    pub resolve: ZqRawValue,
    /// `[[Reject]]` — the reject function (call it to reject the promise).
    pub reject: ZqRawValue,
}

/// zapi function body FFI signature.
///
/// **The return value IS the JS value**: on success, return the result handle;
/// on error, return the `value_exception` sentinel (same convention as quickjs's
/// `JS_EXCEPTION`). See `ZqApi::new_waker` / `waker_wake` / `waker_free`.
#[repr(C)]
pub struct ZqWakeHandleS([u8; 0]);

/// GC trace context — an opaque FFI handle for calling `gc_mark`.
///
/// When the host invokes a trace callback, the first argument (`*mut ZqGcTraceCtxS`)
/// is **the host's `&mut gc_lite::GcTraceCtx` transmuted** (see `zapi_gc_trace_bridge`
/// in the host). The two types are layout-compatible by construction: the host
/// transmutes `&mut GcTraceCtx → *mut ZqGcTraceCtxS` before calling the callback,
/// and the extension can reverse the transmute to call `ZqGcTraceCtx::mark()`.
///
/// Do **not** store this pointer across ticks — it is only valid during the
/// callback that receives it.
#[repr(C)]
pub struct ZqGcTraceCtxS([u8; 0]);

pub type ZqBodyFn = unsafe extern "C" fn(env: ZqRawCtx, cc: *mut ZqCallInfoS) -> ZqRawValue;

/// FFI form of the `spawn_promise` work closure: executes once on a spawned
/// thread, returns a raw pointer to `Box<T>` (T is the extension-defined result
/// type). `work_data` is consumed and freed by the work fn; it must not contain
/// GC-managed JS values.
pub type ZqWorkFn = unsafe extern "C" fn(work_data: *mut c_void) -> *mut c_void;

/// FFI form of the `spawn_promise` convert closure: executes once on the JS
/// thread, converts the work result `Box<T>` into a JS value (consuming and
/// freeing the Box). On failure, throws an exception and returns
/// `value_exception`.
pub type ZqConvertFn = unsafe extern "C" fn(env: ZqRawCtx, result: *mut c_void) -> ZqRawValue;

/// `ZqFieldDef::value_fn` signature: builds a JS value with env and writes the
/// raw handle into `*out`. Returns 0 on success, non-zero on failure. Only used
/// with `ZQ_FIELD_VALUE`.
pub type ZqFieldValueFn = unsafe extern "C" fn(env: ZqRawCtx, out: *mut ZqRawValue) -> i32;

/// Global (thread-local) GC trace callback: called by the host on every GC
/// cycle to inform the GC about JS values held in native storage (thread_local,
/// etc.) via `ZqGcTraceCtx::mark`. Same signature as the class payload
/// `ZqClassDef.gc_trace`, registered via `gc_set_trace_func`.
pub type ZqGcTraceFn = unsafe extern "C" fn(gcx: *mut ZqGcTraceCtxS, target: *mut c_void);

// ── job queue (cross-thread async bridge) ──
/// Job run callback. Runs on the JS thread once per event-loop tick.
/// Returns: 0 = Done, 1 = Block (re-queue next tick), 2 = Continue
/// (re-queue without keeping the process alive), <0 = error (treated as Done).
pub type ZqJobRunFn = unsafe extern "C" fn(data: *mut c_void, env: ZqRawCtx) -> i32;
/// Optional GC trace for JS values held inside `data`.
pub type ZqJobTraceFn = unsafe extern "C" fn(data: *mut c_void, gcx: *mut ZqGcTraceCtxS);

// ── async iterator fetch callback ──
/// FFI callback for `new_async_iterator`. Called on the JS thread to produce
/// the next value. Returns status code:
///   - 0 = Value ready (written to `*ret_val`)
///   - 1 = Done (iteration complete)
///   - 2 = Wait (value not ready, retry next tick)
///   - -1 = Error (reject the promise)
pub type ZqAsyncIterFetchFn =
    unsafe extern "C" fn(env: ZqRawCtx, opaque: *mut c_void, ret_val: *mut ZqRawValue) -> i32;

// ── constant value slot indices ──

/// Total number of constant value slots (reserved for future expansion).
pub const ZQ_CONST_VALUE_SLOT_COUNT: usize = 32;

/// Index into `ZqApi::const_values` for the `undefined` value handle.
pub const ZQ_CONST_VALUE_SLOT_UNDEFINED: usize = 0;
/// Index into `ZqApi::const_values` for the `null` value handle.
pub const ZQ_CONST_VALUE_SLOT_NULL: usize = 1;
/// Index into `ZqApi::const_values` for the boolean `true` value handle.
pub const ZQ_CONST_VALUE_SLOT_TRUE: usize = 2;
/// Index into `ZqApi::const_values` for the boolean `false` value handle.
pub const ZQ_CONST_VALUE_SLOT_FALSE: usize = 3;
/// Index into `ZqApi::const_values` for the integer `0` value handle (`+0`).
pub const ZQ_CONST_VALUE_SLOT_ZERO: usize = 4;
/// Index into `ZqApi::const_values` for the `NaN` number value handle.
pub const ZQ_CONST_VALUE_SLOT_NAN: usize = 5;
/// Index into `ZqApi::const_values` for the `+Infinity` number value handle.
pub const ZQ_CONST_VALUE_SLOT_INFINITY: usize = 6;
/// Index into `ZqApi::const_values` for the `-Infinity` number value handle.
pub const ZQ_CONST_VALUE_SLOT_MINUS_INFINITY: usize = 7;
/// Index into `ZqApi::const_values` for the exception sentinel (`JS_EXCEPTION` style).
pub const ZQ_CONST_VALUE_SLOT_EXCEPTION: usize = 8;

// ──────────────────────────────────────
// Field definitions (ABI version)
// ──────────────────────────────────────

/// Unified field descriptor: shared by module exports and class members
/// (mirrors host-side `FieldDef`).
///
/// A `ZqFieldDef` describes "define a property on an object/namespace":
///
/// * Each field in `ZqModuleInfo.exports` → an export in the module namespace;
/// * Each field in `ZqClassDef.proto_entries` / `static_entries` → a member on
///   the prototype / constructor.
///
/// `kind` determines which slots are active; remaining slots must be `None`/`null`/0.
/// Use `ZqFieldDef::func` and other const constructors to ensure consistency.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ZqFieldDef {
    /// Property/export name (NUL-terminated, valid for the process lifetime).
    pub name: *const c_char,
    /// `ZQ_FIELD_*`
    pub kind: u8,
    /// Function arity (FUNC).
    pub arity: u8,
    /// Magic for dispatch (`0` = host auto-assigns; GETSET shares one for get/set).
    pub magic: i32,
    /// `ZQ_PROP_*` property flags (see `FieldDef.prop_flags`).
    pub prop_flags: u32,

    // ── behavior slots ──
    /// FUNC: method/function body.
    pub body: Option<ZqBodyFn>,
    /// GETSET：getter。
    pub get: Option<ZqBodyFn>,
    /// GETSET：setter。
    pub set: Option<ZqBodyFn>,
    /// CLASS: class definition (host registers and publishes the constructor).
    pub class_def: *const ZqClassDef,
    /// VALUE: lazy value builder (called at evaluation time with env, writes `*out`).
    pub value_fn: Option<ZqFieldValueFn>,

    // ── constant slots ──
    /// STR: constant string (NUL-terminated).
    pub str_val: *const c_char,
    /// INT: constant integer.
    pub i64_val: i64,
    /// FLOAT: constant float.
    pub f64_val: f64,
    /// BOOL: constant boolean.
    pub u8_val: u8,
}

unsafe impl Sync for ZqFieldDef {}

impl ZqFieldDef {
    /// Method (`FieldDef::FUNC` semantics).
    pub const fn func(name: *const c_char, body: ZqBodyFn, arity: u8, magic: i32) -> Self {
        Self {
            name,
            kind: ZQ_FIELD_FUNC,
            arity,
            magic,
            prop_flags: ZQ_PROP_WRITABLE | ZQ_PROP_CONFIGURABLE,
            body: Some(body),
            get: None,
            set: None,
            class_def: std::ptr::null(),
            value_fn: None,
            str_val: std::ptr::null(),
            i64_val: 0,
            f64_val: 0.0,
            u8_val: 0,
        }
    }

    /// Accessor (`FieldDef::GETSET` semantics). At least one of get/set must be `Some`.
    pub const fn getset(
        name: *const c_char,
        get: Option<ZqBodyFn>,
        set: Option<ZqBodyFn>,
        magic: i32,
    ) -> Self {
        Self {
            name,
            kind: ZQ_FIELD_GETSET,
            arity: 0,
            magic,
            prop_flags: ZQ_PROP_CONFIGURABLE,
            body: None,
            get,
            set,
            class_def: std::ptr::null(),
            value_fn: None,
            str_val: std::ptr::null(),
            i64_val: 0,
            f64_val: 0.0,
            u8_val: 0,
        }
    }

    /// Class constructor (`FieldDef::CLASS` semantics). `name` is usually `class_def.name`.
    pub const fn class(name: *const c_char, class_def: *const ZqClassDef) -> Self {
        Self {
            name,
            kind: ZQ_FIELD_CLASS,
            arity: 0,
            magic: 0,
            prop_flags: 0,
            body: None,
            get: None,
            set: None,
            class_def,
            value_fn: None,
            str_val: std::ptr::null(),
            i64_val: 0,
            f64_val: 0.0,
            u8_val: 0,
        }
    }

    /// Lazily-built value (`FieldDef::LAZY_VALUE` semantics).
    pub const fn value(name: *const c_char, value_fn: ZqFieldValueFn) -> Self {
        Self {
            name,
            kind: ZQ_FIELD_VALUE,
            arity: 0,
            magic: 0,
            prop_flags: 0,
            body: None,
            get: None,
            set: None,
            class_def: std::ptr::null(),
            value_fn: Some(value_fn),
            str_val: std::ptr::null(),
            i64_val: 0,
            f64_val: 0.0,
            u8_val: 0,
        }
    }

    /// Constant string (`FieldDef::STR` semantics). `s` is NUL-terminated.
    pub const fn str(name: *const c_char, s: *const c_char, prop_flags: u32) -> Self {
        Self {
            name,
            kind: ZQ_FIELD_STR,
            arity: 0,
            magic: 0,
            prop_flags,
            body: None,
            get: None,
            set: None,
            class_def: std::ptr::null(),
            value_fn: None,
            str_val: s,
            i64_val: 0,
            f64_val: 0.0,
            u8_val: 0,
        }
    }

    /// Constant integer (`FieldDef::INT32` semantics).
    pub const fn int(name: *const c_char, val: i64, prop_flags: u32) -> Self {
        Self {
            name,
            kind: ZQ_FIELD_INT,
            arity: 0,
            magic: 0,
            prop_flags,
            body: None,
            get: None,
            set: None,
            class_def: std::ptr::null(),
            value_fn: None,
            str_val: std::ptr::null(),
            i64_val: val,
            f64_val: 0.0,
            u8_val: 0,
        }
    }

    /// Constant float (`FieldDef::FLOAT64` semantics).
    pub const fn float(name: *const c_char, val: f64, prop_flags: u32) -> Self {
        Self {
            name,
            kind: ZQ_FIELD_FLOAT,
            arity: 0,
            magic: 0,
            prop_flags,
            body: None,
            get: None,
            set: None,
            class_def: std::ptr::null(),
            value_fn: None,
            str_val: std::ptr::null(),
            i64_val: 0,
            f64_val: val,
            u8_val: 0,
        }
    }

    /// Constant boolean (`FieldDef::BOOL` semantics).
    pub const fn bool(name: *const c_char, val: bool, prop_flags: u32) -> Self {
        Self {
            name,
            kind: ZQ_FIELD_BOOL,
            arity: 0,
            magic: 0,
            prop_flags,
            body: None,
            get: None,
            set: None,
            class_def: std::ptr::null(),
            value_fn: None,
            str_val: std::ptr::null(),
            i64_val: 0,
            f64_val: 0.0,
            u8_val: val as u8,
        }
    }

    /// `undefined` constant (`FieldDef::UNDEFINED` semantics).
    pub const fn undefined(name: *const c_char, prop_flags: u32) -> Self {
        Self {
            name,
            kind: ZQ_FIELD_UNDEFINED,
            arity: 0,
            magic: 0,
            prop_flags,
            body: None,
            get: None,
            set: None,
            class_def: std::ptr::null(),
            value_fn: None,
            str_val: std::ptr::null(),
            i64_val: 0,
            f64_val: 0.0,
            u8_val: 0,
        }
    }

    /// Field name as `&str` (null/non-UTF-8 → `None`).
    pub fn name_str(&self) -> Option<&str> {
        if self.name.is_null() {
            None
        } else {
            unsafe { std::ffi::CStr::from_ptr(self.name) }.to_str().ok()
        }
    }
}

// ──────────────────────────────────────
// Class definitions (ABI version)
// ──────────────────────────────────────

/// ABI-compatible class definition, filled by the extension side and registered
/// via `register_class`.
///
/// The host translates `ZqClassDef` into its internal `ClassDef` and registers
/// it. `call` is the constructor body; `proto_entries` / `static_entries` are
/// the members on the prototype (instance) and constructor (static).
#[repr(C)]
pub struct ZqClassDef {
    pub name: *const c_char,
    pub parent_class: *const ZqClassDef,
    pub finalize: Option<unsafe extern "C" fn(rt: *mut c_void, opaque: *mut c_void)>,
    pub gc_trace: Option<unsafe extern "C" fn(trace_ctx: *mut ZqGcTraceCtxS, opaque: *mut c_void)>,
    pub call: Option<unsafe extern "C" fn(env: ZqRawCtx, info: *mut ZqCallInfoS) -> ZqRawValue>,
    pub has_instance: Option<
        unsafe extern "C" fn(env: ZqRawCtx, obj: ZqRawValue, class_def: *const ZqClassDef) -> i32,
    >,

    // ── v6: unified field entries (mirror `FieldDef`) ──
    /// Constructor arity (`ClassCtorDef.length`).
    pub call_arity: u8,
    /// Instance member count (prototype).
    pub proto_entries_len: usize,
    /// Instance members on the prototype (methods/accessors/constants,
    /// `ee.method` / `ee.prop`).
    pub proto_entries: *const ZqFieldDef,
    /// Static member count (constructor).
    pub static_entries_len: usize,
    /// Static members on the constructor (`Cls.method` / `Cls.prop`).
    pub static_entries: *const ZqFieldDef,
}

// Descriptor structs are immutable after registration; the contained pointers
// are never mutated through these types, so sharing them across threads is safe.
unsafe impl Sync for ZqClassDef {}

// ──────────────────────────────────────
// Module registration info
// ──────────────────────────────────────

/// Module registration info, generated by the `#[zq_module]` macro.
#[repr(C)]
#[derive(Default)]
pub struct ZqModuleInfo {
    /// Module name (purely decorative; host identity is determined by dylib URL).
    pub name: *const c_char,
    pub struct_size: u32,

    /// Packed version: high 16 bits = ABI version, low 16 bits = API version.
    /// Use `ZQ_VERSION_PACKED` to build, `abi_version()` / `api_version()` to read.
    pub version: u32,

    /// Export count.
    pub exports_len: usize,
    /// Export field array (`ZqFieldDef`, valid for the process lifetime).
    pub exports: *const ZqFieldDef,

    /// Module load initialization hook (called at evaluation time with env;
    /// non-0 = failure).
    pub init: Option<unsafe extern "C" fn(env: ZqRawCtx) -> i32>,
    /// Host-pre-filled canonical path of this dylib (NUL-terminated UTF-8).
    /// Written before `zq_module_register` is called; the extension can read it
    /// during registration or later via `zuqe_zapi::module_path()` (Node
    /// dlopen-filename pattern). Older hosts leave this null. Appended at the
    /// end to keep the `#[repr(C)]` layout stable for older extensions.
    pub path: *const c_char,
}

impl ZqModuleInfo {
    /// ABI version from packed `version` field (high 16 bits).
    pub const fn abi_version(&self) -> u16 {
        (self.version >> 16) as u16
    }

    /// API version from packed `version` field (low 16 bits).
    pub const fn api_version(&self) -> u16 {
        (self.version & 0xFFFF) as u16
    }

    pub fn exports(&self) -> &[ZqFieldDef] {
        if self.exports.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.exports, self.exports_len) }
        }
    }

    /// Host-provided dylib path as `&str` (null / non-UTF-8 → `None`).
    pub fn path_str(&self) -> Option<&str> {
        if self.path.is_null() {
            None
        } else {
            unsafe { std::ffi::CStr::from_ptr(self.path) }.to_str().ok()
        }
    }
}

// ──────────────────────────────────────
// ZqApi — C ABI function pointer table
// ──────────────────────────────────────

/// Stable C ABI function pointer table.
///
/// **Field order IS the ABI contract**: new fields may only be appended at the
/// end. During development, interface changes do **not** bump `ZQ_ABI_VERSION`
/// (host and extensions always compile from the same source); the version is
/// incremented only on ABI freeze / external release.
///
/// The host constructs and populates this table; the extension references it
/// via `*mut ZqApi`.
///
/// **ECMA-262 references** name the abstract operation and its section number.
/// Section numbers follow the current ECMA-262 editor's draft; the operation
/// name is the stable identifier. `Host-specific` marks a function with no
/// ECMA-262 counterpart (embedder/engine behaviour, not part of the language).
#[repr(C)]
pub struct ZqApi {
    // ── host-provided constant value handles ──
    // Fixed JS values the host precomputes once. Indexed by `ZQ_CONST_VALUE_SLOT_*`.
    // `undefined`/`null`/`true`/`false` are the singleton values of §6.1.1,
    // §6.1.2 and §6.1.3; `0`/`NaN`/`±Infinity` are Number values (§6.1.6).
    // Extensions read them directly instead of calling an FFI function. The
    // engine-internal sentinels (`exception`, `uninitialized`) are *not*
    // exposed: returning them to JS would corrupt the trampoline.
    pub const_values: [ZqRawValue; ZQ_CONST_VALUE_SLOT_COUNT],

    // ── exception ──
    /// Raise `val` as the pending exception (an arbitrary value, not only an
    /// Error).
    ///
    /// ECMA-262: the `throw` statement (§14.14) / ThrowCompletion. Build a typed
    /// error first with `new_error` when needed. After this call the body should
    /// return the exception sentinel.
    /// Returns nothing; the pending exception is read with `exception` /
    /// `take_exception`.
    pub throw_value: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue),

    /// Take the pending exception and clear it.
    ///
    /// Host-specific (engine exception state; the thrown object is raised by
    /// `throw_value`, §14.14). Returns the thrown value, or `undefined` if none.
    pub take_exception: unsafe extern "C" fn(env: ZqRawCtx) -> ZqRawValue,

    /// Peek at the pending exception without clearing it.
    ///
    /// Host-specific (engine exception state). Returns the thrown value, or
    /// `undefined` if none.
    pub exception: unsafe extern "C" fn(env: ZqRawCtx) -> ZqRawValue,

    // ── job queue (cross-thread async bridge) ──
    /// Queue a host job on this context's event loop: `run_fn(data, env)` runs
    /// once per tick until it's Done/Blocked/Continue.
    ///
    /// Host-specific; the JS-visible concept is a Job (§9.5).
    /// Returns 0 on success, <0 on error.
    pub add_polling_job: unsafe extern "C" fn(
        env: ZqRawCtx,
        run_fn: ZqJobRunFn,
        trace_fn: Option<ZqJobTraceFn>,
        data: *mut c_void,
    ) -> i32,

    // ── event-loop wake (cross-thread) ──
    /// Create an opaque waker bound to this context's event loop.
    /// Host-specific (event loop).
    pub new_waker: unsafe extern "C" fn(env: ZqRawCtx) -> *mut ZqWakeHandleS,

    /// Wake the parked JS event loop. Safe from any thread. Host-specific.
    pub waker_wake: unsafe extern "C" fn(h: *mut ZqWakeHandleS),

    /// Release a waker. Host-specific.
    pub waker_free: unsafe extern "C" fn(h: *mut ZqWakeHandleS),

    // ── GC ──
    /// Mark `val` as reachable during a GC trace.
    /// Host-specific (garbage collection). Returns 0 on success, <0 on error.
    pub gc_mark: unsafe extern "C" fn(gcx: *mut ZqGcTraceCtxS, val: ZqRawValue) -> i32,

    /// Register a thread-local GC trace callback.
    /// Host-specific (garbage collection). Returns 0 on success, <0 on error.
    pub gc_set_trace_func:
        unsafe extern "C" fn(env: ZqRawCtx, cb: Option<ZqGcTraceFn>, opaque: *mut c_void) -> i32,

    // ── global object ──
    /// Get `globalObject[name]`.
    ///
    /// ECMA-262: GetV (§7.3.3) / Get (§7.3.2). `name` is UTF-8 and is interned
    /// on the host (no intermediate JS string).
    /// Returns the property value, or the exception sentinel on error.
    pub get_global:
        unsafe extern "C" fn(env: ZqRawCtx, name: *const u8, name_len: usize) -> ZqRawValue,

    /// Set `globalObject[name] = val`, throwing on failure.
    /// ECMA-262: Set (§7.3.4, throw=true). `name` is UTF-8 (interned).
    /// Returns 1=set, 0=not set, -1=error.
    pub set_global: unsafe extern "C" fn(
        env: ZqRawCtx,
        name: *const u8,
        name_len: usize,
        val: ZqRawValue,
    ) -> i32,

    // ── function call ──
    /// Call `info.target(info.this, ...info.argv)`.
    ///
    /// ECMA-262: Call (§7.3.13).
    /// Returns the result value, or the exception sentinel on error.
    pub call: unsafe extern "C" fn(env: ZqRawCtx, info: *mut ZqCallInfoS) -> ZqRawValue,

    /// `new info.target(...info.argv)`.
    ///
    /// ECMA-262: Construct (§7.3.14).
    /// Returns the result value, or the exception sentinel on error.
    pub construct: unsafe extern "C" fn(env: ZqRawCtx, info: *mut ZqCallInfoS) -> ZqRawValue,

    /// `target[method](...argv)` with `target` as `this`.
    ///
    /// ECMA-262: Invoke (§7.3.20). `method` is UTF-8 and is interned on the host
    /// (no intermediate JS string); custom Symbol methods use `invoke_value`.
    /// Returns the result value, or the exception sentinel on error (including
    /// a missing or non-callable method).
    pub invoke: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        method: *const u8,
        method_len: usize,
        argc: usize,
        argv: *mut ZqRawValue,
    ) -> ZqRawValue,

    /// `target[method](...argv)` with a raw value key (custom Symbol methods).
    /// Same ECMA-262 operation as `invoke`.
    /// Returns the result value, or the exception sentinel on error.
    pub invoke_value: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        method: ZqRawValue,
        argc: usize,
        argv: *mut ZqRawValue,
    ) -> ZqRawValue,

    // ── native object / class ──
    /// Register a class definition with the host.
    /// Host-specific (embedder class registry).
    /// Returns the class ID (>= 0), or <0 on error.
    pub register_class: unsafe extern "C" fn(env: ZqRawCtx, class_def: *const ZqClassDef) -> i32,

    /// Create a new instance of `class_def` carrying `payload`.
    /// Host-specific (cf. OrdinaryCreateFromConstructor §10.1.13).
    /// Returns the new object, or the exception sentinel on error.
    pub new_instance: unsafe extern "C" fn(
        env: ZqRawCtx,
        class_def: *const ZqClassDef,
        payload: *mut c_void,
    ) -> ZqRawValue,

    /// Return the native payload pointer of a host-class instance, or null when
    /// `obj` is not one. Host-specific.
    pub get_native_ptr: unsafe extern "C" fn(env: ZqRawCtx, obj: ZqRawValue) -> *mut c_void,

    /// Create a callable backed by `run_fn` with `name` and dispatch `magic`.
    ///
    /// ECMA-262: CreateBuiltinFunction (§10.3.4).
    /// Returns the function object, or the exception sentinel on error.
    pub create_builtin_function: unsafe extern "C" fn(
        env: ZqRawCtx,
        run_fn: ZqBodyFn,
        name: ZqRawValue,
        magic: i32,
    ) -> ZqRawValue,

    // ── property access ──
    // Keys are UTF-8 `(ptr, len)` and are interned on the host, so a repeated
    // literal key is **allocation-free** (much cheaper than building a JS string
    // key). Custom Symbol keys — which UTF-8 cannot express — use the `*_value`
    // escape hatches below.
    /// Get `target[key]`.
    ///
    /// ECMA-262: Get (§7.3.2) / GetV (§7.3.3).
    /// Returns the value, or the exception sentinel on error.
    pub get: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
    ) -> ZqRawValue,

    /// Get `target[key]` with a raw value key (custom Symbol).
    /// Same ECMA-262 operation as `get`.
    /// Returns the value, or the exception sentinel on error.
    pub get_value:
        unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue, key: ZqRawValue) -> ZqRawValue,

    /// Set `target[key] = val`, throwing on failure.
    ///
    /// ECMA-262: Set (§7.3.4, throw=true).
    /// Returns 1=set, 0=not set, -1=error.
    pub set: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
        val: ZqRawValue,
    ) -> i32,

    /// Set `target[key] = val` with a raw value key (custom Symbol).
    /// Same ECMA-262 operation as `set`.
    /// Returns 1=set, 0=not set, -1=error.
    pub set_value: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: ZqRawValue,
        val: ZqRawValue,
    ) -> i32,

    /// Test `key in target`.
    ///
    /// ECMA-262: HasProperty (§7.3.11).
    /// Returns 1=present, 0=absent, -1=error.
    pub has_property: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
    ) -> i32,

    /// Test `key in target` with a raw value key (custom Symbol).
    /// Same ECMA-262 operation as `has_property`.
    /// Returns 1=present, 0=absent, -1=error.
    pub has_value: unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue, key: ZqRawValue) -> i32,

    /// `delete target[key]`.
    ///
    /// ECMA-262: [[Delete]] (§10.1.10) / OrdinaryDelete (§10.1.10.1). Contrast
    /// DeletePropertyOrThrow (§7.3.9): a non-configurable property yields 0 here
    /// rather than throwing.
    /// Returns 1=deleted, 0=not deleted, -1=error.
    pub delete_property: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
    ) -> i32,

    /// `delete target[key]` with a raw value key (custom Symbol).
    /// Same ECMA-262 operation as `delete_property`.
    /// Returns 1=deleted, 0=not deleted, -1=error.
    pub delete_value:
        unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue, key: ZqRawValue) -> i32,

    /// `LengthOfArrayLike(obj)` = `ToLength(? Get(obj, "length"))`.
    ///
    /// ECMA-262: LengthOfArrayLike (§7.3.18).
    /// Returns -1 on error, otherwise the length.
    pub length_of_array_like: unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue) -> i64,

    // ── value constructors ──
    /// Create a Number value from an f64. ECMA-262: the Number type (§6.1.6).
    pub new_number: unsafe extern "C" fn(n: f64) -> ZqRawValue,

    /// Create a String value (§6.1.4) from UTF-8 bytes. The bytes must be valid
    /// UTF-8; invalid sequences are handled per engine policy.
    /// Returns the string, or the exception sentinel on failure.
    pub new_string: unsafe extern "C" fn(env: ZqRawCtx, ptr: *const u8, len: usize) -> ZqRawValue,

    /// Create a plain object with %Object.prototype%.
    ///
    /// ECMA-262: OrdinaryObjectCreate (§10.1.12).
    /// Returns the object, or the exception sentinel on error.
    pub ordinary_object_create: unsafe extern "C" fn(env: ZqRawCtx) -> ZqRawValue,

    /// Create an Array from `elements`.
    ///
    /// ECMA-262: ArrayCreate (§10.4.2.2) + CreateDataPropertyOrThrow (§7.3.6).
    /// Returns the array, or the exception sentinel on error.
    pub create_array_from_list:
        unsafe extern "C" fn(env: ZqRawCtx, len: usize, elements: *const ZqRawValue) -> ZqRawValue,

    // ── value query / conversion ──
    /// The value's type, one of `ZQ_VALUE_TYPE_*`.
    ///
    /// Host refinement of the `typeof` operator (§13.5.3): Object and Function
    /// are split apart, and the internal exception sentinel is exposed. Not
    /// exactly `Type(x)`, which folds Function into Object.
    pub value_get_type: unsafe extern "C" fn(val: ZqRawValue) -> i32,

    /// `a === b` (no coercion).
    ///
    /// ECMA-262: IsStrictlyEqual (§7.2.14).
    /// Returns 1 (equal) / 0 (not equal); never throws.
    pub is_strictly_equal: unsafe extern "C" fn(a: ZqRawValue, b: ZqRawValue) -> i32,

    /// ToBoolean(arg).
    ///
    /// ECMA-262: ToBoolean (§7.1.2).
    /// Returns 1=true, 0=false, -1=error.
    pub to_boolean: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue) -> i32,

    /// ToInt32(arg) (wrapping modulo 2^32).
    ///
    /// ECMA-262: ToInt32 (§7.1.8). ToUint32 (§7.1.9) is `to_int32(v) as u32`.
    /// Writes `*ret` and returns 0, or -1 on error.
    pub to_int32: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue, ret: *mut i32) -> i32,

    /// Coerce to a signed 64-bit integer.
    ///
    /// ECMA-262: ToNumeric (§7.1.3). Numbers saturate; BigInts convert exactly
    /// and throw a RangeError when they do not fit in `i64`.
    /// Writes `*ret` and returns 0, or -1 on error.
    pub to_int64: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue, ret: *mut i64) -> i32,

    /// ToNumber(arg).
    ///
    /// ECMA-262: ToNumber (§7.1.4); throws for Symbol and BigInt.
    /// Writes `*ret` and returns 0, or -1 on error.
    pub to_number: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue, ret: *mut f64) -> i32,

    /// ToString(arg).
    ///
    /// ECMA-262: ToString (§7.1.19).
    /// Returns the string, or the exception sentinel on error.
    pub to_string: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue) -> ZqRawValue,

    /// Copy the UTF-8 bytes of string `s` into `buf`, starting at byte `offset`.
    ///
    /// Host-specific string encoding (ECMA-262 strings are sequences of UTF-16
    /// code units, §6.1.4).
    ///
    /// * `buf == null` (measurement mode): writes nothing and returns the number
    ///   of bytes needed from `offset` to the end;
    /// * `buf != null`: copies `min(buf_len, remaining)` bytes and returns the
    ///   number copied;
    /// * returns -1 if `s` is not a string.
    pub string_read_utf8: unsafe extern "C" fn(
        env: ZqRawCtx,
        s: ZqRawValue,
        buf: *mut u8,
        buf_len: usize,
        offset: usize,
    ) -> i32,

    // ── promise ──
    /// `NewPromiseCapability(%Promise%)`: create a pending Promise together with
    /// its `[[Resolve]]` / `[[Reject]]` functions.
    ///
    /// ECMA-262: NewPromiseCapability (§27.5.1.5) / PromiseCapability Records
    /// (§27.5.1.1). Settle the promise by calling `out.resolve` / `out.reject`
    /// (via `call`); double-settle is a silent no-op.
    /// Writes `*out` and returns 0, or -1 on error (exception sentinel).
    pub new_promise_capability:
        unsafe extern "C" fn(env: ZqRawCtx, out: *mut ZqPromiseCapabilityS) -> i32,

    // ── async iterator ──
    /// Create an async iterable/iterator driven by `fetch_fn` on the JS thread.
    ///
    /// ECMA-262: the Async Iterable Interface (§27.1.1.3) /
    /// %AsyncIteratorPrototype% (§27.1.4).
    /// Returns the iterator object, or the exception sentinel on error.
    pub new_async_iterator: unsafe extern "C" fn(
        env: ZqRawCtx,
        fetch_fn: ZqAsyncIterFetchFn,
        trace_fn: Option<ZqGcTraceFn>,
        drop_fn: Option<unsafe extern "C" fn(*mut c_void)>,
        opaque: *mut c_void,
    ) -> ZqRawValue,

    // ── instanceof ──
    /// `val instanceof class_def`'s constructor (honours `Symbol.hasInstance`).
    ///
    /// ECMA-262: InstanceofOperator (§13.10.2) / OrdinaryHasInstance (§7.3.21).
    /// Returns 1/0, or -1 on error.
    pub instanceof_operator:
        unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue, class_def: *const ZqClassDef) -> i32,

    // ── error object creation ──
    /// Create (do not throw) an Error/NativeError of `err_type` (a
    /// `ZQ_ERR_TYPE_*` constant) with `message`.
    ///
    /// ECMA-262: Error (§20.5.1.1) / NativeError (§20.5.6.1.1).
    /// Returns the error value, or the exception sentinel on failure.
    pub new_error: unsafe extern "C" fn(
        env: ZqRawCtx,
        err_type: i32,
        message: *const u8,
        message_len: usize,
    ) -> ZqRawValue,

    // ── BigInt construction ──
    // Read back through `to_int64` (handles Number and BigInt).
    /// Create a BigInt value from an i64. ECMA-262: the BigInt type (§6.1.7).
    /// Returns the BigInt, or the exception sentinel on failure.
    pub new_bigint_i64: unsafe extern "C" fn(env: ZqRawCtx, v: i64) -> ZqRawValue,

    /// Create a BigInt value from a u64. ECMA-262: the BigInt type (§6.1.7).
    /// Returns the BigInt, or the exception sentinel on failure.
    pub new_bigint_u64: unsafe extern "C" fn(env: ZqRawCtx, v: u64) -> ZqRawValue,
}

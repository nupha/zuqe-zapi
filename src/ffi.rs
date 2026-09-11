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
#[repr(C)]
pub struct ZqApi {
    // ── host-provided constant value handles ──
    // Fixed JS values the host precomputes once. Indexed by `ZQ_CONST_VALUE_SLOT_*`.
    // Extensions read them directly from the table instead of calling an FFI
    // function (`undefined`/`null`/`true`/`false` have no `*_raw` fn at all).
    // The engine-internal sentinels (`exception`, `uninitialized`) are *not*
    // exposed: returning them to JS would corrupt the trampoline.
    pub const_values: [ZqRawValue; ZQ_CONST_VALUE_SLOT_COUNT],

    // ── exception ──
    /// Throw a JS error of the given type. `err_type` is one of the
    /// `ZQ_ERR_TYPE_*` integer constants; `err_message` / `err_message_len`
    /// is the UTF-8 error message.
    pub throw: unsafe extern "C" fn(
        env: ZqRawCtx,
        err_type: i32,
        err_message: *const u8,
        err_message_len: i32,
    ),
    /// Take the current exception and return its raw error value
    /// (undefined if none). After this call the pending exception is cleared.
    pub take_exception: unsafe extern "C" fn(env: ZqRawCtx) -> ZqRawValue,
    /// Peek the current exception without consuming it (undefined if none).
    pub exception: unsafe extern "C" fn(env: ZqRawCtx) -> ZqRawValue,

    // ── job queue (cross-thread async bridge) ──
    /// Queue a polling job on this context's event loop. `run(data)` executes
    /// once per tick until it returns Done or an error. Returns 0 on success,
    /// <0 on error.
    pub add_polling_job: unsafe extern "C" fn(
        env: ZqRawCtx,
        run_fn: ZqJobRunFn,
        trace_fn: Option<ZqJobTraceFn>,
        data: *mut c_void,
    ) -> i32,

    // ── event-loop wake (cross-thread) ──
    /// Create an opaque waker on the JS thread.
    pub new_waker: unsafe extern "C" fn(env: ZqRawCtx) -> *mut ZqWakeHandleS,
    /// Wake the parked JS event loop. Safe from any thread.
    pub waker_wake: unsafe extern "C" fn(h: *mut ZqWakeHandleS),
    /// Release a waker.
    pub waker_free: unsafe extern "C" fn(h: *mut ZqWakeHandleS),

    // ── GC ──
    /// Mark `val` as reachable during GC tracing. Returns 0 on success, <0 on
    /// error.
    pub gc_mark: unsafe extern "C" fn(gcx: *mut ZqGcTraceCtxS, val: ZqRawValue) -> i32,
    /// Register a global (thread_local) GC trace callback. 0 on success, <0 on error.
    pub gc_set_trace_func:
        unsafe extern "C" fn(env: ZqRawCtx, cb: Option<ZqGcTraceFn>, opaque: *mut c_void) -> i32,

    // ── global object ──
    /// Get a property from the global object by raw value key.
    pub get_global: unsafe extern "C" fn(env: ZqRawCtx, name: ZqRawValue) -> ZqRawValue,
    /// Get a property from the global object by UTF-8 name.
    pub get_global_str:
        unsafe extern "C" fn(env: ZqRawCtx, name: *const u8, name_len: usize) -> ZqRawValue,
    /// Set a property on the global object by raw value key.
    /// The host throws in strict mode automatically.
    /// Returns 1=set, 0=not set, -1=error.
    pub set_global: unsafe extern "C" fn(env: ZqRawCtx, name: ZqRawValue, val: ZqRawValue) -> i32,
    /// Set a property on the global object by UTF-8 name.
    /// The host throws in strict mode automatically.
    /// Returns 1=set, 0=not set, -1=error.
    pub set_global_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        name: *const u8,
        name_len: usize,
        val: ZqRawValue,
    ) -> i32,

    // ── function call ──
    /// Call a JS function with `this` and arguments.
    pub call_function: unsafe extern "C" fn(env: ZqRawCtx, info: *mut ZqCallInfoS) -> ZqRawValue,
    /// Invoke `target` as a constructor (`new target(...args)`).
    pub call_constructor: unsafe extern "C" fn(env: ZqRawCtx, info: *mut ZqCallInfoS) -> ZqRawValue,
    /// Invoke `target[method](...argv)` with `target` as `this` (get-property +
    /// call in one step). Returns exception sentinel on error (including a
    /// missing or non-callable method).
    pub invoke: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        method: ZqRawValue,
        argc: usize,
        argv: *mut ZqRawValue,
    ) -> ZqRawValue,
    /// Invoke `target[method](...argv)` by UTF-8 method name (interned-atom fast
    /// path, no intermediate JS string). Returns exception sentinel on error.
    pub invoke_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        method: *const u8,
        method_len: usize,
        argc: usize,
        argv: *mut ZqRawValue,
    ) -> ZqRawValue,

    // ── native object / class ──
    /// Register a class definition with the host. Returns the class ID (>= 0)
    /// on success, < 0 on error.
    pub register_class: unsafe extern "C" fn(env: ZqRawCtx, class_def: *const ZqClassDef) -> i32,
    /// Create a new instance of a registered class with the given payload.
    pub new_instance: unsafe extern "C" fn(
        env: ZqRawCtx,
        class_def: *const ZqClassDef,
        payload: *mut c_void,
    ) -> ZqRawValue,
    /// Get the native payload pointer from a native object.
    pub get_native_ptr: unsafe extern "C" fn(env: ZqRawCtx, obj: ZqRawValue) -> *mut c_void,
    /// Create a callable JS function backed by a Rust callback.
    pub new_native_function: unsafe extern "C" fn(
        env: ZqRawCtx,
        run_fn: ZqBodyFn,
        name: ZqRawValue,
        magic: i32,
    ) -> ZqRawValue,

    // ── property access (ZqRawValue key) ──
    /// Get a property by raw value key.
    pub get_property:
        unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue, key: ZqRawValue) -> ZqRawValue,
    /// Set a property by raw value key. The host throws in strict mode
    /// automatically; the caller cannot control this.
    /// Returns 1=set, 0=not set, -1=error.
    pub set_property: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: ZqRawValue,
        val: ZqRawValue,
    ) -> i32,
    /// Check whether a property exists. Returns 1 if present, 0 if not, -1 if error.
    pub has_property:
        unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue, key: ZqRawValue) -> i32,
    /// Delete a property. Returns 1=deleted, 0=not deleted, -1=error.
    pub delete_property:
        unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue, key: ZqRawValue) -> i32,

    // ── property access (UTF-8 key, fast-path) ──
    // Same semantics as the `ZqRawValue`-keyed counterparts, but the key is
    // passed as UTF-8 `(ptr, len)`. The host resolves it to an interned atom
    // **without allocating an intermediate JS string**, so repeated access with
    // a literal key is allocation-free.
    /// Get a property by UTF-8 key. Returns exception sentinel on error.
    pub get_property_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
    ) -> ZqRawValue,
    /// Set a property by UTF-8 key. The host throws in strict mode automatically.
    /// Returns 1=set, 0=not set, -1=error.
    pub set_property_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
        val: ZqRawValue,
    ) -> i32,
    /// Check whether a property exists by UTF-8 key. 1=present, 0=absent, -1=error.
    pub has_property_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
    ) -> i32,
    /// Delete a property by UTF-8 key. 1=deleted, 0=not deleted, -1=error.
    /// The host throws in strict mode automatically.
    pub delete_property_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
    ) -> i32,

    /// Get target's length property. Returns -1 for error, otherwise the length.
    pub get_length_prop: unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue) -> i64,

    // ── value constructors ──
    /// Construct a JS number from f64.
    pub new_number: unsafe extern "C" fn(n: f64) -> ZqRawValue,
    /// Construct a JS number from i32.
    pub new_int32: unsafe extern "C" fn(n: i32) -> ZqRawValue,
    /// Construct a JS string from **UTF-8 encoded bytes** (`ptr`/`len` must be
    /// valid UTF-8; the host does not validate — invalid sequences are handled
    /// per engine policy). Returns exception sentinel on failure.
    pub new_string: unsafe extern "C" fn(env: ZqRawCtx, ptr: *const u8, len: usize) -> ZqRawValue,
    /// Construct a JS Symbol with the given description value.
    pub new_symbol: unsafe extern "C" fn(env: ZqRawCtx, desc: ZqRawValue) -> ZqRawValue,
    /// Construct a new empty JS object.
    pub new_object: unsafe extern "C" fn(env: ZqRawCtx) -> ZqRawValue,
    /// Construct a JS array from the given elements.
    pub new_array:
        unsafe extern "C" fn(env: ZqRawCtx, len: usize, elements: *const ZqRawValue) -> ZqRawValue,

    // ── value query / conversion ──
    /// The type of a JS value, see `ZQ_VALUE_TYPE_*` constants.
    pub value_get_type: unsafe extern "C" fn(val: ZqRawValue) -> i32,
    /// Coerce a JS value to a boolean (ToBoolean; 1=true, 0=false, -1=error).
    pub value_to_bool: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue) -> i32,
    /// Coerce a JS value to number (i64). return 0=ok, -1=error
    pub value_to_int64: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue, ret: *mut i64) -> i32,
    /// Coerce a JS value to number (f64). return 0=ok, -1=error
    pub value_to_float64:
        unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue, ret: *mut f64) -> i32,
    /// Coerce a JS value to JS string
    pub value_to_string: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue) -> ZqRawValue,

    /// Copy UTF-8 bytes of string `s` into `buf`, starting at `offset` bytes.
    ///
    /// * `buf == null` (measurement mode): writes no bytes, returns the number of
    ///   bytes needed from `offset` to the end, so the caller can allocate a
    ///   buffer (quickjs `JS_GetStringUtf8` convention);
    /// * `buf != null`: copies `min(buf_len, remaining bytes)` bytes, returns the
    ///   actual number copied;
    /// * returns `-1` if `s` is not a string.
    pub string_read_utf8: unsafe extern "C" fn(
        env: ZqRawCtx,
        s: ZqRawValue,
        buf: *mut u8,
        buf_len: usize,
        offset: usize,
    ) -> i32,

    /// Read the description of a JS Symbol value.
    pub symbol_get_description: unsafe extern "C" fn(env: ZqRawCtx, sym: ZqRawValue) -> ZqRawValue,

    // ── array ──
    /// Push an element to an Array. Returns -1 for error, otherwise the new length.
    pub array_push: unsafe extern "C" fn(env: ZqRawCtx, arr: ZqRawValue, val: ZqRawValue) -> i64,

    // ── promise ──
    /// Create a pending Promise. GC-rooted until settled.
    pub new_promise: unsafe extern "C" fn(env: ZqRawCtx) -> ZqRawValue,
    /// Settle a promise as fulfilled. Double-settle is a silent no-op.
    pub promise_resolve:
        unsafe extern "C" fn(env: ZqRawCtx, promise: ZqRawValue, value: ZqRawValue),
    /// Settle a promise as rejected. Double-settle is a silent no-op.
    pub promise_reject:
        unsafe extern "C" fn(env: ZqRawCtx, promise: ZqRawValue, reason: ZqRawValue),

    // ── async iterator ──
    /// Create an async iterable iterator. `fetch_fn` is called on the JS
    /// thread to produce each value. Returns `const_values[SLOT_UNDEFINED]`
    /// when done, exception sentinel on error.
    pub new_async_iterator: unsafe extern "C" fn(
        env: ZqRawCtx,
        fetch_fn: ZqAsyncIterFetchFn,
        trace_fn: Option<ZqGcTraceFn>,
        drop_fn: Option<unsafe extern "C" fn(*mut c_void)>,
        opaque: *mut c_void,
    ) -> ZqRawValue,
}

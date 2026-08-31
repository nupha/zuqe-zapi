// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::ffi::{c_char, c_void};

/// ABI (binary layout) contract version (high 16 bits of `ZqModuleInfo.version`).
///
/// Meaning: memory layout and function pointer signatures of the `ZqApi` table
/// and all `Zq*S` structs.
/// Usage: the host rejects extensions whose ABI version differs from its own.
/// Bump only when layout-breaking changes are released externally.
pub const ZQ_ABI_VERSION: u16 = 1;

/// API (feature) version (low 16 bits of `ZqModuleInfo.version`).
///
/// Meaning: capability tier provided by the `ZqApi` table, independent of binary layout.
/// Usage: the host rejects extensions whose required API tier exceeds its own.
/// Bump only when new API capabilities are added (backward-compatible, append-only).
pub const ZQ_API_VERSION: u16 = 1;

/// Pack ABI and API versions into a single u32: high 16 bits = ABI, low 16 bits = API.
pub const ZQ_VERSION_PACKED: u32 = ((ZQ_ABI_VERSION as u32) << 16) | (ZQ_API_VERSION as u32);

// ── ZqFieldDef kinds (mirror zuqe `FieldDetail`) ──

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

// ── property flags (mirror zuqe `JS_PROP_*`) ──
pub const ZQ_PROP_CONFIGURABLE: u32 = 1;
pub const ZQ_PROP_WRITABLE: u32 = 1 << 1;
pub const ZQ_PROP_ENUMERABLE: u32 = 1 << 2;

/// A JS value type (`typeof` style). `#[repr(i32)]`, cbindgen translates to C enum.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZqType {
    Undefined = 0,
    Null = 1,
    Bool = 2,
    Number = 3,
    String = 4,
    Symbol = 5,
    Function = 6,
    Object = 7,
    BigInt = 8,
    /// The exception sentinel (`value_exception`); not a normal JS value.
    Exception = 9,
}

impl ZqType {
    /// The JS `typeof` name ("undefined", "boolean", ... "object").
    pub const fn name(self) -> &'static str {
        match self {
            Self::Undefined => "undefined",
            Self::Null => "object", // JS quirk: `typeof null` is "object"
            Self::Bool => "boolean",
            Self::Number => "number",
            Self::String => "string",
            Self::Symbol => "symbol",
            Self::Function => "function",
            Self::Object => "object",
            Self::BigInt => "bigint",
            Self::Exception => "exception",
        }
    }
}

/// ABI representation of an engine value (GC node handle). **Not a newtype** — a raw `u64` type alias.
pub type ZqRawValue = u64;

/// FFI boundary environment handle (non-callback-thread path).
///
/// Raw pointer alias: the safe lifetime view is `ZqCtx<'cx>`; extension authors
/// only work with `ZqCtx`.
pub type ZqEnv = *mut ZqEnvS;

/// zapi function body FFI signature.
///
/// **The return value IS the JS value**: on success, return the result handle;
/// on error, return the `value_exception` sentinel (same convention as quickjs's
/// `JS_EXCEPTION`). To throw a specific JS value, write it into `info.thrown`
/// and then return `value_exception`, so the host throws the corresponding value.
/// See `ZqApi::wake_handle_new` / `wake_handle_wake` / `wake_handle_free`.
#[repr(C)]
pub struct ZqWakeHandleS {
    _opaque: [*mut c_void; 0],
}

/// `info.thrown`, then return `value_exception` so the host throws the corresponding value.
pub type ZapiBodyFn = unsafe extern "C" fn(env: ZqEnv, info: *mut ZqCallbackInfoS) -> ZqRawValue;

/// FFI form of the `spawn_promise` work closure: executes once on a spawned
/// thread, returns a raw pointer to `Box<T>` (T is the extension-defined result
/// type). `work_data` is consumed and freed by the work fn; it must not contain
/// GC-managed JS values.
pub type ZqWorkFn = unsafe extern "C" fn(work_data: *mut c_void) -> *mut c_void;

/// FFI form of the `spawn_promise` convert closure: executes once on the JS
/// thread, converts the work result `Box<T>` into a JS value (consuming and
/// freeing the Box). On failure, throws an exception and returns
/// `value_exception`.
pub type ZqConvertFn = unsafe extern "C" fn(env: ZqEnv, result: *mut c_void) -> ZqRawValue;

/// `ZqFieldDef::value_fn` signature: builds a JS value with env and writes the
/// raw handle into `*out`. Returns 0 on success, non-zero on failure. Only used
/// with `ZQ_FIELD_VALUE`.
pub type ZqFieldValueFn = unsafe extern "C" fn(env: ZqEnv, out: *mut ZqRawValue) -> i32;

/// Global (thread-local) GC trace callback: called by the host on every GC
/// cycle to inform the GC about JS values held in native storage (thread_local,
/// etc.) via `ZqGcTraceCtx::mark`. Same signature as the class payload
/// `ZqClassDef.gc_trace`, registered via `set_gc_trace_func`.
pub type ZqGcTraceFn = unsafe extern "C" fn(trace_ctx: *mut ZqGcTraceCtxS, opaque: *mut c_void);

// ── job queue (cross-thread async bridge) ──
/// Job run callback. Runs on the JS thread once per event-loop tick.
/// Returns: 0 = Done, 1 = Again (re-queue next tick), 2 = AgainNoHold
/// (re-queue without keeping the process alive), <0 = error (treated as Done).
pub type ZqJobRunFn = unsafe extern "C" fn(data: *mut c_void, env: ZqEnv) -> i32;
/// Optional GC trace for JS values held inside `data`.
pub type ZqJobTraceFn = unsafe extern "C" fn(data: *mut c_void, gcx: *mut ZqGcTraceCtxS);

/// A wrap of rt, ctx and gc tracer
#[repr(C)]
pub struct ZqEnvS {
    pub ctx: *mut c_void,
    #[deprecated]
    pub rt: *mut c_void,
}

/// GC trace context, used by class payload's `trace` callback.
///
/// Populated by the host with a pointer to its internal GC trace context;
/// the extension side informs the GC about held JS values via
/// `api.gc_mark(trace_ctx, value)`.
#[repr(C)]
pub struct ZqGcTraceCtxS([u8; 0]);

/// Callback info containing target / this / args / exception value / magic.
///
/// Success values do **not** go through this struct — the `body` returns the
/// result directly as the return value. Only "JS values to throw" are passed
/// via `thrown` (written before returning `value_exception` so the host reads
/// and throws it).
#[repr(C)]
pub struct ZqCallbackInfoS {
    /// The called function object.
    pub target: ZqRawValue,
    /// The `this` value.
    pub this: ZqRawValue,
    /// Argument array (raw handles).
    pub args: *const ZqRawValue,
    /// Argument count.
    pub argc: u32,
    /// JS value to throw (0 = none); written by the extension before returning
    /// `value_exception`.
    pub thrown: ZqRawValue,
    /// Magic value set at registration (`cc.magic()`), used for dispatching one
    /// body across multiple methods.
    pub magic: i32,
}

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
    pub body: Option<ZapiBodyFn>,
    /// GETSET：getter。
    pub get: Option<ZapiBodyFn>,
    /// GETSET：setter。
    pub set: Option<ZapiBodyFn>,
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
    pub const fn func(name: *const c_char, body: ZapiBodyFn, arity: u8, magic: i32) -> Self {
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
        get: Option<ZapiBodyFn>,
        set: Option<ZapiBodyFn>,
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
    pub call: Option<unsafe extern "C" fn(env: ZqEnv, info: *mut ZqCallbackInfoS) -> ZqRawValue>,
    pub has_instance: Option<
        unsafe extern "C" fn(env: ZqEnv, obj: ZqRawValue, class_def: *const ZqClassDef) -> i32,
    >,

    // ── v6: unified field entries (mirror `FieldDef`) ──
    /// Constructor arity (`ClassCtorDef.length`).
    pub call_arity: u8,
    /// Instance members on the prototype (methods/accessors/constants,
    /// `ee.method` / `ee.prop`).
    pub proto_entries: *const ZqFieldDef,
    pub proto_entries_len: usize,
    /// Static members on the constructor (`Cls.method` / `Cls.prop`).
    pub static_entries: *const ZqFieldDef,
    pub static_entries_len: usize,
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

    /// Export field array (`ZqFieldDef`, valid for the process lifetime).
    pub exports: *const ZqFieldDef,
    pub exports_len: usize,

    /// Module load initialization hook (called at evaluation time with env;
    /// non-0 = failure).
    pub init: Option<unsafe extern "C" fn(env: ZqEnv) -> i32>,
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
    // Fixed JS values the host precomputes once. They are plain constants, so
    // extensions read them directly from the table instead of calling an FFI
    // function (`undefined`/`null`/`true`/`false` have no `*_raw` fn at all).
    // The engine-internal sentinels (`exception`, `uninitialized`) are *not*
    // exposed: returning them to JS would corrupt the trampoline.
    /// The `undefined` value handle.
    pub value_undefined: ZqRawValue,
    /// The `null` value handle.
    pub value_null: ZqRawValue,
    /// The boolean `true` value handle.
    pub value_true: ZqRawValue,
    /// The boolean `false` value handle.
    pub value_false: ZqRawValue,
    /// The `NaN` number value handle.
    pub value_nan: ZqRawValue,
    /// The integer `0` number value handle (`+0`; the `-0` float is distinct).
    pub value_zero: ZqRawValue,
    /// The integer `1` number value handle.
    pub value_one: ZqRawValue,
    /// The integer `-1` number value handle.
    pub value_negative_one: ZqRawValue,
    /// The `+Infinity` number value handle.
    pub value_infinity: ZqRawValue,
    /// The `-Infinity` number value handle.
    pub value_minus_infinity: ZqRawValue,
    /// The **exception sentinel** (`JS_EXCEPTION` style): a `body` returns this
    /// handle to signal an error instead of a result value. It is not a value
    /// the extension should hand to JS.
    pub value_exception: ZqRawValue,

    pub new_number: unsafe extern "C" fn(env: ZqEnv, n: f64) -> ZqRawValue,
    /// Construct a JS string from **UTF-8 encoded bytes** (`ptr`/`len` must be
    /// valid UTF-8; the host does not validate — invalid sequences are handled
    /// per engine policy). Paired with `get_string_utf8`: reads and writes UTF-8.
    /// Returns `value_exception` on failure.
    pub new_string_utf8: unsafe extern "C" fn(env: ZqEnv, ptr: *const u8, len: usize) -> ZqRawValue,
    pub new_symbol: unsafe extern "C" fn(env: ZqEnv, desc: ZqRawValue) -> ZqRawValue,
    pub new_object: unsafe extern "C" fn(env: ZqEnv) -> ZqRawValue,
    pub new_array: unsafe extern "C" fn(env: ZqEnv, len: usize) -> ZqRawValue,

    /// Copy UTF-8 bytes of string `s` into `buf`, starting at `offset` bytes.
    ///
    /// * `buf == null` (measurement mode): writes no bytes, returns the number of
    ///   bytes needed from `offset` to the end, so the caller can allocate a
    ///   buffer (quickjs `JS_GetStringUtf8` convention);
    /// * `buf != null`: copies `min(buf_len, remaining bytes)` bytes, returns the
    ///   actual number copied;
    /// * returns `-1` if `s` is not a string.
    pub get_string_utf8: unsafe extern "C" fn(
        env: ZqEnv,
        s: ZqRawValue,
        buf: *mut u8,
        buf_len: usize,
        offset: usize,
    ) -> i32,
    pub get_symbol_description: unsafe extern "C" fn(env: ZqEnv, sym: ZqRawValue) -> ZqRawValue,

    pub set_property: unsafe extern "C" fn(
        env: ZqEnv,
        obj: ZqRawValue,
        key: ZqRawValue,
        val: ZqRawValue,
        throw: i32,
        flags: u32,
    ) -> ZqRawValue,
    pub get_property:
        unsafe extern "C" fn(env: ZqEnv, obj: ZqRawValue, key: ZqRawValue) -> ZqRawValue,
    /// Check whether a property exists on `obj`. Returns 1 if present, 0 otherwise.
    pub has_property: unsafe extern "C" fn(env: ZqEnv, obj: ZqRawValue, key: ZqRawValue) -> i32,
    /// Delete a property from `obj`. Returns 1 on success, 0 if the property
    /// does not exist or cannot be deleted. `throw` controls whether a JS
    /// exception is thrown on failure (1 = throw, 0 = silent).
    pub delete_property:
        unsafe extern "C" fn(env: ZqEnv, obj: ZqRawValue, key: ZqRawValue, throw: i32) -> i32,

    /// Append `val` to the end of array `arr`. Returns 0 on success, <0 on error.
    pub array_push: unsafe extern "C" fn(env: ZqEnv, arr: ZqRawValue, val: ZqRawValue) -> i32,
    /// Set `arr[idx] = val`. Returns 0 on success, <0 on error (out of bounds,
    /// not an array, etc.).
    pub array_set:
        unsafe extern "C" fn(env: ZqEnv, arr: ZqRawValue, idx: ZqRawValue, val: ZqRawValue) -> i32,
    pub array_len: unsafe extern "C" fn(env: ZqEnv, arr: ZqRawValue) -> ZqRawValue,

    pub get_global: unsafe extern "C" fn(env: ZqEnv, name: ZqRawValue) -> ZqRawValue,

    pub call_function: unsafe extern "C" fn(env: ZqEnv, info: *mut ZqCallbackInfoS) -> ZqRawValue,

    /// Mark `val` as reachable during GC tracing. Returns 0 on success, <0 on
    /// error. Called from `ZqGcTraceCtx::mark` to inform the GC about JS values
    /// held in native storage.
    pub gc_mark: unsafe extern "C" fn(gcx: *mut ZqGcTraceCtxS, val: ZqRawValue) -> i32,

    // ── native object / class ──
    /// Register a class definition with the host. Returns the class ID (>= 0)
    /// on success, < 0 on error. After registration, `new_instance` can create
    /// objects of this class.
    pub register_class: unsafe extern "C" fn(env: ZqEnv, class_def: *const ZqClassDef) -> i32,
    pub new_instance: unsafe extern "C" fn(
        env: ZqEnv,
        class_def: *const ZqClassDef,
        payload: *mut c_void,
    ) -> ZqRawValue,
    pub get_native_ptr: unsafe extern "C" fn(env: ZqEnv, obj: ZqRawValue) -> *mut c_void,

    // ── value introspection ──
    /// The type of a JS value (`ZqType`, a C enum across the ABI).
    pub get_value_type: unsafe extern "C" fn(env: ZqEnv, val: ZqRawValue) -> ZqType,

    /// Coerce a JS value to an f64 number (ToNumber; NaN for non-numeric).
    pub value_to_float64: unsafe extern "C" fn(env: ZqEnv, val: ZqRawValue) -> f64,
    /// Coerce a JS value to a boolean (ToBoolean; 1 if truthy, 0 otherwise).
    pub value_to_bool: unsafe extern "C" fn(env: ZqEnv, val: ZqRawValue) -> i32,
    /// Coerce a JS value to a JS string (ToString; a string handle).
    pub value_to_string: unsafe extern "C" fn(env: ZqEnv, val: ZqRawValue) -> ZqRawValue,

    /// Invoke `target` as a constructor (`new target(...args)`).
    pub call_constructor:
        unsafe extern "C" fn(env: ZqEnv, info: *mut ZqCallbackInfoS) -> ZqRawValue,

    // ── GC safety ──
    /// Register a global (thread_local) GC trace callback. Called by the
    /// extension (typically in its module `init`) so the host can keep JS values
    /// held in native storage alive across collections. 0 on success, <0 on error.
    pub set_gc_trace_func:
        unsafe extern "C" fn(env: ZqEnv, cb: Option<ZqGcTraceFn>, opaque: *mut c_void) -> i32,

    // ── job queue (cross-thread async bridge) ──
    /// Queue a polling job on this context's event loop (the fetch/fs async
    /// pattern): `run(data)` executes once per tick until it returns Done or
    /// an error. Use it to pump results from an OS worker thread into JS by
    /// calling back with `call_function`. Returns 0 on success, <0 on error.
    pub add_polling_job: unsafe extern "C" fn(
        env: ZqEnv,
        run_fn: ZqJobRunFn,
        trace_fn: Option<ZqJobTraceFn>,
        data: *mut c_void,
    ) -> i32,

    // ── promise primitives (JS thread only) ──
    /// Create a pending Promise. GC-rooted until settled, so the raw handle
    /// may be held (e.g. inside a polling job's data) across ticks.
    pub new_promise: unsafe extern "C" fn(env: ZqEnv) -> ZqRawValue,
    /// Settle fulfilled. Double-settle is a silent no-op.
    pub promise_resolve: unsafe extern "C" fn(env: ZqEnv, promise: ZqRawValue, value: ZqRawValue),
    /// Settle rejected. Double-settle is a silent no-op.
    pub promise_reject: unsafe extern "C" fn(env: ZqEnv, promise: ZqRawValue, reason: ZqRawValue),

    // ── exception hygiene ──
    /// Take the pending exception and return its raw handle
    /// (`value_undefined` if none). After this call `cur_exception` is empty;
    /// the returned handle is GC-rooted until the current scope ends.
    pub take_exception: unsafe extern "C" fn(env: ZqEnv) -> ZqRawValue,

    /// Peek the pending exception without consuming it
    /// (`value_undefined` if none).
    pub exception: unsafe extern "C" fn(env: ZqEnv) -> ZqRawValue,

    // ── event-loop wake (cross-thread) ──
    /// Opaque wake handle. Created on the JS thread via `wake_handle_new`;
    /// `wake_handle_wake` is safe from any thread (worker threads included);
    /// release with `wake_handle_free`.
    /// Wake handle: the extension's worker thread calls `wake_handle_wake` after
    /// pushing results into shared state, so the parked JS event loop wakes up
    /// immediately to process them. Without this call, a pure event-driven mode
    /// would hang indefinitely (when no fallback polling is active).
    pub wake_handle_new: unsafe extern "C" fn(env: ZqEnv) -> *mut ZqWakeHandleS,
    pub wake_handle_wake: unsafe extern "C" fn(h: *mut ZqWakeHandleS),
    pub wake_handle_free: unsafe extern "C" fn(h: *mut ZqWakeHandleS),
}

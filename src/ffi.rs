// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::ffi::{c_char, c_void};

/// ABI (binary layout) contract version (high 16 bits of `ZqModuleInfo.version`).
///
/// Meaning: memory layout and function pointer signatures of the `ZqApi` table and structs.
pub const ZQ_ABI_VERSION: u16 = 3;

/// API (feature) version (low 16 bits of `ZqModuleInfo.version`).
///
/// Meaning: capability tier provided by the `ZqApi` table, independent of binary layout.
pub const ZQ_API_VERSION: u16 = 2;

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

// ── `ZqMemberDef.kind` ────────────────────────────────────────────────────
//
// A member installed on a class *prototype*. Method and accessor kinds mirror
// `ZQ_FIELD_FUNC` / `ZQ_FIELD_GETSET`; the constant kinds mirror
// `ZQ_FIELD_STR` .. `ZQ_FIELD_UNDEFINED`.

/// `ZqMemberDef.kind` — a method (`body`).
pub const ZQ_MEMBER_METHOD: u8 = 0;
/// `ZqMemberDef.kind` — an accessor (`get` / `set`, either may be absent).
pub const ZQ_MEMBER_GETSET: u8 = 1;
/// `ZqMemberDef.kind` — a constant string (`str_val`).
pub const ZQ_MEMBER_STR: u8 = 2;
/// `ZqMemberDef.kind` — a constant integer (`i64_val`).
pub const ZQ_MEMBER_INT: u8 = 3;
/// `ZqMemberDef.kind` — a constant float (`f64_val`).
pub const ZQ_MEMBER_FLOAT: u8 = 4;
/// `ZqMemberDef.kind` — a constant boolean (`u8_val`).
pub const ZQ_MEMBER_BOOL: u8 = 5;
/// `ZqMemberDef.kind` — `undefined`.
pub const ZQ_MEMBER_UNDEFINED: u8 = 6;

impl ZqFieldDef {
    /// A neutral field, used to fill an array before its entries are written.
    pub const EMPTY: Self = Self {
        name: std::ptr::null(),
        kind: ZQ_FIELD_UNDEFINED,
        arity: 0,
        magic: 0,
        prop_flags: 0,
        body: None,
        get: None,
        set: None,
        class_def: std::ptr::null(),
        value_fn: None,
        str_val: std::ptr::null(),
        i64_val: 0,
        f64_val: 0.0,
        u8_val: 0,
    };
}

/// Concatenate two field slices into one fixed-size array.
///
/// `N` must be `declared.len() + extra.len()`; entries keep their order, so
/// `extra` lands after `declared`. The proc macro emits one `ZqFieldDef` array
/// per class and reaches for this when `#[zq_class(prototype_members = ..)]`
/// appends a table. Concatenation is what lets the host check the declared and
/// table names against each other — it refuses the class if any two collide.
pub const fn merge_fields<const N: usize>(
    declared: &[ZqFieldDef],
    extra: &[ZqFieldDef],
) -> [ZqFieldDef; N] {
    let mut out = [ZqFieldDef::EMPTY; N];
    let mut i = 0;
    while i < declared.len() {
        out[i] = declared[i];
        i += 1;
    }
    let mut j = 0;
    while j < extra.len() {
        out[declared.len() + j] = extra[j];
        j += 1;
    }
    out
}

/// One member to install on a class prototype.
///
/// The wire form of [`crate::ZqMember`], and what a `const` table of
/// `#[zq_class(prototype_members = ...)]` holds. Names are NUL-terminated C
/// strings that must stay valid for the duration of the call — the host
/// interns them, so nothing is retained.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ZqMemberDef {
    /// Property name, NUL-terminated.
    pub name: *const c_char,
    /// `ZQ_MEMBER_*`
    pub kind: u8,
    /// Function arity (`METHOD`).
    pub arity: u8,
    /// Dispatch value; `ZqCallInfo::magic()` yields it in the body.
    pub magic: i32,
    /// `ZQ_PROP_*`. Defaults to writable + configurable, so a later install of
    /// the same name replaces this one.
    pub flags: u32,
    /// `METHOD`: the body.
    pub body: Option<ZqBodyFn>,
    /// `GETSET`: the getter.
    pub get: Option<ZqBodyFn>,
    /// `GETSET`: the setter.
    pub set: Option<ZqBodyFn>,
    /// `STR`: NUL-terminated value.
    pub str_val: *const c_char,
    /// `INT`
    pub i64_val: i64,
    /// `FLOAT`
    pub f64_val: f64,
    /// `BOOL`
    pub u8_val: u8,
}

// Every pointer in here points at a literal in the extension's read-only data.
unsafe impl Sync for ZqMemberDef {}
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

// ── binary buffer kinds (`ZqApi::get_buf_type` / `new_buf_from_bytes`) ──
//
// One flat discriminant namespace for every byte container. Values are stable:
// reserve gaps for future containers rather than renumbering.
/// Not a byte container (plain object, Array, string, ...).
pub const ZQ_BUF_NONE: i32 = 0;
pub const ZQ_BUF_ARRAY_BUFFER: i32 = 1;
pub const ZQ_BUF_SHARED_ARRAY_BUFFER: i32 = 2;
pub const ZQ_BUF_DATAVIEW: i32 = 3;
pub const ZQ_BUF_TYPED_ARRAY_INT8: i32 = 16;
pub const ZQ_BUF_TYPED_ARRAY_UINT8: i32 = 17;
pub const ZQ_BUF_TYPED_ARRAY_UINT8C: i32 = 18;
pub const ZQ_BUF_TYPED_ARRAY_INT16: i32 = 19;
pub const ZQ_BUF_TYPED_ARRAY_UINT16: i32 = 20;
pub const ZQ_BUF_TYPED_ARRAY_INT32: i32 = 21;
pub const ZQ_BUF_TYPED_ARRAY_UINT32: i32 = 22;
pub const ZQ_BUF_TYPED_ARRAY_FLOAT16: i32 = 23;
pub const ZQ_BUF_TYPED_ARRAY_FLOAT32: i32 = 24;
pub const ZQ_BUF_TYPED_ARRAY_FLOAT64: i32 = 25;
pub const ZQ_BUF_TYPED_ARRAY_BIGINT64: i32 = 26;
pub const ZQ_BUF_TYPED_ARRAY_BIGUINT64: i32 = 27;

/// Opaque handle to a runtime. **Identity only**: never dereference it; compare
/// or key on the pointer itself via [`ZqRuntime::id`](crate::ZqRuntime::id). The
/// same handle is returned by `get_runtime` and passed to a module's
/// runtime-finalize hook.
pub struct ZqRuntime([u8; 0]);

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

/// `ZqPropertyDescriptorS.kind` — a data property (`value`).
pub const ZQ_PROP_KIND_DATA: i32 = 0;
/// `ZqPropertyDescriptorS.kind` — an accessor property (`get`/`set`).
pub const ZQ_PROP_KIND_ACCESSOR: i32 = 1;

/// A property descriptor (ECMA-262 §6.2.5), filled by
/// `get_own_property_descriptor`. A property's attributes are always fully
/// present once it exists, so `flags` carries the actual `ZQ_PROP_*` values.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ZqPropertyDescriptorS {
    /// `ZQ_PROP_KIND_*`.
    pub kind: i32,
    /// `ZQ_PROP_*` attributes (configurable/writable/enumerable).
    pub flags: u32,
    /// Data property: the property value.
    pub value: ZqRawValue,
    /// Accessor property: the getter (`undefined` when absent).
    pub get: ZqRawValue,
    /// Accessor property: the setter (`undefined` when absent).
    pub set: ZqRawValue,
}

/// zapi function body FFI signature.
///
/// **The return value IS the JS value**: on success, return the result handle;
/// on error, return the `value_exception` sentinel (same convention as quickjs's
/// `JS_EXCEPTION`). See `ZqApi::new_waker` / `wake` / `drop_waker`.
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

/// FFI form of the `promise_threaded` work closure: executes once on a spawned
/// thread, returns a raw pointer to `Box<T>` (T is the extension-defined result
/// type). `work_data` is consumed and freed by the work fn; it must not contain
/// GC-managed JS values.
pub type ZqWorkFn = unsafe extern "C" fn(work_data: *mut c_void) -> *mut c_void;

/// FFI form of the promise `convert` closure: executes once on the JS
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
/// Generic disposer for an extension-owned opaque pointer.
///
/// Every host API that takes ownership of extension data requires one of these,
/// so ownership is always explicit and the host is the single place that frees:
/// it calls `drop_fn(data)` exactly once when the owning object is discarded
/// (job leaves the queue, async iterator closed). Callers that pass a borrowed
/// or `'static` pointer must supply a no-op. Must not panic across the ABI.
pub type ZqDropFn = unsafe extern "C" fn(data: *mut c_void);

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

    /// Runtime-finalize hook: called once per runtime that imported this module,
    /// when that runtime is torn down (after the GC heap is gone, so no trace
    /// callback runs again). `rt` is the runtime identity handle (see
    /// [`ZqRuntime`]). Use it to release runtime-scoped native state —
    /// thread_locals, boxed GC-trace opaques. Must **not** call any JS API.
    /// `None` = no runtime-scoped state. Appended at the end to keep the layout
    /// stable for older extensions.
    pub runtime_finalize: Option<unsafe extern "C" fn(rt: *mut ZqRuntime)>,
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
///
/// **Ordering convention**: engine-internal facilities (host-specific — GC,
/// job queue, event-loop wake, native-class registry) come first, then the
/// ECMA-262 language operations, then built-in object factories and bulk
/// buffer facilities. Within each area, entries are grouped by function.
/// New fields must still only be appended at the end.
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

    // ── runtime identity ──
    /// Identity handle of the runtime this context belongs to. Host-specific.
    ///
    /// The returned pointer is the **same** handle the host passes to a
    /// module's runtime-finalize hook (`ZqModuleInfo::runtime_finalize`): use it
    /// as a stable key for extension-owned per-runtime state (thread_locals).
    /// Identity only — must not be dereferenced.
    pub get_runtime: unsafe extern "C" fn(env: ZqRawCtx) -> *mut ZqRuntime,

    // ── GC ──
    /// Mark `val` as reachable during a GC trace.
    /// Host-specific (garbage collection). Returns 0 on success, <0 on error.
    pub gc_mark: unsafe extern "C" fn(gcx: *mut ZqGcTraceCtxS, val: ZqRawValue) -> i32,

    /// Register a thread-local GC trace callback.
    /// Host-specific (garbage collection). Returns 0 on success, <0 on error.
    pub gc_set_trace_func:
        unsafe extern "C" fn(env: ZqRawCtx, cb: Option<ZqGcTraceFn>, opaque: *mut c_void) -> i32,

    // ── job queue ──
    /// Queue a host job on this context's event loop: `run_fn(opaque, env)` runs
    /// once per tick until it's Done/Blocked/Continue.
    ///
    /// Host-specific; the JS-visible concept is a Job (§9.5).
    ///
    /// The host owns `opaque` and calls `drop_fn(opaque)` exactly once when the job
    /// leaves the queue — whether it ran to completion or was discarded (queue
    /// torn down at runtime teardown, error, interrupt). After `drop_fn` fires,
    /// neither `run_fn` nor `trace_fn` is called for that `opaque` again.
    /// Returns 0 on success, <0 on error.
    pub add_job: unsafe extern "C" fn(
        env: ZqRawCtx,
        run_fn: ZqJobRunFn,
        trace_fn: Option<ZqJobTraceFn>,
        drop_fn: ZqDropFn,
        opaque: *mut c_void,
    ) -> i32,

    // ── event-loop wake (cross-thread) ──
    //
    // Contract — the safe layer (`ZqWakeHandle`) turns this into a
    // *compile-time* property, so a host only has to honour it:
    //   * `new_waker` mints exactly one token; the caller owns it;
    //   * `wake` may be called any number of times, from any thread, on a loop
    //     that is running, parked, or already gone (a cheap no-op);
    //   * `drop_waker` releases the token and must be called **exactly once**
    //     per mint — the token is *not* refcounted at the ABI level.
    //
    // Sharing a wake capability is therefore a Rust-side concern, and it has
    // exactly two spellings: `Arc<ZqWakeHandle>`, or (preferably, for
    // "N producers → one loop") cloning a `ZqWakeSender`, which keeps one
    // `Arc` internally. Never hand the same pointer to two owners.
    /// Create an opaque waker bound to this context's event loop.
    /// Host-specific (event loop). See the contract note above.
    pub new_waker: unsafe extern "C" fn(env: ZqRawCtx) -> *mut ZqWakeHandleS,

    /// Wake the parked JS event loop. Safe from any thread; may be called any
    /// number of times. Host-specific.
    pub wake: unsafe extern "C" fn(h: *mut ZqWakeHandleS),

    /// Release a waker: exactly once per `new_waker`. Host-specific.
    pub drop_waker: unsafe extern "C" fn(h: *mut ZqWakeHandleS),

    // ── native class ──
    /// Register a class definition with the host.
    /// Host-specific (embedder class registry).
    /// Returns the class ID (>= 0), or <0 on error.
    pub register_class: unsafe extern "C" fn(env: ZqRawCtx, class_def: *const ZqClassDef) -> i32,

    /// Create a new instance of `class_def` carrying the opaque `payload`,
    /// using the class's prototype. No JS constructor runs.
    ///
    /// Host-specific (native-class allocation; roughly
    /// `OrdinaryObjectCreate(proto)` plus an engine payload — **not**
    /// `OrdinaryCreateFromConstructor`: there is no `newTarget` and the
    /// constructor is never invoked).
    /// Returns the new object, or the exception sentinel on error.
    pub new_instance: unsafe extern "C" fn(
        env: ZqRawCtx,
        class_def: *const ZqClassDef,
        payload: *mut c_void,
    ) -> ZqRawValue,

    /// Return the native payload pointer of a host-class instance, or null when
    /// `obj` is not one. Host-specific.
    pub get_object_payload: unsafe extern "C" fn(obj: ZqRawValue) -> *mut c_void,

    /// Detach and return the native payload pointer of a host-class instance,
    /// setting the instance's payload to null. Ownership transfers to the
    /// caller: the engine will no longer free or trace the returned pointer.
    /// Returns null when `obj` is not a host-class instance or its payload is
    /// already null. Host-specific.
    pub take_object_payload: unsafe extern "C" fn(obj: ZqRawValue) -> *mut c_void,

    /// `instanceof` against a native class registered via `register_class`.
    /// Host-specific fast path (resolves the class's constructor directly).
    /// Returns 1/0, or -1 on error.
    pub instanceof_class:
        unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue, class_def: *const ZqClassDef) -> i32,

    // ── exception ──
    /// Raise `val` as the pending exception (an arbitrary value, not only an
    /// Error).
    ///
    /// ECMA-262: the `throw` statement (§14.14) / ThrowCompletion. Build a typed
    /// error first with `new_error` when needed. After this call the body should
    /// return the exception sentinel.
    /// Returns nothing; the pending exception is read with `exception` /
    /// `take_exception`.
    pub throw: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue),

    /// Take the pending exception and clear it.
    ///
    /// Host-specific (engine exception state; the thrown object is raised by
    /// `throw`, §14.14). Returns the thrown value, or `undefined` if none.
    pub take_exception: unsafe extern "C" fn(env: ZqRawCtx) -> ZqRawValue,

    /// Peek at the pending exception without clearing it.
    ///
    /// Host-specific (engine exception state). Returns the thrown value, or
    /// `undefined` if none.
    pub exception: unsafe extern "C" fn(env: ZqRawCtx) -> ZqRawValue,

    // ── function call ──
    /// Call `info.target(cc.this, ...cc.argv)`.
    ///
    /// ECMA-262: Call (§7.3.13).
    /// Returns the result value, or the exception sentinel on error.
    pub call: unsafe extern "C" fn(env: ZqRawCtx, cc: *mut ZqCallInfoS) -> ZqRawValue,

    /// `new info.target(...cc.argv)`.
    ///
    /// ECMA-262: Construct (§7.3.14).
    /// Returns the result value, or the exception sentinel on error.
    pub construct: unsafe extern "C" fn(env: ZqRawCtx, cc: *mut ZqCallInfoS) -> ZqRawValue,

    // ── call conveniences (get + call / construct) ──
    /// `target[method](...argv)` with `target` as `this`.
    ///
    /// ECMA-262: Invoke (§7.3.20). `method` is a raw JS value key.
    /// Returns the result value, or the exception sentinel on error (including
    /// a missing or non-callable method).
    pub invoke: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        method: ZqRawValue,
        argc: usize,
        argv: *mut ZqRawValue,
    ) -> ZqRawValue,

    /// `invoke` with a UTF-8 `(ptr, len)` method name, interned on the host.
    pub invoke_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        method: *const u8,
        method_len: usize,
        argc: usize,
        argv: *mut ZqRawValue,
    ) -> ZqRawValue,

    // ── global object ──
    /// Get `globalObject[name]` (`name` is UTF-8).
    ///
    /// ECMA-262: GetV (§7.3.3) / Get (§7.3.2). `name` is interned on the host
    /// (no intermediate JS string).
    /// Returns the property value, or the exception sentinel on error.
    pub get_global_str:
        unsafe extern "C" fn(env: ZqRawCtx, name: *const u8, name_len: usize) -> ZqRawValue,

    /// Set `globalObject[name] = val`, throwing on failure (`name` is UTF-8).
    /// ECMA-262: Set (§7.3.4, throw=true). `name` is interned on the host.
    /// Returns 1=set, 0=not set, -1=error.
    pub set_global_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        name: *const u8,
        name_len: usize,
        val: ZqRawValue,
    ) -> i32,

    // ── property access ──
    // The canonical form takes the key as a raw JS value (a String or Symbol —
    // ECMA-262 PropertyKey). The `_str` variants take a UTF-8 `(ptr, len)` key
    // that the host interns with no intermediate JS string; use them for
    // constant/literal keys.
    /// Get `target[key]`.
    ///
    /// ECMA-262: Get (§7.3.2) / GetV (§7.3.3).
    /// Returns the value, or the exception sentinel on error.
    pub get_property:
        unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue, key: ZqRawValue) -> ZqRawValue,

    /// `get_property` with a UTF-8 `(ptr, len)` key, interned on the host.
    pub get_property_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
    ) -> ZqRawValue,

    /// Set `target[key] = val`, throwing on failure.
    ///
    /// ECMA-262: Set (§7.3.4, throw=true).
    /// Returns 1=set, 0=not set, -1=error.
    pub set_property: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: ZqRawValue,
        val: ZqRawValue,
    ) -> i32,

    /// `set_property` with a UTF-8 `(ptr, len)` key.
    pub set_property_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
        val: ZqRawValue,
    ) -> i32,

    /// Test `key in target`.
    ///
    /// ECMA-262: HasProperty (§7.3.11).
    /// Returns 1=present, 0=absent, -1=error.
    pub has_property:
        unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue, key: ZqRawValue) -> i32,

    /// `has_property` with a UTF-8 `(ptr, len)` key.
    pub has_property_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
    ) -> i32,

    /// `delete target[key]`.
    ///
    /// ECMA-262: [[Delete]] (§10.1.10) / OrdinaryDelete (§10.1.10.1). Contrast
    /// DeletePropertyOrThrow (§7.3.9): a non-configurable property yields 0 here
    /// rather than throwing.
    /// Returns 1=deleted, 0=not deleted, -1=error.
    pub delete_property:
        unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue, key: ZqRawValue) -> i32,

    /// `delete_property` with a UTF-8 `(ptr, len)` key.
    pub delete_property_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
    ) -> i32,

    // ── property definition (descriptor) ──
    // Unlike `set_property`, these honour `flags` (ZQ_PROP_*) and can create accessors,
    // so they are the only way to install non-enumerable / read-only members.
    /// Define `target[key]` as a data property with attributes `flags`
    /// (`ZQ_PROP_*`).
    ///
    /// ECMA-262: DefinePropertyOrThrow (§7.3.8) / OrdinaryDefineOwnProperty
    /// (§10.1.9).
    /// Returns 1=defined, -1=error.
    pub define_property: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: ZqRawValue,
        val: ZqRawValue,
        flags: u32,
    ) -> i32,

    /// `define_property` with a UTF-8 `(ptr, len)` key.
    pub define_property_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
        val: ZqRawValue,
        flags: u32,
    ) -> i32,

    /// Define `target[key]` as an accessor property. `get`/`set` are function
    /// values or `undefined`.
    ///
    /// ECMA-262: OrdinaryDefineOwnProperty (§10.1.9) / ValidateAndApplyPropertyDescriptor (§10.1.6).
    /// Returns 1=defined, -1=error.
    pub define_accessor: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: ZqRawValue,
        get: ZqRawValue,
        set: ZqRawValue,
        flags: u32,
    ) -> i32,

    /// `define_accessor` with a UTF-8 `(ptr, len)` key.
    pub define_accessor_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
        get: ZqRawValue,
        set: ZqRawValue,
        flags: u32,
    ) -> i32,

    // ── own property introspection ──
    /// Read the own property descriptor of `target[key]`.
    ///
    /// ECMA-262: OrdinaryGetOwnProperty (§10.1.5) /
    /// ValidateAndApplyPropertyDescriptor (§10.1.6).
    /// Writes `*out` and returns 1=present, 0=absent, -1=error.
    pub get_own_property_descriptor: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: ZqRawValue,
        out: *mut ZqPropertyDescriptorS,
    ) -> i32,

    /// `get_own_property_descriptor` with a UTF-8 `(ptr, len)` key.
    /// Writes `*out` and returns 1=present, 0=absent, -1=error.
    pub get_own_property_descriptor_str: unsafe extern "C" fn(
        env: ZqRawCtx,
        target: ZqRawValue,
        key: *const u8,
        key_len: usize,
        out: *mut ZqPropertyDescriptorS,
    ) -> i32,

    // ── own property introspection convenience ──
    /// All own property keys of `target` as a fresh Array (integer keys in
    /// ascending order, then strings in insertion order, then symbols when
    /// `include_symbols` is non-zero).
    ///
    /// ECMA-262: OwnPropertyKeys (§10.1.11) / EnumerableOwnProperties (§7.3.22).
    /// Returns the Array, or the exception sentinel on error.
    pub get_own_property_names:
        unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue, include_symbols: i32) -> ZqRawValue,

    // ── value query / conversion ──
    /// The value's type, one of `ZQ_VALUE_TYPE_*`.
    ///
    /// Host refinement of the `typeof` operator (§13.5.3): Object and Function
    /// are split apart, and the internal exception sentinel is exposed. Not
    /// exactly `Type(x)`, which folds Function into Object.
    pub get_type_of: unsafe extern "C" fn(val: ZqRawValue) -> i32,

    /// `IsArray(val)` (honours Proxy).
    ///
    /// ECMA-262: IsArray (§7.2.2).
    /// Returns 1=array, 0=not, -1=error.
    pub is_array: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue) -> i32,

    /// `IsCallable(val)`.
    ///
    /// ECMA-262: IsCallable (§7.2.4).
    /// Returns 1=callable, 0=not; never throws.
    pub is_callable: unsafe extern "C" fn(val: ZqRawValue) -> i32,

    /// Whether `val` has a `[[Construct]]` internal method.
    /// Host refinement of IsConstructor (§7.2.4).
    /// Returns 1=constructor, 0=not; never throws.
    pub is_constructor: unsafe extern "C" fn(val: ZqRawValue) -> i32,

    /// `a === b` (no coercion).
    ///
    /// ECMA-262: IsStrictlyEqual (§7.2.14).
    /// Returns 1 (equal) / 0 (not equal); never throws.
    pub is_strictly_equal: unsafe extern "C" fn(a: ZqRawValue, b: ZqRawValue) -> i32,

    /// `val instanceof target` for an arbitrary constructor value (honours
    /// `Symbol.hasInstance`).
    ///
    /// ECMA-262: InstanceofOperator (§13.10.2) / OrdinaryHasInstance (§7.3.21).
    /// Returns 1/0, or -1 on error.
    pub instanceof: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue, target: ZqRawValue) -> i32,

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

    // ── value constructors (engine-core factories) ──
    // Engine-core values only: raw scalar primitives (Number/String/Symbol,
    // unboxed in-engine) and the object-model building blocks (Object/Array/
    // Function). BigInt (a language type, but boxed in-engine) and built-in
    // objects (Error, ...) live in later sections.
    /// Create a Number value from an f64. ECMA-262: the Number type (§6.1.6).
    pub new_number: unsafe extern "C" fn(n: f64) -> ZqRawValue,

    /// Create a String value (§6.1.4) from UTF-8 bytes. The bytes must be valid
    /// UTF-8; invalid sequences are handled per engine policy.
    /// Returns the string, or the exception sentinel on failure.
    pub new_string: unsafe extern "C" fn(env: ZqRawCtx, ptr: *const u8, len: usize) -> ZqRawValue,

    /// Create a Symbol value with description `desc` (UTF-8).
    ///
    /// ECMA-262: Symbol (§20.4). Returns the symbol, or the exception sentinel
    /// on failure.
    pub new_symbol: unsafe extern "C" fn(env: ZqRawCtx, ptr: *const u8, len: usize) -> ZqRawValue,

    /// Create a plain object with %Object.prototype%.
    ///
    /// ECMA-262: OrdinaryObjectCreate (§10.1.12).
    /// Returns the object, or the exception sentinel on error.
    pub new_object: unsafe extern "C" fn(env: ZqRawCtx) -> ZqRawValue,

    /// Create a plain object with prototype `proto` (a null prototype when
    /// `proto` is `null` or `undefined`).
    ///
    /// ECMA-262: OrdinaryObjectCreate (§10.1.12).
    /// Returns the object, or the exception sentinel on error.
    pub new_object_with_proto: unsafe extern "C" fn(env: ZqRawCtx, proto: ZqRawValue) -> ZqRawValue,

    /// Create an Array from `elements`.
    ///
    /// ECMA-262: ArrayCreate (§10.4.2.2) + CreateDataPropertyOrThrow (§7.3.6).
    /// Returns the array, or the exception sentinel on error.
    pub new_array:
        unsafe extern "C" fn(env: ZqRawCtx, len: usize, elements: *const ZqRawValue) -> ZqRawValue,

    /// Create a callable backed by `run_fn` with `name` and dispatch `magic`.
    ///
    /// ECMA-262: CreateBuiltinFunction (§10.3.4).
    /// Returns the function object, or the exception sentinel on error.
    pub new_builtin_function: unsafe extern "C" fn(
        env: ZqRawCtx,
        run_fn: ZqBodyFn,
        name: ZqRawValue,
        magic: i32,
    ) -> ZqRawValue,

    // ── built-in object factories ──
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

    // ── BigInt (language type §6.1.7, boxed in-engine) ──
    /// Create a BigInt value from an i64. ECMA-262: the BigInt type (§6.1.7).
    /// Returns the BigInt, or the exception sentinel on failure.
    pub new_bigint_i64: unsafe extern "C" fn(env: ZqRawCtx, v: i64) -> ZqRawValue,

    /// Create a BigInt value from a u64. ECMA-262: the BigInt type (§6.1.7).
    /// Returns the BigInt, or the exception sentinel on failure.
    pub new_bigint_u64: unsafe extern "C" fn(env: ZqRawCtx, v: u64) -> ZqRawValue,

    /// `LengthOfArrayLike(obj)` = `ToLength(? Get(obj, "length"))`.
    ///
    /// ECMA-262: LengthOfArrayLike (§7.3.18).
    /// Returns -1 on error, otherwise the length.
    pub get_length: unsafe extern "C" fn(env: ZqRawCtx, target: ZqRawValue) -> i64,

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
    pub read_string_utf8: unsafe extern "C" fn(
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
    ///
    /// The host owns `opaque` and calls `drop_fn(opaque)` exactly once when the
    /// iterator is closed/dropped. After that, `fetch_fn`/`trace_fn` are not
    /// called for it again.
    /// Returns the iterator object, or the exception sentinel on error.
    pub new_async_iterator: unsafe extern "C" fn(
        env: ZqRawCtx,
        fetch_fn: ZqAsyncIterFetchFn,
        trace_fn: Option<ZqGcTraceFn>,
        drop_fn: ZqDropFn,
        opaque: *mut c_void,
    ) -> ZqRawValue,

    // ── binary buffers (bulk, O(1) ABI for O(N) data) ──
    /// Create a binary buffer by copying `len` bytes from `ptr`. `kind` is a
    /// `ZQ_BUF_*` discriminant selecting the resulting class (ArrayBuffer,
    /// SharedArrayBuffer, DataView, or a typed array). The host copies into its
    /// own allocator, so no ownership crosses the ABI. For typed arrays `len`
    /// must be a multiple of the element size, otherwise a `RangeError` is
    /// thrown. Returns the value, or the exception sentinel on failure.
    pub new_buf_from_bytes:
        unsafe extern "C" fn(env: ZqRawCtx, kind: i32, ptr: *const u8, len: usize) -> ZqRawValue,

    /// Identify the binary buffer kind of `val`.
    ///
    /// Returns a `ZQ_BUF_*` discriminant (`ZQ_BUF_NONE` for anything that is not
    /// an ArrayBuffer / SharedArrayBuffer / DataView / typed array). Never throws.
    pub get_buf_type: unsafe extern "C" fn(env: ZqRawCtx, val: ZqRawValue) -> i32,

    /// Read raw bytes of a binary buffer (host byte order). Mirrors
    /// `read_string_utf8` (measure-then-copy):
    /// * `dst == null`: measurement mode — return the total byte length from
    ///   `offset` to the end, writing nothing;
    /// * `dst != null`: copy `min(dst_len, total - offset)` bytes and return the
    ///   number copied;
    /// * `val` is not a binary buffer: throws a `TypeError` and returns -1.
    ///
    /// A detached or out-of-bounds view reports length 0. `offset` beyond the
    /// view length copies nothing. Reading a SharedArrayBuffer is a snapshot
    /// copy and may observe concurrent writes from another agent.
    pub read_buf_bytes: unsafe extern "C" fn(
        env: ZqRawCtx,
        val: ZqRawValue,
        dst: *mut u8,
        dst_len: usize,
        offset: usize,
    ) -> i64,

    //
    // --- ABI 3 ---
    //
    /// The prototype object of a class previously passed to `register_class`.
    /// This is how an extension reaches its own class prototype without
    /// allocating a probe instance and asking `Object.getPrototypeOf`.
    ///
    /// Returns the `exception` sentinel when `class_id` is not registered.
    pub class_prototype: unsafe extern "C" fn(env: ZqRawCtx, class_id: i32) -> ZqRawValue,

    /// Installs members on the prototype of a registered class.
    ///
    /// Every name must be new: a name already on the prototype, or repeated
    /// within `members`, throws a `TypeError` naming it. Nothing is replaced.
    /// Not atomic — entries before a failing one stay installed — which is why
    /// this is the fallback and `#[zq_class(prototype_members = ..)]` is not:
    /// a declared table is installed while the class is set up, so it can
    /// refuse the whole class.
    pub class_install_prototype_members: unsafe extern "C" fn(
        env: ZqRawCtx,
        class_id: i32,
        members: *const ZqMemberDef,
        len: usize,
    ) -> i32,
}

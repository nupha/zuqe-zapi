use std::ffi::{c_char, c_void};

pub const ZQ_ABI_VERSION: u32 = 4;
pub const ZQ_API_VERSION: u32 = 2;

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

pub const ZQ_TYPE_UNDEFINED: i32 = 0;
pub const ZQ_TYPE_NULL: i32 = 1;
pub const ZQ_TYPE_BOOL: i32 = 2;
pub const ZQ_TYPE_NUMBER: i32 = 3;
pub const ZQ_TYPE_STRING: i32 = 4;
pub const ZQ_TYPE_SYMBOL: i32 = 5;
pub const ZQ_TYPE_FUNCTION: i32 = 6;
pub const ZQ_TYPE_OBJECT: i32 = 7;

/// 引擎值的 ABI 表示（GC 节点句柄）。**不是 newtype**，是裸 `u64` type alias。
pub type ZqRawValue = u64;

/// FFI 边界用的环境句柄（非回调线程路径）。
///
/// 裸指针别名：安全的 lifetime 视图见 `ZqCtx<'cx>`，扩展作者只碰 `ZqCtx`。
pub type ZqEnv = *mut ZqEnvS;

/// zapi 函数体 FFI 签名。
pub type ZapiBodyFn = unsafe extern "C" fn(env: ZqEnv, info: *mut ZqCallbackInfoS) -> ZqRawValue;

/// `ZqFieldDef::value_fn` 签名：用 env 构建一个 JS 值，把 raw handle 写进 `*out`。
/// 返回 0 成功、非 0 失败。仅用于 `ZQ_FIELD_VALUE`。
pub type ZqExportValueFn = unsafe extern "C" fn(env: ZqEnv, out: *mut ZqRawValue) -> i32;

/// 全局（线程态）GC trace 回调：每次 GC 时被 host 调用，把扩展在 native 存储
/// （thread_local 等）里持有的 JS 值经 `ZqGcTraceCtx::mark` 告知 GC。
/// 与 class payload 的 `ZqClassDef.gc_trace` 同签名，通过 `set_gc_trace_func` 注册。
pub type ZqGcTraceFn = unsafe extern "C" fn(trace_ctx: *mut ZqGcTraceCtxS, opaque: *mut c_void);

// ──────────────────────────────────────
// 不透明结构体前向声明（repr(C) 但字段不对外导出）
// ──────────────────────────────────────

/// 回调/执行环境句柄。host 端自行定义兼容布局，扩展侧仅作为 `*mut` 传递。
#[repr(C)]
pub struct ZqEnvS {
    /// 指向当前 ContextImpl 的指针
    pub ctx: *mut c_void,
    /// 当前 GC trace 上下文（class payload 的 trace 回调使用）
    pub trace: *mut ZqGcTraceCtxS,
    /// 运行时指针，供 raw 路径（poller 线程）开 scope
    pub rt: *mut c_void,
}

/// GC 追踪上下文，class payload 的 `trace` 回调使用。
///
/// `inner` 由 host 端填充为其内部 GC trace 上下文的指针；
/// 扩展侧通过 `api.gc_add_trace(env, trace_ctx, value)` 将持有的
/// JS 值告知 GC。
#[repr(C)]
pub struct ZqGcTraceCtxS {
    pub inner: *mut c_void,
}

/// 回调信息，包含 target / this / args / return / exception。
#[repr(C)]
pub struct ZqCallbackInfoS {
    /// 被调用函数对象
    pub target: ZqRawValue,
    /// this 值
    pub this: ZqRawValue,
    /// 参数数组（raw handles）
    pub args: *const ZqRawValue,
    /// 参数个数
    pub argc: u32,
    /// 返回值（set_return 写入）
    pub ret: ZqRawValue,
    /// 异常哨兵（0 = 无）
    pub threw: ZqRawValue,

    // ── v4: magic ──
    /// 注册时指定的 magic（`cc.magic()`），用于一个 body 服务多个方法的分发。
    pub magic: i32,
}

// ──────────────────────────────────────
// 字段定义（ABI 版本）
// ──────────────────────────────────────

/// 统一字段描述：模块导出和类成员共用（对应 host 侧 `FieldDef`）。
///
/// 一个 `ZqFieldDef` 描述"在某个对象/命名空间上定义一个属性"：
///
/// * `ZqModuleInfo.exports` 里的每个字段 → module 命名空间的一条导出；
/// * `ZqClassDef.proto_entries` / `static_entries` 里的每个字段 → 原型 /
///   构造器上的一个成员。
///
/// `kind` 决定使用哪些槽位，其余槽位须为 `None`/`null`/0。用 `ZqFieldDef::func`
/// 等 const 构造器填写即可保证一致。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ZqFieldDef {
    /// 属性/导出名（NUL 结尾，进程生命周期有效）。
    pub name: *const c_char,
    /// `ZQ_FIELD_*`
    pub kind: u8,
    /// 函数形参数量（FUNC）。
    pub arity: u8,
    /// 分发用的 magic（`0` = host 自动分配；GETSET 的 get/set 共享一个）。
    pub magic: i32,
    /// `ZQ_PROP_*` 属性标志（对照 `FieldDef.prop_flags`）。
    pub prop_flags: u32,

    // ── 行为槽 ──
    /// FUNC：方法/函数体。
    pub body: Option<ZapiBodyFn>,
    /// GETSET：getter。
    pub get: Option<ZapiBodyFn>,
    /// GETSET：setter。
    pub set: Option<ZapiBodyFn>,
    /// CLASS：类定义（host 据此注册并发布构造器）。
    pub class_def: *const ZqClassDef,
    /// VALUE：惰性值构建器（求值期带 env 调用，写 `*out`）。
    pub value_fn: Option<ZqExportValueFn>,

    // ── 常量槽 ──
    /// STR：常量字符串（NUL 结尾）。
    pub str_val: *const c_char,
    /// INT：常量整数。
    pub i64_val: i64,
    /// FLOAT：常量浮点。
    pub f64_val: f64,
    /// BOOL：常量布尔。
    pub u8_val: u8,
}

unsafe impl Sync for ZqFieldDef {}

impl ZqFieldDef {
    /// 方法（`FieldDef::FUNC` 语义）。
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

    /// 访问器（`FieldDef::GETSET` 语义）。get/set 至少一个为 `Some`。
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

    /// 类构造器（`FieldDef::CLASS` 语义）。`name` 通常取 `class_def.name`。
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

    /// 惰性构建的值（`FieldDef::LAZY_VALUE` 语义）。
    pub const fn value(name: *const c_char, value_fn: ZqExportValueFn) -> Self {
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

    /// 常量字符串（`FieldDef::STR` 语义）。`s` 为 NUL 结尾。
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

    /// 常量整数（`FieldDef::INT32` 语义）。
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

    /// 常量浮点（`FieldDef::FLOAT64` 语义）。
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

    /// 常量布尔（`FieldDef::BOOL` 语义）。
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

    /// `undefined` 常量（`FieldDef::UNDEFINED` 语义）。
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

    /// 字段名作为 `&str`（null/非 UTF-8 → `None`）。
    pub fn name_str(&self) -> Option<&str> {
        if self.name.is_null() {
            None
        } else {
            unsafe { std::ffi::CStr::from_ptr(self.name) }.to_str().ok()
        }
    }
}

// ──────────────────────────────────────
// 类定义（ABI 版本）
// ──────────────────────────────────────

/// ABI 兼容的类定义，扩展侧填写后通过 `register_class` 注册。
///
/// host 端将 `ZqClassDef` 翻译为内部 `ClassDef` 并注册。`call` 是构造器主体，
/// `proto_entries` / `static_entries` 是原型（实例）与构造器（静态）上的成员。
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
    /// 构造器形参数量（`ClassCtorDef.length`）。
    pub call_arity: u8,
    /// 原型上的实例成员（方法/访问器/常量，`ee.method` / `ee.prop`）。
    pub proto_entries: *const ZqFieldDef,
    pub proto_entries_len: usize,
    /// 构造器上的静态成员（`Cls.method` / `Cls.prop`）。
    pub static_entries: *const ZqFieldDef,
    pub static_entries_len: usize,
}

// Descriptor structs are immutable after registration; the contained pointers
// are never mutated through these types, so sharing them across threads is safe.
unsafe impl Sync for ZqClassDef {}

// ──────────────────────────────────────
// 模块注册信息
// ──────────────────────────────────────

/// 模块注册信息，由 `#[zq_module]` 宏生成。
#[repr(C)]
#[derive(Default)]
pub struct ZqModuleInfo {
    /// 模块名（纯装饰，host 身份由 dylib URL 决定）
    pub name: *const c_char,
    pub struct_size: u32,
    pub abi_version: u32,
    pub min_api_version: u32,
    /// 导出字段数组（`ZqFieldDef`，进程生命周期有效）
    pub exports: *const ZqFieldDef,
    pub exports_len: usize,
    /// 模块加载初始化钩子（求值期带 env 调用；非 0 = 失败）
    pub init: Option<unsafe extern "C" fn(env: ZqEnv) -> i32>,
}

impl ZqModuleInfo {
    pub fn exports(&self) -> &[ZqFieldDef] {
        if self.exports.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.exports, self.exports_len) }
        }
    }
}

// ──────────────────────────────────────
// ZqApi — C ABI 函数指针表
// ──────────────────────────────────────

/// 稳定的 C ABI 函数指针表。
///
/// **字段顺序即 ABI 契约**：新增字段只能追加到末尾，且必须 bump `ZQ_ABI_VERSION`。
///
/// host 端构造该表并填充全部函数指针；扩展侧通过 `*mut ZqApi` 引用它。
#[repr(C)]
pub struct ZqApi {
    // ── 版本校验 ──
    pub version: u32,

    // ── 值构造 (primitives) ──
    pub undefined_raw: unsafe extern "C" fn(env: ZqEnv) -> ZqRawValue,
    pub null_raw: unsafe extern "C" fn(env: ZqEnv) -> ZqRawValue,
    pub bool_raw: unsafe extern "C" fn(env: ZqEnv, v: bool) -> ZqRawValue,
    pub number_raw: unsafe extern "C" fn(env: ZqEnv, v: f64) -> ZqRawValue,
    pub string_raw: unsafe extern "C" fn(env: ZqEnv, ptr: *const u8, len: usize) -> ZqRawValue,
    pub new_symbol: unsafe extern "C" fn(env: ZqEnv, desc: ZqRawValue) -> ZqRawValue,
    pub get_symbol_description: unsafe extern "C" fn(env: ZqEnv, sym: ZqRawValue) -> ZqRawValue,

    // ── 字符串读写 ──
    pub string_len: unsafe extern "C" fn(env: ZqEnv, s: ZqRawValue) -> usize,
    pub get_string_utf8: unsafe extern "C" fn(
        env: ZqEnv,
        s: ZqRawValue,
        buf: *mut u8,
        buf_len: usize,
        offset: usize,
    ) -> i32,

    // ── 对象 / 数组 / 属性 ──
    pub new_object: unsafe extern "C" fn(env: ZqEnv) -> ZqRawValue,
    pub new_array: unsafe extern "C" fn(env: ZqEnv, len: usize) -> ZqRawValue,
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
    pub has_property: unsafe extern "C" fn(env: ZqEnv, obj: ZqRawValue, key: ZqRawValue) -> i32,
    pub delete_property:
        unsafe extern "C" fn(env: ZqEnv, obj: ZqRawValue, key: ZqRawValue, throw: i32) -> i32,
    pub array_push: unsafe extern "C" fn(env: ZqEnv, arr: ZqRawValue, val: ZqRawValue) -> i32,
    pub array_set:
        unsafe extern "C" fn(env: ZqEnv, arr: ZqRawValue, idx: ZqRawValue, val: ZqRawValue) -> i32,
    pub array_len: unsafe extern "C" fn(env: ZqEnv, arr: ZqRawValue) -> ZqRawValue,
    pub get_global: unsafe extern "C" fn(env: ZqEnv, name: ZqRawValue) -> ZqRawValue,

    // ── 函数调用 ──
    pub call_function: unsafe extern "C" fn(env: ZqEnv, info: *mut ZqCallbackInfoS) -> ZqRawValue,

    // ── GC 安全 ──
    pub gc_add_trace:
        unsafe extern "C" fn(env: ZqEnv, trace_ctx: *mut ZqGcTraceCtxS, val: ZqRawValue) -> i32,

    // ── native object / class ──
    pub register_class: unsafe extern "C" fn(env: ZqEnv, class_def: *const ZqClassDef) -> i32,
    pub new_instance: unsafe extern "C" fn(
        env: ZqEnv,
        class_def: *const ZqClassDef,
        payload: *mut c_void,
    ) -> ZqRawValue,
    pub get_native_ptr: unsafe extern "C" fn(env: ZqEnv, obj: ZqRawValue) -> *mut c_void,

    // ── value introspection & construction ──
    /// JS `typeof`-style type code (see `ZQ_TYPE_*`).
    pub type_of: unsafe extern "C" fn(env: ZqEnv, val: ZqRawValue) -> i32,
    /// ToNumber: read a JS value as f64 (NaN for non-numeric values).
    pub number_value: unsafe extern "C" fn(env: ZqEnv, val: ZqRawValue) -> f64,
    /// ToBoolean: 1 if truthy, 0 otherwise.
    pub bool_value: unsafe extern "C" fn(env: ZqEnv, val: ZqRawValue) -> i32,
    /// ToString: returns a JS string value.
    pub string_value: unsafe extern "C" fn(env: ZqEnv, val: ZqRawValue) -> ZqRawValue,
    /// Invoke `target` as a constructor (`new target(...args)`).
    pub call_constructor:
        unsafe extern "C" fn(env: ZqEnv, info: *mut ZqCallbackInfoS) -> ZqRawValue,

    // ── GC safety ──
    /// Register a global (thread_local) GC trace callback. Called by the
    /// extension (typically in its module `init`) so the host can keep JS values
    /// held in native storage alive across collections. 0 on success, <0 on error.
    pub set_gc_trace_func: unsafe extern "C" fn(
        env: ZqEnv,
        cb: Option<ZqGcTraceFn>,
        opaque: *mut c_void,
    ) -> i32,
}

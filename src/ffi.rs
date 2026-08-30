// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::ffi::{c_char, c_void};

/// ABI（二进制布局）契约版本。
///
/// 含义：`ZqApi` 表及各 `Zq*S` 结构体的内存布局和函数指针签名。
/// 用途：扩展在 `zq_module_register` 校验 `api.version == ZQ_ABI_VERSION`，
/// 不一致即拒绝加载，防止二进制不兼容的扩展跑在布局不符的 host 上。
/// 递增时机：仅当**破坏布局**的改动（增删字段、改函数签名）对外发布时；
/// 开发期保持恒定（host 与扩展同源编译，此校验只是兜底）。
pub const ZQ_ABI_VERSION: u32 = 1;

/// API（功能面）版本。
///
/// 含义：`ZqApi` 表提供的能力层级，不涉及二进制布局。
/// 用途：扩展在 `ZqModuleInfo.min_api_version` 声明所需的最低 API 层级，
/// host 据此判定"自身 API 版本 ≥ 扩展所需"后才加载。
/// 递增时机：仅当**新增** API 能力（向后兼容、追加函数）时；破坏性改动属
/// ABI 变更，归 `ZQ_ABI_VERSION` 管。
///
/// 当前两者都是 1：开发期，未对外发布。
pub const ZQ_API_VERSION: u32 = 1;

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

/// 一个 JS 值的类型（`typeof` 风格）。`#[repr(i32)]`，cbindgen 会翻译成 C enum。
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

/// 引擎值的 ABI 表示（GC 节点句柄）。**不是 newtype**，是裸 `u64` type alias。
pub type ZqRawValue = u64;

/// FFI 边界用的环境句柄（非回调线程路径）。
///
/// 裸指针别名：安全的 lifetime 视图见 `ZqCtx<'cx>`，扩展作者只碰 `ZqCtx`。
pub type ZqEnv = *mut ZqEnvS;

/// zapi 函数体 FFI 签名。
///
/// **返回值就是 JS 值**：成功时返回结果 handle；出错时返回 `value_exception`
/// 哨兵（quickjs 的 `JS_EXCEPTION` 同款约定）。需要抛出具体 JS 值时，先把它写进
/// 不透明唤醒句柄（host 端定义具体布局，扩展侧仅持指针）。
/// See `ZqApi::wake_handle_new` / `wake_handle_wake` / `wake_handle_free`.
#[repr(C)]
pub struct ZqWakeHandleS {
    _opaque: [*mut c_void; 0],
}

/// `info.thrown` 再返回 `value_exception`，host 据此抛对应值。
pub type ZapiBodyFn = unsafe extern "C" fn(env: ZqEnv, info: *mut ZqCallbackInfoS) -> ZqRawValue;

/// `ZqFieldDef::value_fn` 签名：用 env 构建一个 JS 值，把 raw handle 写进 `*out`。
/// 返回 0 成功、非 0 失败。仅用于 `ZQ_FIELD_VALUE`。
pub type ZqFieldValueFn = unsafe extern "C" fn(env: ZqEnv, out: *mut ZqRawValue) -> i32;

/// 全局（线程态）GC trace 回调：每次 GC 时被 host 调用，把扩展在 native 存储
/// （thread_local 等）里持有的 JS 值经 `ZqGcTraceCtx::mark` 告知 GC。
/// 与 class payload 的 `ZqClassDef.gc_trace` 同签名，通过 `set_gc_trace_func` 注册。
pub type ZqGcTraceFn = unsafe extern "C" fn(trace_ctx: *mut ZqGcTraceCtxS, opaque: *mut c_void);

// ── job queue (cross-thread async bridge) ──
/// Job run callback. Runs on the JS thread once per event-loop tick.
/// Returns: 0 = Done, 1 = Again (re-queue next tick), 2 = AgainNoHold
/// (re-queue without keeping the process alive), <0 = error (treated as Done).
pub type ZqJobRunFn = unsafe extern "C" fn(data: *mut c_void, env: ZqEnv) -> i32;
/// Optional GC trace for JS values held inside `data`.
pub type ZqJobTraceFn = unsafe extern "C" fn(data: *mut c_void, trace_ctx: *mut ZqGcTraceCtxS);

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

/// 回调信息，包含 target / this / args / 异常值 / magic。
///
/// 成功值**不**走这个结构——`body` 直接把结果作为返回值返回。只有"要抛的具体
/// JS 值"才经 `thrown` 传出（返回 `value_exception` 时 host 读取它）。
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
    /// 要抛出的 JS 值（0 = 无）；返回 `value_exception` 前由扩展写入。
    pub thrown: ZqRawValue,
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
    pub value_fn: Option<ZqFieldValueFn>,

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
    /// Host 预填的本 dylib 规范路径（NUL 结尾 UTF-8）。在
    /// `zq_module_register` 调用前写入，扩展可在注册期或后续通过
    /// `zuqe_zapi::module_path()` 读取（Node dlopen-filename 模式）。
    /// 旧 host 为 null。追加在末尾以保持旧扩展的 `#[repr(C)]` 布局稳定。
    pub path: *const c_char,
}

impl ZqModuleInfo {
    pub fn exports(&self) -> &[ZqFieldDef] {
        if self.exports.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.exports, self.exports_len) }
        }
    }

    /// Host 提供的 dylib 路径作为 `&str`（null / 非 UTF-8 → `None`）。
    pub fn path_str(&self) -> Option<&str> {
        if self.path.is_null() {
            None
        } else {
            unsafe { std::ffi::CStr::from_ptr(self.path) }.to_str().ok()
        }
    }
}

// ──────────────────────────────────────
// ZqApi — C ABI 函数指针表
// ──────────────────────────────────────

/// 稳定的 C ABI 函数指针表。
///
/// **字段顺序即 ABI 契约**：新增字段只能追加到末尾。开发期接口改动**不逐次
/// bump `ZQ_ABI_VERSION`**（host 与扩展总是同源编译）；仅在对外的 ABI 冻结 /
/// 发布时递增一次。
///
/// host 端构造该表并填充全部函数指针；扩展侧通过 `*mut ZqApi` 引用它。
#[repr(C)]
pub struct ZqApi {
    // ── 版本校验 ──
    pub version: u32,

    pub new_number: unsafe extern "C" fn(env: ZqEnv, v: f64) -> ZqRawValue,
    /// 从 **UTF-8 编码的字节** 构造 JS 字符串（`ptr`/`len` 指向的字节必须是合法
    /// UTF-8；host 不校验，非法序列按引擎策略处理）。与 `get_string_utf8`
    /// 配对：读出是 UTF-8，写入也是 UTF-8。失败返回 `value_exception`。
    pub new_string_from_utf8:
        unsafe extern "C" fn(env: ZqEnv, ptr: *const u8, len: usize) -> ZqRawValue,
    pub new_symbol: unsafe extern "C" fn(env: ZqEnv, desc: ZqRawValue) -> ZqRawValue,
    pub new_object: unsafe extern "C" fn(env: ZqEnv) -> ZqRawValue,
    pub new_array: unsafe extern "C" fn(env: ZqEnv, len: usize) -> ZqRawValue,

    /// 拷贝字符串 `s` 的 UTF-8 字节到 `buf`，从 `offset` 字节起。
    ///
    /// * `buf == null`（测量模式）：不写任何字节，返回从 `offset` 起到结尾所需
    ///   的字节数，调用方据此分配缓冲（quickjs `JS_GetStringUtf8` 习惯）；
    /// * `buf != null`：拷贝 `min(buf_len, 剩余字节数)` 字节，返回实际拷贝数；
    /// * 返回 `-1` 表示 `s` 不是字符串。
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
    /// calling back with `call_function`.
    pub queue_job: unsafe extern "C" fn(
        env: ZqEnv,
        run: ZqJobRunFn,
        trace: Option<ZqJobTraceFn>,
        data: *mut c_void,
    ) -> i32,

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
    /// 唤醒句柄：扩展的 worker 线程在把结果推入共享状态后调用
    /// `wake_handle_wake`，让 park 中的 JS 事件循环立即醒来处理。
    /// 若不调用，纯事件驱动模式下事件循环会一直挂起（无兜底轮询时）。
    pub wake_handle_new: unsafe extern "C" fn(env: ZqEnv) -> *mut ZqWakeHandleS,
    pub wake_handle_wake: unsafe extern "C" fn(h: *mut ZqWakeHandleS),
    pub wake_handle_free: unsafe extern "C" fn(h: *mut ZqWakeHandleS),

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
}

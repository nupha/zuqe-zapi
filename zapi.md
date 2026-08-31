# zapi — zuqe 原生扩展安全封装

zapi 让第三方以 **cdylib** 形式编写原生扩展，无需接触任何内部头文件或
`pub(crate)` 类型。它把引擎内部 API（到处是裸指针、GC 句柄、生命周期约束）
收进一个 **稳定的、C 兼容的 ABI 表**，再在之上一层提供 Rust 安全封装。

设计分三层：

- **Layer 1 — ABI 表 `ZqApi`**：纯 C 签名（`extern "C"`、裸指针、`u64` 值）。
  这是稳定契约，跨版本二进制兼容靠版本号 + 表大小双重校验。
- **Layer 2 — `zapi` 安全封装**：`ZqCtx` / `ZqValue` / `ZqRef` / `ZqEnv` /
  `ZqGcTraceCtx` 等带生命周期/新类型的 Rust 封装，把 FFI 危险藏在内部。
- **Layer 3 — 过程宏 `#[zq_function]` / `#[zq_module]`**：把普通 Rust 函数
  转成 ABI 入口，自动完成参数解包、错误传播、panic 捕获。

---

## 1. 类型名对照（当前代码）

| 名字            | 形态                                                         | 说明                                                                       |
| --------------- | ------------------------------------------------------------ | -------------------------------------------------------------------------- |
| `ZqRawValue`    | `pub type ZqRawValue = u64;`                                 | 引擎值的 ABI 表示（GC 节点句柄）。**不是 newtype**，是裸 `u64`。           |
| `ZqValue<'cx>`  | `repr(transparent)` struct over `ZqRawValue` + `PhantomData` | 安全句柄，带 `'cx` 生命周期，禁止逃逸出调用。                              |
| `ZqCtx`         | 安全封装（见 `ZqCtx::from_raw`）                             | 回调上下文；`aa.rt()` 取运行时，`aa.scope_arena()` 取 GC 作用域栈。        |
| `ZqRef<'cx, T>` | newtype over `ZqRawValue`                                    | 对 native-object payload 的可变借用（`T: GcTrace`）。                      |
| `ZqEnv`         | newtype over `*mut ZqEnvS`                                   | FFI 边界用的环境句柄（非回调线程路径）。                                   |
| `ZqGcTraceCtx`  | newtype over `*mut ZqGcTraceCtxS`                            | GC 追踪上下文（class payload 的 `trace`）。                                |
| `ZqError`       | 封装 `Exception` + `Option<String>`                          | 错误；`throw()` 抛到 JS、`.status()` 取 `ZqStatus`。                       |
| `ZqStatus`      | `repr(i32)` 枚举                                             | `Ok` / `TypeError` / `RangeError` / `Exception` / `Error` / `Pending` 等。 |
| `Exports`       | 封装 `*mut ZqEnvS`                                           | `zq_module` 的 init 用，往模块导出表塞函数/对象。                          |
| `Class<'cx, T>` | 封装 `*const ZqClassDef`                                     | native-class 注册。                                                        |

> 注意：`ZqRawValue` 是 **type alias 而非 newtype**。因此没有 `ZqRawValue(0)` /
> `ZqRawValue::ZERO` 这种构造，统一用 `0u64` / `0`。它实现 `From<ZqRawValue> for u64`
> 已被移除（orphan rule）。

---

## 2. Layer 1 — C ABI 表 `ZqApi`

`ZqApi` 是一张固定布局的 `repr(C)` 函数指针表。扩展在注册时拿到 `*mut ZqApi`，
全程只通过它调用引擎。结构体字段顺序即 ABI 契约，**新增字段只能追加到末尾**，
且必须 bump `ZQ_ABI_VERSION`。

### 2.1 版本与校验

```rust
pub const ZQ_ABI_VERSION: u32 = 1;

// 扩展侧（zapi-macros 生成）在 zq_module_register 入口处：
assert!(api.version == ZQ_ABI_VERSION, "ABI version mismatch");
```

loader 端（host）构造 `ZqApi { version: ZQ_ABI_VERSION, .. }` 并填充全部函数指针。

### 2.2 值构造（primitives）

| 字段                     | 签名                                              | 语义                           |
| ------------------------ | ------------------------------------------------- | ------------------------------ |
| `undefined_raw`          | `unsafe fn(*mut ZqEnvS) -> u64`                   | 返回 `undefined` 的 raw handle |
| `null_raw`               | `unsafe fn(*mut ZqEnvS) -> u64`                   | `null`                         |
| `bool_raw`               | `unsafe fn(*mut ZqEnvS, bool) -> u64`             | 布尔                           |
| `number_raw`             | `unsafe fn(*mut ZqEnvS, f64) -> u64`              | 数字                           |
| `string_raw`             | `unsafe fn(*mut ZqEnvS, *const u8, usize) -> u64` | 从 UTF-8 构造字符串            |
| `symbol_raw`             | `unsafe fn(*mut ZqEnvS, u64) -> u64`              | 从描述字符串造 symbol          |
| `new_symbol`             | `unsafe fn(*mut ZqEnvS, u64) -> u64`              | 同上（扩展入口）               |
| `symbol_to_string`       | `unsafe fn(*mut ZqEnvS, u64) -> u64`              | symbol → 描述字符串            |
| `get_symbol_description` | `unsafe fn(*mut ZqEnvS, u64) -> u64`              | 取 symbol 描述                 |

### 2.3 字符串读写

| 字段                    | 签名                                                        | 语义                                         |
| ----------------------- | ----------------------------------------------------------- | -------------------------------------------- |
| `string_len`            | `unsafe fn(*mut ZqEnvS, u64) -> usize`                      | 字节长度                                     |
| `get_string_utf8`       | `unsafe fn(*mut ZqEnvS, u64, *mut u8, usize, usize) -> i32` | 拷贝 UTF-8 到缓冲区，返回写入字节数（或 -1） |
| `new_string_builder`    | `unsafe fn(*mut ZqEnvS, usize) -> u64`                      | 预分配 builder                               |
| `string_builder_append` | `unsafe fn(*mut ZqEnvS, u64, *const u8, usize) -> i32`      | 追加片段                                     |
| `string_builder_finish` | `unsafe fn(*mut ZqEnvS, u64) -> u64`                        | 完成 → 字符串值                              |
| `free_string`           | `unsafe fn(*mut ZqEnvS, u64)`                               | 释放 builder 中间态                          |

### 2.4 对象 / 数组 / 属性

| 字段              | 签名                                                                             | 语义                          |
| ----------------- | -------------------------------------------------------------------------------- | ----------------------------- |
| `new_object`      | `unsafe fn(*mut ZqEnvS) -> u64`                                                  | 空对象                        |
| `new_array`       | `unsafe fn(*mut ZqEnvS, usize) -> u64`                                           | 预分配数组                    |
| `set_property`    | `unsafe fn(*mut ZqEnvS, u64 obj, u64 key, u64 val, i32 throw, u32 flags) -> u64` | 设属性，返回 `val` 或异常哨兵 |
| `get_property`    | `unsafe fn(*mut ZqEnvS, u64 obj, u64 key) -> u64`                                | 取属性                        |
| `has_property`    | `unsafe fn(*mut ZqEnvS, u64 obj, u64 key) -> i32`                                | 是否存在                      |
| `delete_property` | `unsafe fn(*mut ZqEnvS, u64 obj, u64 key, i32 throw) -> i32`                     | 删除                          |
| `array_push`      | `unsafe fn(*mut ZqEnvS, u64 arr, u64 val) -> i32`                                | 推入                          |
| `array_get`       | `unsafe fn(*mut ZqEnvS, u64 arr, u64 idx) -> u64`                                | 索引取                        |
| `array_set`       | `unsafe fn(*mut ZqEnvS, u64 arr, u64 idx, u64 val) -> i32`                       | 索引设                        |
| `array_len`       | `unsafe fn(*mut ZqEnvS, u64 arr) -> u64`                                         | 长度                          |
| `set_global`      | `unsafe fn(*mut ZqEnvS, u64 name, u64 val) -> i32`                               | 设全局                        |
| `get_global`      | `unsafe fn(*mut ZqEnvS, u64 name) -> u64`                                        | 取全局                        |
| `has_global`      | `unsafe fn(*mut ZqEnvS, u64 name) -> i32`                                        | 全局存在？                    |

### 2.5 函数注册与调用

| 字段                | 签名                                                                                                               | 语义                                                       |
| ------------------- | ------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------- |
| `new_function`      | `unsafe fn(*mut ZqEnvS, u64 name, ZqRawValue, unsafe fn(*mut ZqEnvS, *mut ZqCallbackInfoS) -> u64, u8 len) -> u64` | 造 JS 函数对象                                             |
| `register_function` | `unsafe fn(*mut ZqEnvS, u64 obj, u64 name, unsafe fn(..) -> u64, u8 len) -> i32`                                   | 挂为对象方法                                               |
| `remove_wrap`       | `unsafe fn(*mut ZqEnvS, u64 obj, *const c_char) -> i32`                                                            | 移除 wrap                                                  |
| `call_function`     | `unsafe fn(*mut ZqEnvS, *mut ZqCallbackInfoS) -> u64`                                                              | 调用 JS 函数（callback info 携带 target/this/args/return） |

### 2.6 异常

| 字段                           | 签名                                              | 语义                                                |
| ------------------------------ | ------------------------------------------------- | --------------------------------------------------- |
| `exception_pending`            | `unsafe fn(*mut ZqEnvS) -> i32`                   | 是否有未决异常                                      |
| `throw`                        | `unsafe fn(*mut ZqEnvS, u64 val) -> i32`          | 抛任意值                                            |
| `throw_type_error`             | `unsafe fn(*mut ZqEnvS, *const u8, usize) -> i32` | 抛 TypeError                                        |
| `throw_range_error`            | `unsafe fn(*mut ZqEnvS, *const u8, usize) -> i32` | 抛 RangeError                                       |
| `set_exception`                | `unsafe fn(*mut ZqEnvS, u64 val)`                 | 在 callback 中设异常（配合 `set_return_exception`） |
| `get_last_error_message`       | `unsafe fn(*mut ZqEnvS) -> u64`                   | 取最后错误消息字符串                                |
| `get_and_clear_last_exception` | `unsafe fn(*mut ZqEnvS) -> u64`                   | 取并清除最后异常值                                  |

### 2.7 GC 安全

| 字段               | 签名                                                     | 语义                                                      |
| ------------------ | -------------------------------------------------------- | --------------------------------------------------------- |
| `gc_add_ref`       | `unsafe fn(*mut ZqEnvS, u64)`                            | 跨回调保留值（root）                                      |
| `gc_release`       | `unsafe fn(*mut ZqEnvS, u64)`                            | 释放保留                                                  |
| `gc_mark`          | `unsafe fn(*mut ZqGcTraceCtxS, u64) -> i32`              | Mark value as reachable in trace ctx                     |
| `gc_release_trace` | `unsafe fn(*mut ZqEnvS, *mut ZqGcTraceCtxS)`             | 结束追踪                                                  |
| `set_global_trace` | `unsafe fn(*mut ZqEnvS, usize)`                          | 设置全局 trace（usize 为 `Option<usize>` 编码，0 = None） |

### 2.8 native object / class

| 字段             | 签名                                                            | 语义                  |
| ---------------- | --------------------------------------------------------------- | --------------------- |
| `register_class` | `unsafe fn(*mut ZqEnvS, *const ZqClassDef) -> i32`              | 注册类                |
| `new_instance`   | `unsafe fn(*mut ZqEnvS, *const ZqClassDef, *mut c_void) -> u64` | 用 payload 指针造实例 |
| `get_class_def`  | `unsafe fn(*mut ZqEnvS, u64) -> *const ZqClassDef`              | 取实例的类定义        |
| `get_native_ptr` | `unsafe fn(*mut ZqEnvS, u64) -> *mut c_void`                    | 取 payload 指针       |
| `set_native_ptr` | `unsafe fn(*mut ZqEnvS, u64, *mut c_void) -> i32`               | 设 payload 指针       |
| `get_raw`        | `unsafe fn(*mut ZqEnvS, u64) -> u64`                            | 取底层 raw（透传）    |

### 2.9 注册与模块

| 字段              | 签名                                       | 语义                                             |
| ----------------- | ------------------------------------------ | ------------------------------------------------ |
| `module_register` | `unsafe fn(*mut ZqApi, *mut ZqModuleInfo)` | 扩展在 `zq_module_register` 内回调，吐出模块信息 |

`ZqCallbackInfoS` 与 `ZqEnvS` 在 zapi 侧为 **不透明布局**（`#[repr(C)]` 但字段不对外
导出），仅作为 `*mut` 传递。host 端按自身内部布局定义同尺寸结构体即可——ABI 只关心
指针宽度与调用约定，不关心内部字段名。

---

## 3. Layer 2 — `zapi` 安全封装（关键约定）

### 3.1 `ZqCtx` 与安全边界

```rust
pub struct ZqCtx<'cx> {
    env: *mut ZqEnvS,
    _marker: PhantomData<&'cx ()>,
}

impl<'cx> ZqCtx<'cx> {
    /// # Safety
    /// `env` 必须是一个活的回调环境指针（来自 ZqApi 调用）。
    pub unsafe fn from_raw(env: *mut ZqEnvS) -> Self { ... }
    pub fn rt(&self) -> &RuntimeImpl { ... }          // 经 user_data 解引用
    pub fn scope_arena(&self) -> &GcScopeStack { ... } // GC 作用域栈
    pub fn string(&self, s: &str) -> ZqValue<'cx> { ... }
    pub fn number(&self, n: f64) -> ZqValue<'cx> { ... }
    pub fn array(&self, v: &[ZqValue<'cx>]) -> ZqValue<'cx> { ... }
    pub fn object(&self) -> ZqValue<'cx> { ... }
    pub fn func(&self, name: &str, f: unsafe fn(*mut ZqEnvS, *mut ZqCallbackInfoS) -> u64, len: u8) -> ZqValue<'cx> { ... }
    pub fn undefined(&self) -> ZqValue<'cx> { ... }
}
```

**生命周期即安全契约**：`ZqValue<'cx>` 不能存储到比 `'cx` 更长的地方。任何想把值
跨回调保存的需求，都必须走 `gc_add_ref`（见 3.4）。

### 3.2 `ZqValue<'cx>`

`repr(transparent)` over `ZqRawValue` + `PhantomData`。所有到 ABI 的转化走
`raw()` / `from_raw`（或直接 `ZqRawValue`）。实现 `FromZqValue`（解包 JS 参数）
与 `IntoZqValue`（转换返回/参数）trait，覆盖 `i32`/`i64`/`f64`/`bool`/`String`/
`Option<T>`/`ZqValue`/`ZqRef` 等。

`FromZqValue` 失败时调用 `throw_type_error` 并 `return Err`——由 trampoline 在
边界清理。

### 3.3 `ZqRef<'cx, T>`

```rust
pub struct ZqRef<'cx, T: GcTrace> { raw: ZqRawValue, _marker: PhantomData<&'cx T> }
impl<'cx, T: GcTrace> ZqRef<'cx, T> {
    pub fn payload(&self) -> &T { ... }        // 解 native ptr
    pub fn payload_mut(&mut self) -> &mut T { ... }
}
```

用于 native-object 方法访问自身 payload。

### 3.4 GC 安全三件套

- **回调内**：用 `ZqValue<'cx>`（栈生命周期管理），无需手动 root。
- **跨回调保存**：`gc_add_ref(value)` 把值钉为 GC root，`gc_release` 释放。
- **class payload `trace`**: call `gc_mark(ctx, value)` in the `GcTrace::trace`
  implementation to inform the GC about held JS values; call `gc_release_trace`
  when done.

### 3.5 错误

```rust
pub enum ZqStatus { Ok, TypeError, RangeError, Exception, Error, Pending, ... }
pub struct ZqError { exc: Exception, msg: Option<String> }
impl ZqError {
    pub fn throw(&self, cx: &ZqCtx) { ... }  // 抛到当前 JS 上下文
    pub fn status(&self) -> ZqStatus { ... }
}
```

`Result<T>` 在 `#[zq_function]` 边界由 trampoline 捕获：错误 → `throw_type_error` /
`set_exception` 回到 JS。

---

## 4. Layer 3 — 过程宏

### 4.1 `#[zq_function]`

```rust
#[zq_function]
fn add(cx: &ZqCtx, a: i32, b: i32) -> Result<i32> { Ok(a + b) }
```

宏要求**第一个参数必须是 `&ZqCtx`**（错则编译期报错）。其余参数经 `FromZqValue`
从 `CallbackInfo.args` 解包（不匹配 → JS TypeError）。返回 `Result<T>`（`T: IntoZqValue`）。

宏生成：

1. 一个底层 FFI 入口 `unsafe fn(cx_raw: *mut ZqEnvS, info: *mut ZqCallbackInfoS) -> u64`；
2. 一个 `Marker` 结构体，含 `call(cx, a, b) -> Result<T>` 供 Rust 侧直接调用，以及
   供 `module_register` 使用的 `info()`（名字、body 指针、arity）。

### 4.2 `#[zq_module]`

```rust
#[zq_module(name = "demo")]
fn init(cx: &ZqCtx, exports: &Exports) -> Result<()> {
    exports.export("add", add::info())?;
    Ok(())
}
```

要求签名 `fn(&ZqCtx, &Exports) -> Result<()>`。生成 `unsafe extern "C" zq_module_register(api: *mut ZqApi, info: *mut ZqModuleInfo)`，
内部：校验 `api.version`、填 `info.name`、把 `init` 适配成 `extern "C" fn(*mut ZqEnvS, *const ZqExportsS) -> i32`，
并把所有 `export` 收集进 `info.funcs`。

> 注：`ZqModuleInfo.funcs` 的元素类型 `ZqFuncInfo` 是 `repr(C)`，含
> `name: *const c_char`、`body: unsafe fn(*mut ZqEnvS, *mut ZqCallbackInfoS) -> u64`、
> `arity: u8`。

---

## 5. 构建

扩展 crate：

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
zapi = { path = "../zapi" }
```

宿主（引擎）侧需把 `ZqApi` 实现出来（见第 6 节）。`tests/mock_host.rs` 是一个
**完整可编译的 host 参考实现**，演示了 `ZqApi` 全部 50+ 函数如何落到具体引擎 API、
poller 线程如何走 raw 路径、以及 `call_function_impl` 的回调翻译。

---

## 6. zhuque 引擎 host 端集成规划

> **当前状态**：zhuque 引擎（`/Users/johnray/devel/zhuque`）已实现 zapi host stub
> （`src/zapi/host.rs`，feature `zapi`），包含完整的 `ZqApi` 表（4 个 primitive
> 函数已实现，其余为 `todo!()`）、运行时跳板 `register_zapi_func`、class 注册/实例化/
> GC trace 桥接。`examples/zjs/main.rs` 已集成 zapi 协议的动态模块加载。
> 资源生命周期管理（Library 句柄 / API 表）当前为泄漏实现，详见 §6.8。

### 6.1 核心矛盾：两套回调 ABI

zhuque 内部 native function 的回调签名是：

```rust
pub type CallbackFn = fn(ctx: &mut ScopedContext, cc: CallContext) -> JSResult;
```

而 zapi 扩展导出的 `body` 是：

```rust
unsafe fn(*mut ZqEnvS, *mut ZqCallbackInfoS) -> u64
```

两者**不能直接互调**。host 端必须提供一层 **thunk**：当 JS 调用该扩展函数时，
zhuque 用 `CallbackFn` 进入，thunk 把 `(ScopedContext, CallContext)` 翻译成
`(*mut ZqEnvS, *mut ZqCallbackInfoS)`，再调扩展 `body`，最后把 `u64` 结果 / 异常
哨兵翻译回 `JSResult`。

### 6.2 `ZqEnvS` / `ZqCallbackInfoS` 在 host 端的定义

zapi 侧这两个类型是**不透明**的——host 端自行定义与其 ABI 兼容的布局（字段名随意，
只要大小/对齐匹配且语义正确）：

```rust
#[repr(C)]
pub struct ZqEnvS {
    ctx: *mut ContextImpl,          // 当前 ScopedContext 对应的上下文
    trace: *mut GcTraceCtx,         // 当前 GC trace 上下文（class payload 用）
    rt: RuntimePtr,                 // 运行时，供 raw 路径（poller 线程）开 scope
}

#[repr(C)]
pub struct ZqCallbackInfoS {
    target: u64,                    // 被调用函数对象
    this: u64,                      // this 值
    args: *const u64,               // 参数数组（raw handles）
    argc: u32,
    ret: u64,                       // 返回值（set_return 写入）
    threw: u64,                     // 异常哨兵（0 = 无）
}
```

`ZqCtx::from_raw(env)` 在 zapi 内部经 `env.user_data` 解引用——host 端需保证
`user_data` 字段里存的是能重建 `ScopedContext` / `ContextImpl` 的指针（对照
`mock_host` 的 `with_env`：把 `&mut ScopedContext` 暂存进 `env.user_data`）。

### 6.3 函数注册流程（loader）

```
1. dlopen(extension.so)
2. 取符号 zq_module_register : unsafe extern "C" fn(*mut ZqApi, *mut ZqModuleInfo)
3. 构造 host 侧 ZqApi 表（填充全部 50+ 函数 → 见 6.4）
4. 分配 ZqModuleInfo，调用 zq_module_register(&mut api, &mut info)
5. 对每个 info.funcs[i]：
     - 生成 thunk(ScopedContext, CallContext) -> JSResult
     - 用 zhuque 的 NativeFunction::new / register_function 注册为
       globalThis[info.name][func.name] 或模块导出
6. 调 info.init(env, exports) 填充模块导出表
```

### 6.4 `ZqApi` 实现要点（host 侧）

每个 `ZqApi` 字段都是一次 **zhuque 内部 API 调用**：

- **值构造**：`string_raw` → `ctx.string(...)`；`number_raw` → `ctx.number(...)`；
  `new_object` / `new_array` → `Object` 构造。
- **属性**：`set_property` / `get_property` → `Object::set_property_value` /
  `get_property` 系列（对照 `zhuque/src/object/property/`）。
- **调用 JS**：`call_function` 在 thunk 内由 zhuque `NativeFunction::class_call` 完成，
  无需单独实现——host 只需把 `body` 包成 `CallbackFn`。
- **异常**：`throw_type_error` → `ctx.throw_type_error(...)`；`exception_pending` →
  查 `ContextImpl` 的 pending exception；`get_and_clear_last_exception` →
  `ctx.take_exception()`。
- **GC**: `gc_add_ref` / `gc_release` → add/remove value from context's root
  set; `gc_mark` → `GcTraceCtx`'s trace (see `zhuque/src/gc/` and
  `class/native.rs`).
- **class**：`register_class` / `new_instance` / `get_native_ptr` → 复用 zhuque 的
  `NativeObject` 机制（`class/using.rs` 的 `use_classes`）。

### 6.5 非回调线程（poller / 后台任务）

`mock_host` 已经证明：扩展的 `init` 可能 spawn 线程，线程里直接调 `ZqApi` 函数
（无 `ScopedContext`）。此时 `env` 不能依赖临时 scope，必须：

1. `env.rt` 持有 `RuntimePtr`，线程函数内 `rt.with_new_scope(|scoped| { ... })` 临时开作用域；
2. 所有跨线程保留的 JS 值必须 `gc_add_ref`（脱离任何回调 scope）；
3. 线程结束前 `gc_release` 全部保留值，或移交回主线程 scope。

### 6.6 模块语义选择

zapi 当前 `Exports` 是**全局对象导出**（底层 `set_global`）。host 端有两种落地：

- **A. 全局命名空间**：`globalThis[module_name]` 是个对象，所有导出挂其下。
  简单、符合现状。
- **B. ES module**：把 `info.funcs` + `init` 导出包成 zhuque 的 `ModuleDef`
  （对照 `zhuque/src/module.rs`），支持 `import` 语法。需 zhuque 增加
  "动态注册 module" 的能力（目前 `module` 主要由 `path_to_url` 加载源码）。

建议先落地 **A**（与 `Exports`/`set_global` 一致），B 作为后续增强。

### 6.7 实施步骤建议

1. 在 zhuque 新建 `src/zapi/zapi_host.rs`，定义 `ZqEnvS` / `ZqCallbackInfoS` /
   `ZqApi` host 实现（约 50 个转发函数 + thunk 生成器）。
2. 新建 `src/zapi/loader.rs`：`load_extension(path) -> Result<ModuleHandle>`，
   封装 dlopen + `zq_module_register` + 注册流程（6.3）。
3. 在 `RuntimeImpl` / `ContextImpl` 暴露 `register_extension(path)` 给宿主程序。
4. 参考 `zapi/tests/mock_host.rs` 逐函数对齐行为（尤其 GC root 与异常清理），
   让 `cargo test -p zapi` 的 mock_host 用例可作为 host 端实现的对拍基准。
5. 先实现类型 A 模块语义；类型 B（ES module）后续按需。

### 6.8 资源生命周期管理

> **状态：当前为泄漏实现，待后续接入 Runtime 生命周期。**

当前 `examples/zjs/main.rs` 的 `load_zapi_native_module` 中有两处故意泄漏：

```rust
std::mem::forget(lib);  // Library 句柄（dlopen 返回）
Box::leak(api);         // ZqApi 函数指针表
```

**泄漏原因**：zuqe 引擎尚未提供扩展卸载（unload）的生命周期钩子。在 Runtime 销毁时
没有机制去遍历已加载的扩展并执行清理。

**正确的生命周期管理应为**：

```
Runtime::drop()
  ├─ 遍历已加载扩展列表
  │   ├─ class on_unregister() 回调（扩展清理 class 级资源）
  │   ├─ Box::from_raw(api)    释放 ZqApi 表
  │   └─ drop(lib)             dlclose .so/.dylib
  └─ RuntimeImpl 其余资源析构
```

**需要新增的机制**：

1. 在 `RuntimeImpl` 或 `ContextImpl` 中新增扩展注册表，记录每个已加载扩展的：
   - `Library` 句柄（用于 dlclose）
   - `Box<ZqApi>` 所有权（用于释放 API 表）
   - class ID 列表（用于在 unload 时调用 `on_unregister`）

2. Host 端 `load_extension` 加载成功后，将上述三项注入注册表。

3. `Runtime` 析构或显式 `unload_extension` 时，按注册表反序清理：
   - 调用每个 class 的 `on_unregister` 回调
   - 释放 API 表
   - `dlclose` 动态库

4. GC 安全检查：unload 前必须确保该扩展创建的所有对象均已被 GC 回收；
   否则 `on_dispose`（`ZqClassDef.finalize`）会调用已卸载的代码。实现方式为
   在 unload 前触发一次完整 GC，且 class 注册表在 GC 完成后才清空。

---

## 7. FAQ（已据当前代码修正）

- **`ZqValue` 和 `ZqRawValue` 的区别？** `ZqRawValue` 是裸 `u64`（ABI 层值句柄，
  type alias）；`ZqValue<'cx>` 是带生命周期的安全封装（封装层）。扩展代码里几乎
  只用 `ZqValue<'cx>` 和 `&ZqCtx`。
- **第一个参数为什么是 `&ZqCtx` 而不是 `&mut`？** `ZqCtx` 内部经 `rt()` 拿到
  `&mut` 访问能力，但对外暴露 `&` 以避免别名问题；需要改运行时状态时经
  `aa.rt()` 取可变引用。
- **panic 会怎样？** 每个 `#[zq_function]` 边界由 `catch_unwind` + `AssertUnwindSafe`
  包裹；panic 转为 `Error` 异常抛回 JS，不会 abort 宿主。
- **`ZqEnv` 和 `ZqCtx` 何时用哪个？** `ZqCtx` 用于回调内（有 scope）；`ZqEnv`
  （`*mut ZqEnvS` newtype）用于 FFI 边界 / 非回调线程路径（如 poller、class payload）。
- **ABI 能否加字段？** 只能**追加到 `ZqApi` 末尾**并 bump `ZQ_ABI_VERSION`；
  中间插字段会破坏二进制布局兼容。

---

## 8. ABI 类型归属（已定论：留在 `zapi`，不下沉 `zuqe`）

> **结论先行**：`ZqApi` / `ZqEnvS` / `ZqCallbackInfoS` / `ZqClassDef` / `ZqModuleInfo` /
> `ZQ_ABI_VERSION` 等 ABI 契约类型**必须留在 `zapi` crate（的 `ffi` 模块）**，
> **绝不能搬进 `zuqe`**。host 端（`zuqe` 引擎）实现 `ZqApi` 时，应**依赖 `zapi` 的 `ffi`
> 模块**（`use zapi::ffi::*`），而不是反过来让 `zapi` 依赖 `zuqe`。

### 为什么不能下沉到 `zuqe`（会直接打穿设计前提）

本文件开头六原则 ⑤ 写明：**核心就是一份 `extern "C"` 函数表，C 直接 `zq_*` 调用、
Rust 既可直接调 `raw` 也可走糖层——二者同一份契约；扩展编译成独立 cdylib，不依赖
`zuqe` crate、不与宿主同批编译**。

若把 `raw` 搬进 `zuqe`：

1. `zapi` 必须 `use zuqe::zapi_abi` → **`zapi` 依赖 `zuqe`**；
2. 第三方扩展 `use zapi::prelude::*` → 传递依赖 `zuqe` → 写扩展也要有 `zuqe`
   crate，破坏"跨版本二进制兼容 / 不与宿主同批编译"；
3. **C 扩展直接写不了**：C 侧靠 `cbindgen` 从 `zapi/src/ffi.rs` 生成 `.h`，头文件
   一旦进了 `zuqe`，C 开发者要拿 `zuqe` 才能拿头，且 C 根本不能 `use zuqe` 的 Rust
   模块——"扩展依赖引擎内部类型"回到原点，原则 ⑤ 破产。

### 正确的依赖方向

`zapi` 不是"扩展封装库"，它是**host 与扩展共用的 ABI 契约 crate**，零引擎内部依赖。
引擎依赖它来拿到类型定义并实现 `ZqApi`，是完全合理的——所谓"引擎反向依赖扩展
封装库"是伪问题（`raw` 不属于任何一方的内部实现）：

```
zuqe (host 实现 ZqApi)  ──use──>  zapi (raw 契约 + prelude 糖层)   ← 引擎依赖 SDK
扩展 (cdylib)           ──use──>  zapi                            ← 扩展依赖 SDK
C 扩展                   ──cbindgen──>  zapi/src/ffi.rs 生成的 .h  ← 不依赖任何 Rust crate
```

### 可选的进一步拆分（不是必须）

若希望 host 端只拉 `raw` 类型、不连带 `prelude` 的 std/线程局部状态，可把 `raw`
拆成独立 crate（如 `zapi-abi`），让 `zapi`（糖层）与 `zuqe`（host）都只依赖
`zapi-abi`。方向仍是 **`zuqe`/`zapi` → `zapi-abi`**，与"下沉到 `zuqe`"相反。
是否拆取决于 `prelude` 是否确实给 host 带来不该有的负担（当前 host 实现 `ZqApi`
只用到 `raw` 的类型与常量，`prelude` 不参与），非紧迫项。

### 与第 6 节的关系

第 6 节的 host 端实现（`src/zapi/zapi_host.rs` 等）直接 `use zapi::ffi::*`，按现状
即可落地，**无需任何类型搬迁**。本节的定论只用于否决"下沉到 `zuqe`"那条路，避免
后续误改。

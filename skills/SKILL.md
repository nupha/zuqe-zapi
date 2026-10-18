---
name: write-zapi-extension-library
description: How to write a zapi native extension library - build setup, the proc macros, code patterns for every kind of function, argument validation, errors, threading, and GC tracing. Read before creating or modifying a zapi extension.
---

# Writing a zapi extension

A zapi extension is a `cdylib` the JS runtime loads and calls through a
generated C ABI. You write ordinary Rust plus a few proc macros; they
generate the glue, the export list, and the module entry point. You never write
FFI code.

This document is about writing extensions. It explains the framework only as far
as you need it to make correct choices.

## 1. Setup

### Cargo.toml

```toml
[package]
name = "aike_myext"
version = "0.1.0"
edition = "2024"

[lib]
name = "myext"                 # -> myext.dll / libmyext.so / libmyext.dylib
crate-type = ["cdylib"]        # required

[dependencies]
zuqe_zapi = "0.4.0"            # must match the host's version
zuqe_zapi_macros = "0.4.0"
```

### Layout

```text
myext/
├── Cargo.toml
├── src/
│   ├── lib.rs            # the #[zq_module] entry point + #[zq_function] exports
│   └── ... other files
├── types/
│   └── myext.d.ts        # the TS declaration; named after [lib] name
└── ... other files
```

### The type declaration

`types/myext.d.ts` is a required deliverable and the source of truth for the JS
API. Hand-write it next to the Rust code it describes, so the two version
together. Name it after `[lib] name`: `myext.d.ts`, not `libmyext.d.ts` and not
the crate name.

Declare the module under the name JS imports it by, then declare every export
inside:

```ts
declare module "myext" {
  /** Options bag. `?` means the caller may omit the key. */
  export interface OpenOptions {
    /** Model directory. Required - there is no default. */
    dir: string;
    /** Worker threads. Defaults to one less than the logical CPU count. */
    numThreads?: number;
    /** Backend to use. Defaults to "cpu". */
    provider?: "cpu" | "cuda";
  }

  /** One result row. */
  export interface Hit {
    score: number;
    text: string;
  }

  export class Recognizer {
    private constructor();
    /** Async, because loading a model is slow. */
    static open(options: OpenOptions): Promise<Recognizer>;
    /** Synchronous once open. Throws `TypeError` on a bad argument. */
    recognize(path: string): Hit[];
    /** Releases native resources. Safe to call twice. */
    close(): void;
  }

  /** The linked native library version. */
  export function version(): string;
}
```

What to keep straight:

- **A Promise in the signature means the Rust body returned one.** A method is
  neither "sync" nor "async" - it is whatever the body returns. `open` above wraps
  a `promise_*` call; `recognize` is a plain body.
- **`?` is not "has a default".** It means the caller may omit it. If your Rust
  code rejects `null`, do not write `string | null` here.
- **Mark a throwing constructor `private`.** When `constructor` only throws and
  instances come from a static factory, JS should not be able to call `new`.
- **A union of string literals** documents an enum argument; the Rust side must
  reject anything else with a `RangeError`.
- **Declare only what the module really exposes.** A member listed here that JS
  does not have is a lie nothing will catch.

When you add an export in Rust, add it here in the same change.

### Build and run

```sh
cargo build                                     # debug; never --release
cp target/debug/myext.dll tests/myext.aike   # the runtime loads .aike files
AIKE_LIB_PATH=$PWD/target/debug aike -m tests/t.mjs
```

- A bare `import` resolves to `<name>.aike` next to the importer, then
  `AIKE_LIB_PATH`. Copying into `tests/` is the normal dev loop.
- `-m` runs the script as ESM.
- **Debug only.** It is what turns a memory bug into an assertion instead of
  silent corruption.
- `cargo build` caches. After changing source from a script, `touch` the file and
  delete the artifact, or you will test a stale binary.
- `ReferenceError: module '...': zapi ABI/API version N mismatch` at import means
  your `zuqe-zapi` version does not match the host's.

## 2. The proc macros

Seven attribute macros and one derive, all from the `zuqe_zapi_macros` crate,
and each must be imported. The runtime types (`ZqCtx`, `ZqValue`, `ZqCallInfo`,
`ZqError`, ...) come from `zuqe_zapi`. So each source file that uses a macro
needs both:

```rust
use zuqe_zapi::*;
use zuqe_zapi_macros::{zq_class, zq_class_impl, zq_method, zq_static_method};
```

Import them per file rather than once in `lib.rs` - a new module that forgets its
own `use` line will not compile, and that is the intended failure. Import only
what that file uses.

One name to watch: `#[derive(ZqGcTrace)]` is exported by `zuqe_zapi_macros`,
while the `ZqGcTrace` _trait_ you implement comes from `zuqe_zapi`. They share a
name but live in different namespaces, so both can be in scope at once. Most
extensions hand-write `impl ZqGcTrace for ...` and never use the derive.

| Macro                                                   | Use for                               | Shape                                                                                                     |
| ------------------------------------------------------- | ------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| `#[zq_module(name, exports = [...], finalize = f)]`     | one per module                        | `fn module_init()` or `fn module_init(ctx) -> Result<(), ZqError>`                                        |
| `#[zq_function(name = "js", magic = N)]`                | a free function                       | `fn f<'a>(ctx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError>`                            |
| `#[zq_export(name = "js")]`                             | a value computed at module load       | `fn f<'a>(ctx: ZqCtx<'a>) -> Result<ZqValue<'a>, ZqError>`                                                |
| `#[zq_class(name = "Cls", payload = T, payload_no_gc)]` | a JS class                            | on `struct Cls;`                                                                                          |
| `#[zq_class_impl]`                                      | the class body                        | on the inherent `impl Cls`; holds `fn constructor`, `#[zq_method]`, `#[zq_static_method]`                 |
| `#[zq_method(name = "js", get, set, magic = N)]`        | an instance method (on the prototype) | inside `#[zq_class_impl]`: `fn f<'a>(ctx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError>` |
| `#[zq_static_method(name = "js", get, set, magic = N)]` | a static method (on the constructor)  | same shape as `#[zq_method]`                                                                              |
| `#[derive(ZqGcTrace)]`                                  | mark every field of a payload struct  | on `struct Payload` (chapter 8)                                                                           |

Rules:

- A `#[zq_class]` must be paired with a `#[zq_class_impl]` in the same module.
- Method names in Rust are normally `CamelCase` because they _are_ the JS names;
  override with `name = "js_name"`.
- The `#[zq_module]` function is always the init hook, whatever it is called.
- Anything not listed in `exports` does not exist in JS.
- `#[zq_module(finalize = ...)]` runs once per runtime at teardown and must not
  call any JS API.

Naming: the context parameter is `ctx`, the call info is `cc` (`_cc` when
unused). In a `trace` implementation the trace context is `gcx`.

### The smallest module

```rust
use zuqe_zapi::*;
use zuqe_zapi_macros::{zq_function, zq_module};

#[zq_function(name = "version")]
fn version<'a>(ctx: ZqCtx<'a>, _cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
    ZqValue::new_string(ctx, "1.0.0")
}

#[zq_module(name = "myext", exports = [version])]
fn module_init() {}
```

### Module setup goes in the init hook

Whatever the module must set up before any exported function runs, do in the
function you annotate with `#[zq_module]`. It runs once per runtime that imports
the module, with a real `ZqCtx`, which is why setup that depends on the runtime
belongs there rather than in a `static` initializer - and why anything a single
runtime needs must not be shared across runtimes (chapter 3).

It takes `ctx` only if it needs it; a module with nothing to set up uses an empty
`fn module_init() {}`.

Common cases:

- **An async executor**, if you use `promise_fut` - it has to be named before any
  call can use it (see 4.3).
- **An internal class** - one you do not list in `exports` but still need to
  instantiate: `MyInternal::register(ctx)?`.
- **Initial per-runtime state**, when a runtime should start with something set
  up (chapter 3).

### `magic`: one body, several JS names

`magic` stamps an `i32` on the call, read in the body as `cc.magic()`. Stack the
attribute and branch on it.

```rust
// tx.commit(), tx.rollback() and tx.abort() are the same code path.
#[zq_method(name = "commit", magic = 0)]
#[zq_method(name = "rollback", magic = 1)]
#[zq_method(name = "abort", magic = 1)]     // an alias, same body
fn commit_or_rollback<'a>(ctx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
    let kind = if cc.magic() == 0 { JobKind::Commit { .. } } else { JobKind::Rollback };
    // ...
}
```

The same works on a free function; `exports` still lists it once and every
stacked name is published:

```rust
#[zq_function(name = "version", magic = 0)]
#[zq_function(name = "versionAlias", magic = 1)]
fn version<'a>(ctx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
    match cc.magic() {
        0 => version_object(ctx),
        _ => ZqValue::new_string(ctx, "alias"),
    }
}
```

- Names must be unique within one stack; a duplicate is a compile error.
- `magic` defaults to `0`.
- Use it only when the operations are genuinely the same work.

`magic` doubles as an identifier. A native function cannot capture, so when each
registered callback needs its own state, store a small `i32` beside the callback
and recover it with `cc.magic()`; otherwise the function acts on whichever
instance registered last.

## 3. Per-runtime state

**A runtime is one JS engine instance.** One `cdylib` can be loaded into several
runtimes in the same process, and each load is independent. Your `static`s are
shared by all of them, so a plain `static` cannot hold per-runtime data - one
runtime would overwrite another's, and a callback one runtime registered could be
delivered to another.

`ZqRuntimeState<S>` holds **one `S` per runtime** and routes each call to the
right one. You never handle runtime identity yourself.

Use it when a JS value must outlive the call that delivered it and no object
owns it: a runtime-wide log handler, a listener set up before any object exists,
or any callback you keep so a later native event can reach it.

If a class payload can own the value, prefer the payload - it is traced for free
and dies with the object. If the value is only needed within one call, there is
nothing to store.

```rust
use std::collections::HashMap;
use zuqe_zapi::*;

#[derive(Default)]
struct State {
    log_handler: Option<ZqRawValue>,
    listeners: HashMap<String, ZqRawValue>,
}

impl ZqGcTrace for State {
    /// Mark every JS value reachable from here - that is the whole contract.
    fn trace(&self, gcx: &mut ZqGcTraceCtx) {
        if let Some(cb) = self.log_handler {
            let _ = gcx.mark(cb);
        }
        for cb in self.listeners.values() {
            let _ = gcx.mark(*cb);
        }
    }
}

static STATE: ZqRuntimeState<State> = ZqRuntimeState::new();

fn with_state<R>(ctx: ZqCtx, f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(ctx, f)
}

fn set_log_handler(ctx: ZqCtx, cb: ZqValue) -> Result<(), ZqError> {
    if !cb.is_callable() {
        return Err(ctx.throw_type("`handler` must be a function"));
    }
    let raw = cb.raw();
    with_state(ctx, |st| st.log_handler = Some(raw));
    Ok(())
}

fn emit(ctx: ZqCtx, msg: ZqValue) {
    let cb = with_state(ctx, |st| st.log_handler);
    if let Some(cb) = cb {
        ctx.call_event_handler(ZqValue::from(cb), ZqValue::undefined(), &[msg], "log");
    }
}

/// Runtime teardown. Must not call any JS API.
unsafe extern "C" fn finalize_runtime(rt: *mut ZqRuntime) {
    STATE.finalize(rt);
    // Last runtime out turns off the lights.
    if STATE.is_empty() {
        shutdown_process_wide_thing();
    }
}

#[zq_module(name = "myext", exports = [setLogHandler], finalize = finalize_runtime)]
fn module_init() {}
```

| call                    | does                                                                 |
| ----------------------- | -------------------------------------------------------------------- |
| `ZqRuntimeState::new()` | declares the slot; `const`, so it works in a `static`                |
| `STATE.with(ctx, f)`    | runs `f` against the calling runtime's `S`, creating it on first use |
| `STATE.finalize(rt)`    | drops that runtime's `S`; call from the module `finalize` hook       |
| `STATE.is_empty()`      | true once no runtime holds state, i.e. the last one is gone          |

- `S` must be `Default + ZqGcTrace`. A struct of plain Rust data needs only an
  empty `trace`.
- The closure must not re-enter the same slot or trigger a collection. It panics
  with a name rather than hanging. Touching other slots is fine.
- Validate stored callbacks. A stored non-function fails silently: delivery finds
  it uncallable and drops every event with no diagnostic.

Nothing allocates until the first `with`, so declaring several slots is free.

## 4. Code patterns

### 4.1 A class with state

The payload struct holds native state. Reach it through a helper so a foreign
`this` becomes a `TypeError` instead of a crash.

```rust
pub(crate) struct DbPayload {
    worker: Worker,
    path: String,
}

// `payload_no_gc` is an optimization, not a default. Add it only when no field
// can hold a JS value; unsure? leave it off (chapter 8).
#[zq_class(name = "Database", payload = DbPayload, payload_no_gc)]
pub struct JsDatabase;

fn payload<'a>(ctx: ZqCtx<'a>, this: ZqValue<'a>) -> Result<&'a DbPayload, ZqError> {
    JsDatabase::payload(ctx, this).ok_or_else(|| ctx.throw_type("not a Database"))
}

#[zq_class_impl]
impl JsDatabase {
    /// Throw unless construction goes through the factory.
    fn constructor<'a>(ctx: ZqCtx<'a>, _cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        Err(ctx.throw_type("use `await Database.open(path)` instead of `new Database()`"))
    }

    #[zq_static_method(name = "open")]
    fn open<'a>(ctx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        let path = cc.arg_string(ctx, 0)?;
        JsDatabase::new_instance(ctx, DbPayload { worker: Worker::spawn(), path })
    }

    #[zq_method]
    fn close<'a>(ctx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        let p = payload(ctx, cc.this())?;
        p.worker.shutdown();
        Ok(ZqValue::undefined())
    }
}
```

`JsDatabase::payload` returns `None` for anything that is not an instance, which
is why the helper is worth having once per class.

Any method may return a Promise - a method is not "sync" or "async", it just
returns whatever the body returns (see 4.3).

#### Inheritance

`extends = Base` names another `#[zq_class]` as the parent. The host chains both
sides at registration, so a child instance finds every parent member and
`Child.__proto__ === Base`.

```rust
#[zq_class(name = "Transaction", payload = TxPayload, extends = JsDatabase)]
pub struct JsTransaction;
```

```js
const t = await Database.open("db");
t.close(); // inherited from Database
t instanceof Database; // true — instanceof walks the prototype chain
Database.isOpen(t); // inherited static
```

- **Declaration order does not matter.** The child may be declared, exported or
  registered before its parent; the host links them once both are known.
- **`Base` must itself be a `#[zq_class]`** from the same extension. A parent
  from another dylib cannot be named: the child is registered as a class
  **without** a parent, and nothing warns you — no member of the class you meant
  is inherited.
- **A class with no `extends` is an ordinary root class.** Its prototype inherits
  `Object.prototype`, so a `Root` instance still reaches `hasOwnProperty`,
  `toString` and the rest of `Object.prototype`.
- **This is member-lookup inheritance, not `super`.** A `#[zq_method]` body is
  Rust and has no `super` syntax: a child that needs the parent's behaviour
  calls it by name — `JsDatabase::close(ctx, cc)` — rather than `super.close()`.
- **Payloads are independent.** `extends` chains prototypes; it does not make
  the child a valid `Base` for `Base::payload`. Give the child its own payload,
  or a shared one, deliberately.
- The reverse direction needs nothing: `class Derived extends NativeBase` in a
  script already works, because the constructor trampoline derives the instance
  prototype from `new_target.prototype`.

### 4.2 Returning values

| JS value                | how                                                                            |
| ----------------------- | ------------------------------------------------------------------------------ |
| `undefined`             | `Ok(ZqValue::undefined())`                                                     |
| `null`                  | `ZqValue::null()`                                                              |
| `true` / `false`        | `ZqValue::bool(b)`                                                             |
| number                  | `ZqValue::number(1.5)` or `ZqValue::int32(1)`                                  |
| string                  | `ZqValue::new_string(ctx, "hi")?`                                              |
| object                  | `ZqValue::new_object(ctx)?`                                                    |
| object with a prototype | `ZqValue::new_object_with_proto(ctx, proto)?`                                  |
| array                   | `ZqValue::new_array(ctx, &[a, b])?`                                            |
| bytes                   | `ZqValue::typed_array(ctx, &data)?` (chapter 4.8)                              |
| `Error`                 | `ZqValue::new_error(ctx, ZqErrorType::Type, "msg")?` then `ctx.throw_value(e)` |
| a native function       | `ZqValue::new_function(ctx, item::trampoline(), "name", magic)?`               |

Filling an object:

```rust
let obj = ZqValue::new_object(ctx)?;
obj.set_property_str(ctx, "size", ZqValue::number(meta.len() as f64))?;
obj.set_property_str(ctx, "name", ZqValue::new_string(ctx, &p.path)?)?;
Ok(obj)
```

### 4.3 One-shot async work

Any method that must not block the event loop returns a Promise.
`promise_threaded` runs a blocking call on a fresh OS thread; `convert` runs back
on the JS thread and builds the value.

```rust
#[zq_method]
fn readFile<'a>(ctx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
    let p = payload(ctx, cc.this())?;
    let path = p.path.clone();                    // own the data; JS values cannot cross
    ctx.promise_threaded(
        move || std::fs::read(&path).map_err(|e| e.to_string()),
        |ctx, res| match res {
            Ok(bytes) => ZqValue::typed_array(ctx, &bytes),
            Err(e) => Err(ctx.throw_internal(&e)),
        },
    )
}
```

If the work never sends a result the Promise rejects, so it cannot hang the loop.

**Async runtimes need an executor you supply.** `promise_fut` will not pick one -
zapi does not know whether your code runs on tokio, async-std, smol, or something
else, and hardcoding one would either force a dependency you do not use or break
under the one you do. Name it once at module init:

```rust
#[zq_module(name = "myext", exports = [...])]
fn module_init(ctx: ZqCtx) -> Result<(), ZqError> {
    ctx.set_default_executor(|fut| tokio::spawn(fut));
    Ok(())
}
```

- The executor is a plain `fn` pointer and cannot capture. Keep your runtime in a
  `static` and submit from there.
- First registration wins; a second call is ignored.
- Calling `promise_fut*` with no executor throws `InternalError: no default
executor`.

If a global default does not suit you - it hides a dependency in ambient state,
and different calls may legitimately need different executors - pass it per call
instead, with no `module_init` step:

```rust
ctx.promise_fut_on(work, |fut| tokio::spawn(fut), convert)
```

| family                | JS gets                      | work runs on                                 |
| --------------------- | ---------------------------- | -------------------------------------------- |
| `promise_threaded`    | Promise                      | a fresh OS thread                            |
| `promise_fut`         | Promise                      | the executor registered at init              |
| `promise_fut_on`      | Promise                      | an executor passed per call                  |
| `promise_fut_result*` | Promise                      | as above; rejects with the error's `Display` |
| `spawn_event_job`     | nothing; you keep the sender | event-loop job, does **not** hold the loop   |
| `spawn_channel_job`   | nothing; you keep the sender | event-loop job, **holds** the loop           |
| `new_async_iterator`  | AsyncIterable                | polled by the consumer                       |

### 4.4 Long-lived worker thread per instance

`promise_threaded` starts a thread per _call_, which is wrong for an object with
heavy state - a database handle, a parser, an engine. Building that state costs
far more than any single call. So build it once on its own thread and let later
calls post messages.

```
JS thread                            worker thread (one per instance)
---------                            ------------------------------
send job          ---------------->   owns all native objects
await reply       <----------------   replies with Rust data only
```

The worker never touches a JS value; the JS thread never touches the native
objects. They exchange Rust data over one channel, and the reply is converted on
the JS side.

Wrap it so call sites do not repeat the plumbing:

```rust
pub(crate) struct Worker {
    tx: ZqWakeSender<Job>,
}

impl Worker {
    pub(crate) fn spawn() -> Self { /* channel + thread::spawn(worker_thread) */ }

    /// Fire and forget.
    pub(crate) fn try_send(&self, job: Job) -> bool { self.tx.send(job).is_ok() }

    /// Send a job and await its reply as a Promise.
    pub(crate) fn promise<'a, C>(&self, ctx: ZqCtx<'a>, job: Job, convert: C)
        -> Result<ZqValue<'a>, ZqError>
    where
        C: for<'b> FnOnce(ZqCtx<'b>, Reply) -> Result<ZqValue<'b>, ZqError> + 'static,
    { /* ctx.promise_threaded(blocking recv, convert) */ }
}
```

Use `ZqWakeSender` (from `ctx.make_wake_channel`), never a bare `mpsc::Sender`:
every send and every drop wakes the event loop, so a producer cannot forget to
wake and strand the job waiting for its message. It clones and is `Send` for any
message type, so give one clone to each producer.

**Bound the queue.** `try_send` never blocks, so a slow consumer grows the queue
without limit. Drop the incoming item and count it once the queue lag (age of the
oldest item) passes a threshold.

### 4.5 Streaming callbacks to JS

A promise answers one question and settles. This is the other shape: a
subscription. Native code produces events whenever it likes and each goes to a JS
callback.

```rust
let tx = ctx.spawn_event_job("mylog", move |ctx, msg| {
    ctx.call_event_handler(cb, ZqValue::undefined(), &[msg], "mylog");
});
```

`spawn_event_job` registers the draining job and hands you a sender; every
`send` wakes the loop and delivers one message to your handler.

|                      | `spawn_event_job`                | `spawn_channel_job`            |
| -------------------- | -------------------------------- | ------------------------------ |
| holds the event loop | **no**                           | yes                            |
| drains               | the whole channel per tick       | one message per tick           |
| models               | an event stream that may be idle | work that is genuinely pending |

A program whose only pending work is a registered listener must still be able to
exit; a holding job never lets it. So use `spawn_event_job` for anything fed by
events rather than by a request that will finish.

- Use `call_event_handler`, not `call_function`. A listener's throw has nowhere
  to go, and `call_event_handler` clears it and tells you it threw, so one bad
  listener cannot abort the tick.
- Register the job only when someone is listening.
- **Validate the callback argument.** A stored non-function fails silently -
  delivery drops every event with no diagnostic. Check `is_callable()` at
  registration, where JS can still see the error.
- **Re-registering must replace, not stack.** `onLog(cb)` twice must not deliver
  each event to both callbacks. Keep the listener's task handle in your state and
  abort the old one; dropping its sender disconnects the old job. Key the
  replacement by whatever identifies the listener's target, or one target's
  listener evicts another's.
- If the callback handle came from JS and JS may drop it, keep it in a GC-traced
  job payload (chapter 8).

### 4.6 Async iterator

For a stream of results - query rows, a directory walk - rather than one value.
Materialising a large or open-ended set to return it as one array is wrong; an
`AsyncIterable` lets `for await (... of ...)` pull one item at a time.

```rust
impl ZqAsyncIterable for Scan {
    fn next<'a>(&mut self, ctx: ZqCtx<'a>) -> Result<AsyncIterResult<'a>, ZqError> {
        match self.next_item() {
            Some(v) => Ok(AsyncIterResult::Value(v)),
            None => Ok(AsyncIterResult::Done),
        }
    }
}
ctx.new_async_iterator(Scan { .. })
```

- `Value` yield it, `Done` the stream finished, `Wait` nothing ready yet (the
  consumer retries next tick - this is how a not-yet-ready source avoids
  blocking).
- `new_async_iterator` takes ownership: the value is dropped when the consumer
  stops, including on an early `break`. Release anything native it holds there.

### 4.7 Properties, accessors, and callbacks

`#[zq_method(get)]` / `#[zq_method(set)]` make a method a property accessor, so it
reads as a field:

```rust
#[zq_method(get, name = "rootNode")]
fn root_node<'a>(ctx: ZqCtx<'a>, cc: ZqCallInfo) -> Result<ZqValue<'a>, ZqError> { /* ... */ }

#[zq_method(set, name = "pageScale")]
fn set_page_scale<'a>(ctx: ZqCtx<'a>, cc: ZqCallInfo) -> Result<ZqValue<'a>, ZqError> {
    let factor = cc.arg_f64(ctx, 0)?;
    // ... apply it ...
    Ok(ZqValue::undefined())      // setters return undefined
}
```

Prefer an accessor over a `getX()` method when the value is a plain attribute:
`tree.rootNode` beats `tree.getRootNode()`.

Reading fields off an object JS handed you:

| need                | call                                                     |
| ------------------- | -------------------------------------------------------- |
| a string field      | `obj.get_string_str(ctx, "k")? -> Option<String>`        |
| a number field      | `obj.get_f64_str(ctx, "k")? -> Option<f64>`              |
| a bool field        | `obj.get_bool_str(ctx, "k")? -> Option<bool>`            |
| a nested object     | `obj.get_object_str(ctx, "k")? -> Option<ZqValue>`       |
| a nested array      | `obj.get_array_str(ctx, "k")? -> Option<ZqValue>`        |
| required string     | `obj.get_property_str(ctx, "k")?` (throws if absent)     |
| presence            | `obj.has_property_str(ctx, "k")?`                        |
| call a method on it | `obj.invoke_str(ctx, "method", &[a, b])?`                |
| build an array      | `ZqValue::new_array(ctx, &[a, b])?`, or `push` in a loop |

`new_function` is how you attach a native function as a value on an object you
are building - the only way to put one there:

```rust
let f = ZqValue::new_function(ctx, js_accept::trampoline(), "accept", magic)?;
obj.set_property_str(ctx, "accept", f)?;
```

#### Reaching a class prototype

**Declare members. Installing them at runtime is the fallback, not the way to
write it.** Work down this list and stop at the first that fits:

|     | form                                      | use when                                            |
| --- | ----------------------------------------- | --------------------------------------------------- |
| 1   | `#[zq_method]` / `#[zq_static_method]`    | the name is a literal — always the default          |
| 2   | `#[zq_class(prototype_members = T)]`      | names come from a table you own and it is a `const` |
| 3   | `ctx.class_install_prototype_members(..)` | the names only exist once the host is running       |

Step 3 is a fallback because it is the only one of the three that can be wrong in
ways the compiler cannot see. It is a runtime call, so a name that collides with
something already installed throws _after_ the members before it have landed —
the class ends up half-built. A declared member or a `const` table is installed
while the class is being set up, so a collision refuses the whole class instead
of leaving it patched. A name known at compile time also has exactly one place to
be wrong; a runtime table has the table, the dispatcher, and whatever computed
the magic.

Reach for step 3 only when the names genuinely arrive from the host — a
reflection table over a foreign API, a config file, a plugin manifest. The test
to apply is: _could this table be a `const`?_ If yes, it should be.

`ZqCtx::class_prototype` takes the id `register_class` returned:

```rust
let class_id = JsElement::register_id(ctx)?;      // `register` throws the id away
let proto = ctx.class_prototype(class_id)?;
```

The two table forms:

```rust
// a `&'static [ZqMemberDef]` const, spliced in at compile time
#[zq_class(name = "Element", prototype_members = STYLE_MEMBERS)]
pub struct JsElement;

// or installed at runtime, for names only known once the host runs
ctx.class_install_prototype_members(class_id, &[
    ZqMember::method("flex", style_op::trampoline(), 0),
    ZqMember::method("w", style_op::trampoline(), 1),
])?;
```

Whatever the form, `magic` is yours to assign and `cc.magic()` hands it back, so
the dispatcher reads the same order the names came from and there is no second
numbering to keep in step. Keep the names and the dispatch in one table:

```rust
pub const STYLES: &[StyleDef] = &[def(c"flex", Style::Fixed(StyleOp::Flex)), /*…*/];

// one line, magic == index, arity from the variant
pub const STYLE_MEMBERS: [ZqMemberDef; STYLES.len()] =
    to_members(STYLES, style_op::trampoline());
```

`to_members` is your own ten-line `const fn` over that table — zapi has no such
helper, because it would need to know your `StyleDef`. Writing the `ZqMemberDef`
table out by hand instead puts each name in two places and every magic in a third.

Prefer step 1 over step 2 where it fits: a handful of individually written methods
read better as `#[zq_method]` fns, where the compiler checks each signature. Step
2 is for a surface that is _already_ a table — a list of styles, a state enum, a
descriptor list — so that adding an entry is one line in the table you maintain
anyway.

`register_id` exists only because `Cls::register` returns `()`. Prefer
`register` unless you need the id.

**Names must be new.** Both forms refuse a name the class already has, and the
refusal is the point rather than a fallback:

```rust
#[zq_class(name = "Tabled", prototype_members = T)]   // T claims "declared"
struct Tabled;
#[zq_class_impl]
impl Tabled {
    #[zq_method(name = "declared")] fn declared(...) { }   // a TypeError, by name
}
```

```
TypeError: member 'declared' is declared twice on class id 42
```

A name may appear once across everything a class declares:

| collision                                                                                        | refused by                     |
| ------------------------------------------------------------------------------------------------ | ------------------------------ |
| twice in one table                                                                               | the host, at install           |
| a table name vs `#[zq_method]`                                                                   | the host, at class init        |
| a table name vs `#[zq_static_method]`                                                            | the host, at class init        |
| a `#[zq_method]` vs `#[zq_static_method]`                                                        | **the macro, at compile time** |
| a name the prototype already owns                                                                | the host, at install           |
| a reserved name — `constructor` on the prototype, `length`/`name`/`prototype` on the constructor | the host, at class init        |

**Why refuse when the engine could replace.** The engine's ordinary redefine
path (`define_property`, via `check_define_prop_flags`) does replace a
configurable property, and zapi members are installed configurable — so an
overwrite would be _possible_. It is refused on purpose: a name appearing twice
in your own class is almost always a mistake, and silently keeping the last one
hides it.

At the layer underneath it is not even a choice. `define_lazy_property` appends
a slot rather than merging, so a duplicate would leave the shape and the value
table disagreeing on how many slots there are — silent corruption. It now
returns `Err` on an existing name rather than asserting, which is why this
refusal reaches a `TypeError` with the member's name in it instead of a
debug-build abort.

A class registered twice by the same extension gets **two** class ids, and
`new_instance` resolves to the first — so a second `register_id` plus a second
install lands on a different prototype than the first. Install once.

### 4.8 Binary data

For anything crossing as raw bytes - audio, images, blobs, wire protocols. JS
represents these as ArrayBuffer and the typed arrays.

| direction              | call                                                          |
| ---------------------- | ------------------------------------------------------------- |
| bytes -> JS            | `ZqValue::typed_array(ctx, &data)?` for `&[f32]`/`&[u8]`/etc. |
| bytes -> a chosen view | `ZqValue::new_buf(ctx, ZqBufKind::TypedArrayUint8, &bytes)?`  |
| JS -> bytes            | `val.buf_bytes(ctx)? -> Vec<u8>`                              |
| what did JS pass?      | `val.get_buf_type(ctx)? -> Option<ZqBufKind>`                 |

Check the kind before reading: it tells you whether the value is a buffer at all
and which view it is, so you can reject a wrong type with a `TypeError` instead of
mis-reading it.

## 5. Arguments

Every parameter needs an explicit, strict check.

1. **Only `undefined` means omitted.** `cc.arg(n)` yields `undefined` past the
   end of the list. `null` is a value: under a strict type it is a `TypeError`,
   never a default.
2. **Type mismatch -> `TypeError`.** Check `kind()` before converting.
3. **Right type, illegal value -> `RangeError`.**
4. **Validate before side effects**, and the whole argument list before
   acquiring anything.
5. **Name the parameter**: `` `timeout` must be a positive integer ``.
6. **No implicit coercion, truncation, or clamping.** Reject `"5"` for a number,
   `1.5` for an integer, `Infinity` for a size.

```rust
// wrong: coerces, defaults, silently truncates
let timeout = cc.arg(0).to_f64(ctx).unwrap_or(30.0) as i32;

// right
let timeout = if cc.arg(0).is_undefined() { 30 } else {
    let f = cc.arg_f64(ctx, 0)?;
    if !f.is_finite() || f.fract() != 0.0 || f <= 0.0 {
        return Err(ctx.throw_range("`timeout` must be a positive integer"));
    }
    f as i32
};
```

Readers: `cc.arg(n)`, `arg_f64`, `arg_i32`, `arg_bool`, `arg_string`,
`arg_enum(ctx, n, &["a", "b"], "mode")`, `cc.is_empty()`, `cc.len()`, `cc.this()`,
`cc.args()`.

Options objects: reject a non-object with a `TypeError`, then read each key and
treat only `undefined` as absent.

### The `string_read_utf8` trap

`string_read_utf8` is a **strict read**, and on failure it leaves a `TypeError`
pending on the context. Swallow that `Err` and the next native call that returns
`Ok` will trip an assertion - a confusing crash far from the cause.

| you want                          | use                                                  |
| --------------------------------- | ---------------------------------------------------- |
| the value is known to be a string | `string_read_utf8`, propagate with `?`               |
| coerce, like `String(x)`          | `coerce_string`                                      |
| tolerate a non-string             | `try_string_read_utf8` (clears it, returns `Option`) |

## 6. Errors

`ctx.throw_*` raises a typed JS error and returns a marker, so it composes with
`?`:

```rust
let n = value.ok_or_else(|| ctx.throw_range("out of range"))?;
```

| helper                     | JS error         |
| -------------------------- | ---------------- |
| `ctx.throw_type(msg)`      | `TypeError`      |
| `ctx.throw_range(msg)`     | `RangeError`     |
| `ctx.throw_reference(msg)` | `ReferenceError` |
| `ctx.throw_syntax(msg)`    | `SyntaxError`    |
| `ctx.throw_uri(msg)`       | `URIError`       |
| `ctx.throw_eval(msg)`      | `EvalError`      |
| `ctx.throw_internal(msg)`  | `InternalError`  |
| `ctx.throw(kind, msg)`     | any of the above |

- `Err(ZqError)` from a body means "a JS exception is already pending".
- **Never swallow an error.** Not with `let _ =`, not with `.ok()`, not with
  `.unwrap_or_default()`. Propagate with `?` - in a `promise_*` convert closure
  the pending exception becomes the rejection reason; in an ordinary body it
  propagates.
- The one exception: `let _ = gcx.mark(raw)` inside `trace`, which has no error
  channel and cannot fail.
- The one legitimate place to clear an exception is where you cannot propagate
  it. Use `call_event_handler`, which does it for you.
- To throw a rich error object, build it and raise it:
  `ctx.throw_value(ZqValue::new_error(ctx, ZqErrorType::Type, "msg")?)?`.

## 7. Threading

1. **JS values never cross a thread boundary.** Workers move Rust data only;
   convert on the JS thread.
2. **`ZqCtx` is valid for the current tick only.** Never store one - take it as a
   parameter each time.
3. **Everything JS-facing runs on the JS thread**: building values, property
   access, calling functions. A `convert` closure is the designated place.
4. **Always route results back through a wake channel.** A worker that computes
   and then stays silent parks the loop forever.
5. **Keep per-runtime data per-runtime** - use `ZqRuntimeState` (chapter 3), not
   a bare `static` or `thread_local!`.

## 8. GC tracing

The collector sees only the JS heap. A `ZqRawValue` (an integer handle) stored
in your Rust struct is invisible to it, so an unmarked one is unreachable as far
as the collector is concerned and gets freed while you still hold it.

Native code can hold a JS value in three places, and each needs its own answer to
"who marks it?":

| holds JS values in          | marked by                                                   |
| --------------------------- | ----------------------------------------------------------- |
| a class payload             | the class, automatically - unless you write `payload_no_gc` |
| a `ZqJob` payload           | `add_job`, automatically and unavoidably                    |
| a `static` / `thread_local` | you, via `ZqRuntimeState` (chapter 3)                       |

### Does this field need a mark?

1. Does it hold a `ZqRawValue` (directly, or inside an `Option`/`Vec`/`Box`)?
   Then it needs a mark. No exceptions.
2. Is that JS value reachable from JS by any other path right now? If not, the
   mark is the _only_ thing keeping it alive - the usual case for a callback
   passed as an argument, since the object literal holding it was temporary.
3. Unsure? It needs a mark.

> A field needs a mark if it stores a `ZqRawValue` **and** nothing else keeps
> that JS value reachable.

`String`, integers, `bool`, `f64`, pointers, `Sender`/`Receiver` and `Arc` never
need one.

### Marking a class payload

```rust
#[zq_class(name = "Database", payload = DbPayload)]   // note: no payload_no_gc
pub struct JsDatabase;

impl ZqGcTrace for DbPayload {
    fn trace(&self, gcx: &mut ZqGcTraceCtx) {
        if let Some(raw) = self.on_change { let _ = gcx.mark(raw); }
    }
}
```

Or `#[derive(ZqGcTrace)]` on the payload, which marks every field and forwards
through `Option`, `Vec` and `Box`.

**`payload_no_gc` is an optimization, not a default.** A traced payload is always
correct; an untraced one is an assertion you are making. Omitting it costs a
per-instance indirect call per collection, so leave it off unless you have
checked every field - and everything reachable through them - and confirmed none
can hold a JS value.

The two mistakes are not symmetric:

| choice                              | cost when wrong      |
| ----------------------------------- | -------------------- |
| traced payload holding no JS value  | a little time        |
| untraced payload holding a JS value | **the process dies** |

So the attribute is a reviewable claim ("nothing here is a JS value"). Adding a
`ZqRawValue` later without removing it turns the claim into a lie that nothing
will tell you about.

### Marking a job payload

```rust
impl ZqGcTrace for StreamJob {
    fn trace(&self, gcx: &mut ZqGcTraceCtx) {
        if let Some(raw) = self.on_partial { let _ = gcx.mark(raw); }
        if let Some(raw) = self.on_final   { let _ = gcx.mark(raw); }
        if let Some(raw) = self.on_error   { let _ = gcx.mark(raw); }
    }
}
```

Three callbacks means three marks; the one you skip is the one that crashes. For a
job that provably holds no JS value, `ctx.add_job_no_gc(closure)` is the shortcut.

### What a missing mark looks like

Not a JS exception - the process dies: `non-unwinding panic, aborting`, or in a
release build a silent jump through a garbage pointer. Do not go looking in
exception handling and do not try to catch it in JS. Debug builds turn it into a
type assertion, which is what makes it findable.

### Checking a mark actually works

A mark that has never been exercised is indistinguishable from one that does
nothing, and the failure it invites is not catchable in JS. So when you add one,
confirm it does something:

- **The callback must have no reference in JS.** Pass an inline arrow, never a
  bound variable - a variable keeps the callback rooted and the mark untested:
  ```js
  register({ onEvent: () => {} }); // the mark is the only path
  const cb = () => {};
  register({ onEvent: cb }); // JS roots it; nothing is being tested
  ```
- **Force real collections** with `Aike.gc()`. There is no bare `gc()` and no
  `globalThis.gc`.
- **Check enough collections actually happened**, otherwise nothing was proven.
- Then remove the mark, rebuild, and confirm the callback stops arriving. A check
  that passes both with and without the mark is not checking anything.

## 9. Pitfalls

| pitfall                                                               | consequence                                                | rule                                                                   |
| --------------------------------------------------------------------- | ---------------------------------------------------------- | ---------------------------------------------------------------------- |
| `payload_no_gc` on a payload holding a JS value                       | process dies                                               | unsure? leave it off - chapter 8                                       |
| forgetting one `mark` among several                                   | crash on that callback only                                | mark every handle                                                      |
| `string_read_utf8` error swallowed                                    | pending exception trips a later assert                     | propagate, or use `coerce_string` / `try_string_read_utf8`             |
| `let _ =` on a fallible call                                          | error silently lost                                        | propagate with `?`; only `mark` in `trace`                             |
| storing a `ZqCtx`                                                     | dangling context                                           | take it per tick                                                       |
| sending a JS value to a worker thread                                 | engine data race                                           | convert on the JS thread                                               |
| bare `mpsc::Sender` instead of `ZqWakeSender`                         | lost wakeup, loop parks forever                            | every send and drop must wake                                          |
| unbounded queue, no backpressure                                      | silent memory growth                                       | bound it and count drops                                               |
| `null` treated as omitted                                             | wrong contract, silent default                             | only `undefined` is omitted                                            |
| installing a member at runtime when the name is known at compile time | half-installed class, name in two places, magic in a third | declare it: `#[zq_method]`, else `#[zq_class(prototype_members = ..)]` |
| implicit coercion or truncation                                       | wrong values, no error                                     | strict checks; `TypeError` vs `RangeError`                             |
| a holding job for an event stream                                     | program never exits                                        | `spawn_event_job`                                                      |
| listener registered twice                                             | every event delivered twice                                | replace and abort the old task                                         |
| callback argument not validated                                       | events silently dropped                                    | check `is_callable()` at registration                                  |
| `ZqRuntimeState::with` re-entering the same slot                      | panic, by design                                           | do not call the same slot inside `f`                                   |
| finalize hook calling a JS API                                        | crash at teardown                                          | drop state, do not trace it                                            |
| resolving a bundled resource against the cwd                          | works in dev, missing in the install                       | resolve against `module_path()`'s directory                            |
| stale build after a scripted edit                                     | you test the wrong binary                                  | `touch` + delete artifact                                              |
| release build                                                         | memory bugs become silent corruption                       | debug everywhere                                                       |

## 10. API map

Grouped by the type that owns the method. Every name below is exact - if it is not
in this table, it is not a `pub` method of `zuqe_zapi`.

### 10.1 `ZqCtx` - the context handle

| need                                         | call                                                                     |
| -------------------------------------------- | ------------------------------------------------------------------------ |
| globals                                      | `get_global_str`, `set_global_str`                                       |
| call a function / a constructor              | `call_function`, `call_constructor`                                      |
| call a callback whose throw cannot propagate | `call_event_handler` - returns whether it threw and clears the exception |
| JSON                                         | `json_parse`, `json_stringify`                                           |
| class registration                           | `register_class`, `new_instance`                                         |
| a wake handle for another thread             | `wake_handle` -> `ZqWakeHandle`                                          |
| this runtime's identity                      | `runtime` -> `ZqRuntime::id`                                             |
| trace values held outside the JS heap        | `gc_set_trace_func`                                                      |

### 10.2 `ZqCallInfo` - arguments

| need                                         | call                                           |
| -------------------------------------------- | ---------------------------------------------- |
| receiver and arity                           | `this`, `len`, `is_empty`                      |
| iterate the arguments                        | `args`                                         |
| the raw argument (past the end: `undefined`) | `arg(n)`                                       |
| checked readers -> `TypeError`               | `arg_f64`, `arg_i32`, `arg_bool`, `arg_string` |
| a string enum -> `RangeError`                | `arg_enum(ctx, n, &["a", "b"], "name")`        |
| which stacked JS name was called             | `magic`                                        |

### 10.3 `ZqValue` - inspect and convert

| need                                        | call                                                                                                                      |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| identity and type                           | `raw`, `kind` -> `ZqValueType`, `strict_eq`                                                                               |
| predicates                                  | `is_undefined`, `is_null`, `is_nullish`, `is_bool`, `is_nan`, `is_exception`, `is_callable`, `is_constructor`, `is_array` |
| `instanceof` a constructor / a native class | `is_instance_of(ctx, target)`, `is_instance_of_class(ctx, class)`                                                         |
| coercions that may throw                    | `to_f64`, `to_i64`, `to_i32`, `to_bool`, `to_jsstring`                                                                    |
| shape checks, no `?`                        | `as_object`, `as_array`, `as_function`                                                                                    |
| array length / append                       | `length`, `push`                                                                                                          |

### 10.4 `ZqValue` - build

| need                          | call                                                                          |
| ----------------------------- | ----------------------------------------------------------------------------- |
| primitives, no context needed | `undefined`, `null`, `bool`, `int32`, `number`, plus `From<i32/u32/f64/bool>` |
| string, symbol                | `new_string`, `new_symbol`                                                    |
| bigint                        | `new_bigint_i64`, `new_bigint_u64`                                            |
| object, array                 | `new_object`, `new_object_with_proto`, `new_array`, `array_from_iter`         |
| error value, native function  | `new_error`, `new_function(ctx, f::trampoline(), "name", magic)`              |
| bytes                         | `typed_array::<T>`, `new_buf`, `new_array_buffer`, `ZqBufKind`                |
| read bytes back / what is it  | `buf_bytes`, `get_buf_type`                                                   |

### 10.5 `ZqValue` - strings

| need                                                              | call                   |
| ----------------------------------------------------------------- | ---------------------- |
| strict read; a non-string throws and leaves the exception pending | `string_read_utf8`     |
| strict read, exception cleared                                    | `try_string_read_utf8` |
| `String(x)` semantics                                             | `coerce_string`        |
| a symbol's description                                            | `symbol_description`   |

### 10.6 `ZqValue` - properties

Each access has two forms: a key that is already a `ZqValue`, and a `_str` form
taking a UTF-8 `&str` (no intermediate JS string).

| need                                 | value key                                                                                                | UTF-8 key                                       |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| read                                 | `get_property`                                                                                           | `get_property_str`                              |
| write                                | `set_property`                                                                                           | `set_property_str`                              |
| exists                               | `has_property`                                                                                           | `has_property_str`                              |
| delete                               | `del_property`                                                                                           | `del_property_str`                              |
| define a data property (`ZQ_PROP_*`) | `define_property`                                                                                        | `define_property_str`                           |
| define a getter/setter               | `define_accessor`                                                                                        | `define_accessor_str`                           |
| read as string / number / bool       | `get_string`, `get_f64`, `get_bool`                                                                      | `get_string_str`, `get_f64_str`, `get_bool_str` |
| read as object / array               | `get_object`, `get_array`                                                                                | `get_object_str`, `get_array_str`               |
| the same, clearing the exception     | `try_get_*`                                                                                              | `try_get_*_str`                                 |
| read as an enum (-> `RangeError`)    | -                                                                                                        | `get_enum`                                      |
| integer index                        | `get_index`, `set_index`                                                                                 | -                                               |
| set and chain                        | `with`                                                                                                   | -                                               |
| own-property introspection           | `own_property_descriptor`, `own_property_descriptor_str`, `own_property_names` -> `ZqPropertyDescriptor` |                                                 |

### 10.7 Calling out of a value

| need                                | call                       |
| ----------------------------------- | -------------------------- |
| `obj.method(...)` with `this = obj` | `invoke`, `invoke_str`     |
| `fn(...)` / `new Cls(...)`          | `call`, `call_constructor` |

### 10.8 Errors

| need                               | call                                                                                                                               |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| raise, then propagate with `?`     | `ctx.throw_type`, `throw_range`, `throw_reference`, `throw_syntax`, `throw_uri`, `throw_eval`, `throw_internal`, `throw_aggregate` |
| raise any of the above             | `ctx.throw(ZqErrorType, msg)`                                                                                                      |
| raise a value you built            | `ctx.throw_value(val)`                                                                                                             |
| read / clear the pending exception | `ctx.exception`, `ctx.take_exception`                                                                                              |

### 10.9 Async and the event loop

| need                                    | call                                                                                          |
| --------------------------------------- | --------------------------------------------------------------------------------------------- |
| one result, a fresh OS thread           | `promise_threaded`, `promise_threaded_void`                                                   |
| one result, your async runtime          | `promise_fut`, `promise_fut_void`, `promise_fut_string`, `promise_fut_json`, `promise_fut_on` |
| `Result` instead of a value             | `promise_fut_result`, `promise_fut_result_on`                                                 |
| name the executor once                  | `set_default_executor` (`ZqExecutor`)                                                         |
| a promise you settle by hand            | `new_promise_capability` -> `ZqPromiseCapability { promise, resolve, reject }`                |
| a persistent polling job                | `add_job` (`ZqJob`), `add_job_no_gc` -> `ZqPoll::{Done, Again, AgainNoHold}`                  |
| native -> JS messages                   | `make_wake_channel` -> `ZqWakeSender::send`; `ZqWakeHandle::{wake, share}`                    |
| drained messages, holds the loop        | `spawn_channel_job` -> `ZqChannelPoll::{Continue, Done}`                                      |
| an event stream, does not hold the loop | `spawn_event_job`                                                                             |
| a pull-based stream                     | `new_async_iterator` (`ZqAsyncIterable`) -> `AsyncIterResult::{Value, Done, Wait}`            |

### 10.10 Keeping JS values alive

| need                           | call                                              |
| ------------------------------ | ------------------------------------------------- |
| a payload that holds JS values | `impl ZqGcTrace` or `#[derive(ZqGcTrace)]`        |
| mark one handle                | `gcx.mark(raw)` on `ZqGcTraceCtx`                 |
| values the engine cannot see   | `ctx.gc_set_trace_func`                           |
| one `S` per runtime            | `ZqRuntimeState::{new, with, finalize, is_empty}` |
| handle types                   | `ZqRawValue`, `ZqRuntime`                         |

### 10.11 The declared surface

| need                                                                                                                                                                                                                                                                                   | call                                                                                                                                               |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| proc macros                                                                                                                                                                                                                                                                            | `#[zq_module]`, `#[zq_function]`, `#[zq_export]`, `#[zq_class]`, `#[zq_class_impl]`, `#[zq_method]`, `#[zq_static_method]`, `#[derive(ZqGcTrace)]` |
| generated per class                                                                                                                                                                                                                                                                    | `Cls::{class_def, register, register_id, new_instance, payload, payload_mut, take_payload}`, plus `item`, `items`                                  |
| generated per function                                                                                                                                                                                                                                                                 | `f::{trampoline, call, item, items}`                                                                                                               |
| a prototype whose members come from a table                                                                                                                                                                                                                                            | `#[zq_class(prototype_members = ..)]` (compile time), or `class_install_prototype_members` (runtime)                                               |
| a class value back as a handle                                                                                                                                                                                                                                                         | `Class::from_def` (unsafe)                                                                                                                         |
| the loaded dylib's resolved path (not the name JS imported it by), `None` if the host gave none. Its parent is where to resolve bundled resources; the same string is the injected `__file__`. Call once in `module_init` and cache it; copy resources beside the dylib in production. | `module_path`                                                                                                                                      |

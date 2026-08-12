//! Macro expansion smoke tests.
//!
//! This test crate depends only on `zapi` + `zapi-macros` (no engine/host
//! crate), so it is the "zapi-macros works without zuqe" guarantee: if the
//! macro-generated code ever referenced host types, this would fail to compile.

use zuqe_zapi::prelude::*;
use zuqe_zapi_macros::{
    ZqGcTrace, zq_class, zq_class_impl, zq_export, zq_function, zq_method, zq_module,
    zq_static_method,
};

// ── #[zq_function]: a leaf callable from JS ──

#[zq_function(name = "add", magic = 7)]
fn add<'a>(cx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
    let a = cc.arg(&cx, 0).raw();
    let b = cc.arg(&cx, 1).raw();
    Ok(cx.wrap_value(a + b))
}

// ── #[zq_class] + #[zq_class_impl] + #[zq_method] + #[derive(ZqGcTrace)] ──

#[zq_class(name = "Bag", payload = BagPayload)]
struct Bag;

#[zq_class_impl]
impl Bag {
    fn constructor<'a>(cx: ZqCtx<'a>, _cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        Bag::new_instance(&cx, BagPayload { item: 0 })
    }

    #[zq_method]
    fn size<'a>(cx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        let b = cc.arg(&cx, 0);
        Bag::payload_mut(&cx, b).ok_or_else(|| ZqError::new(ZqStatus::Error, None))?;
        Ok(cx.number(1.0))
    }

    // Accessor combinations: get, get/set, set.
    #[zq_method(get, set)]
    fn label<'a>(cx: ZqCtx<'a>, _cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        Ok(cx.undefined())
    }

    #[zq_method(get)]
    fn count<'a>(cx: ZqCtx<'a>, _cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        Ok(cx.number(0.0))
    }

    #[zq_method(set)]
    fn reset<'a>(cx: ZqCtx<'a>, _cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        Ok(cx.undefined())
    }

    // Magic dispatch: one body, two JS names.
    #[zq_method(name = "push", magic = 1)]
    #[zq_method(name = "add_item", magic = 2)]
    fn push_item<'a>(cx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        Ok(cx.number(cc.magic() as f64))
    }

    // Static (constructor) method / accessor.
    #[zq_static_method]
    fn is_bag<'a>(cx: ZqCtx<'a>, cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        Ok(cx.bool(Bag::payload(&cx, cc.arg(&cx, 0)).is_some()))
    }

    #[zq_static_method(get, set)]
    fn tag<'a>(cx: ZqCtx<'a>, _cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        Ok(cx.undefined())
    }
}

#[derive(ZqGcTrace)]
struct BagPayload {
    item: ZqRawValue,
}

// A class without `payload` (no native state, like def_native_object's `()`).
#[zq_class(name = "Marker")]
struct Marker;

#[zq_class_impl]
impl Marker {
    fn constructor<'a>(cx: ZqCtx<'a>, _cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        Marker::new_instance(&cx)
    }
}

// Payload with no GC nodes: `payload_no_gc` opts out of the trace wiring (and
// the payload type does not even need a ZqGcTrace impl).
struct PlainState {
    n: i32,
}

#[zq_class(name = "Plain", payload = PlainState, payload_no_gc)]
struct Plain;

#[zq_class_impl]
impl Plain {
    fn constructor<'a>(cx: ZqCtx<'a>, _cc: ZqCallInfo<'a>) -> Result<ZqValue<'a>, ZqError> {
        Plain::new_instance(&cx, PlainState { n: 0 })
    }
}

#[zq_export(name = "VERSION")]
fn version<'a>(cx: ZqCtx<'a>) -> Result<ZqValue<'a>, ZqError> {
    Ok(cx.string("1.0"))
}

// Standard paradigm: inline `exports = [...]` list (macro wraps each item).
// `init` is optional.
#[zq_module(name = "macro_test", exports = [add, Bag, Marker, version])]
fn module_init() {}

// ── tests ──

#[test]
fn zq_function_expands() {
    let item = add::item();
    assert_eq!(item.name_str(), Some("add"));
    assert_eq!(item.kind, zuqe_zapi::ffi::ZQ_FIELD_FUNC);
    assert_eq!(item.magic, 7);
    assert!(item.body.is_some());
}

#[test]
fn zq_class_expands() {
    let def = Bag::class_def();
    assert_eq!(def.call_arity, 0);
    assert!(!def.name.is_null());
    assert!(def.gc_trace.is_some(), "payload class defaults to a wired gc_trace");

    // ABI class members are unified ZqFieldDef entries now: methods and
    // accessors interleaved in one slice per scope (prototype vs constructor).
    let proto = fields(def.proto_entries, def.proto_entries_len);
    assert_eq!(cname(proto[0].name), "size");
    assert_eq!(proto[0].kind, zuqe_zapi::ffi::ZQ_FIELD_FUNC);
    assert_eq!(cname(proto[1].name), "label");
    assert_eq!(proto[1].kind, zuqe_zapi::ffi::ZQ_FIELD_GETSET);
    assert_eq!(cname(proto[2].name), "count");
    assert_eq!(proto[2].kind, zuqe_zapi::ffi::ZQ_FIELD_GETSET);
    assert_eq!(cname(proto[3].name), "reset");
    assert_eq!(cname(proto[4].name), "push");
    assert_eq!(proto[4].magic, 1);
    assert_eq!(cname(proto[5].name), "add_item");
    assert_eq!(proto[5].magic, 2);
    assert_eq!(def.proto_entries_len, 6);

    let statics = fields(def.static_entries, def.static_entries_len);
    assert_eq!(def.static_entries_len, 2); // is_bag (func) + tag (getset)
    assert_eq!(cname(statics[0].name), "is_bag");
    assert_eq!(statics[0].kind, zuqe_zapi::ffi::ZQ_FIELD_FUNC);
    assert_eq!(cname(statics[1].name), "tag");
    assert_eq!(statics[1].kind, zuqe_zapi::ffi::ZQ_FIELD_GETSET);

    let marker = Marker::class_def();
    assert!(!marker.name.is_null());
    assert!(marker.gc_trace.is_none(), "no-payload class has no gc_trace");
    assert_eq!(marker.proto_entries_len, 0);
    assert_eq!(marker.static_entries_len, 0);

    let plain = Plain::class_def();
    assert!(plain.gc_trace.is_none(), "payload_no_gc opt-out drops the gc_trace");
}

/// Read a NUL-terminated C name from an ABI descriptor.
fn cname(ptr: *const std::ffi::c_char) -> &'static str {
    unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().unwrap()
}

/// Reconstruct a `ZqFieldDef` slice from its `(*const T, usize)` ABI form.
fn fields(ptr: *const zuqe_zapi::ffi::ZqFieldDef, len: usize) -> &'static [zuqe_zapi::ffi::ZqFieldDef] {
    if ptr.is_null() {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }
}

#[test]
fn exports_produce_defs() {
    // Each listed item's `::item()` yields a `ZqFieldDef` directly.
    let add_def = add::item();
    assert_eq!(add_def.name_str(), Some("add"));
    assert_eq!(add_def.kind, zuqe_zapi::ffi::ZQ_FIELD_FUNC);
    assert!(add_def.body.is_some());

    let bag = Bag::item();
    assert_eq!(bag.name_str(), Some("Bag"));
    assert_eq!(bag.kind, zuqe_zapi::ffi::ZQ_FIELD_CLASS);
    assert!(!bag.class_def.is_null());

    let marker = Marker::item();
    assert_eq!(marker.kind, zuqe_zapi::ffi::ZQ_FIELD_CLASS);

    let v = version::item();
    assert_eq!(v.name_str(), Some("VERSION"));
    assert_eq!(v.kind, zuqe_zapi::ffi::ZQ_FIELD_VALUE);
    assert!(v.value_fn.is_some());
}

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

mod buf;
mod descriptor;
mod property;

pub use {
    buf::{ZqBufKind, ZqTypedArrayElem},
    descriptor::ZqPropertyDescriptor,
};

use std::marker::PhantomData;

use crate::{
    ZqCtx, ZqError, ZqErrorType,
    ffi::{
        ZQ_CONST_VALUE_SLOT_EXCEPTION, ZQ_CONST_VALUE_SLOT_FALSE, ZQ_CONST_VALUE_SLOT_INFINITY,
        ZQ_CONST_VALUE_SLOT_MINUS_INFINITY, ZQ_CONST_VALUE_SLOT_NAN, ZQ_CONST_VALUE_SLOT_NULL,
        ZQ_CONST_VALUE_SLOT_TRUE, ZQ_CONST_VALUE_SLOT_UNDEFINED, ZQ_CONST_VALUE_SLOT_ZERO, ZqApi,
        ZqRawValue,
    },
};

/// A JS value type (`typeof` style). `#[repr(i32)]`, cbindgen translates to C enum.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZqValueType {
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

impl ZqValueType {
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

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZqValue<'cx> {
    raw: ZqRawValue,
    _marker: PhantomData<&'cx ()>,
}

impl<'cx> From<ZqRawValue> for ZqValue<'cx> {
    #[inline(always)]
    fn from(raw: ZqRawValue) -> Self {
        debug_assert_ne!(
            raw,
            Self::exception_value(),
            "you forgot to guard Exception value"
        );

        Self {
            raw,
            _marker: PhantomData,
        }
    }
}

impl<'cx> From<ZqValue<'cx>> for ZqRawValue {
    #[inline(always)]
    fn from(value: ZqValue<'cx>) -> Self {
        value.raw
    }
}

// Primitive values convert without a context: only the value is needed.
impl<'cx> From<i32> for ZqValue<'cx> {
    #[inline]
    fn from(v: i32) -> Self {
        ZqValue::number(v as f64)
    }
}

impl<'cx> From<u32> for ZqValue<'cx> {
    #[inline]
    fn from(v: u32) -> Self {
        ZqValue::number(v as f64)
    }
}

impl<'cx> From<f64> for ZqValue<'cx> {
    #[inline]
    fn from(v: f64) -> Self {
        ZqValue::number(v)
    }
}

impl<'cx> From<bool> for ZqValue<'cx> {
    #[inline]
    fn from(v: bool) -> Self {
        ZqValue::bool(v)
    }
}

impl<'cx> ZqValue<'cx> {
    // ── value constructors (need a context) ──

    /// Create a String value.
    pub fn new_string(ctx: ZqCtx<'cx>, s: &str) -> Result<Self, ZqError> {
        let api = ZqApi::singleton();
        let raw = unsafe { (api.new_string)(ctx.raw, s.as_ptr(), s.len()) };
        if raw == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(Self::from(raw))
        }
    }

    /// Create a plain object with `%Object.prototype%`.
    pub fn new_object(ctx: ZqCtx<'cx>) -> Result<Self, ZqError> {
        let api = ZqApi::singleton();
        let raw = unsafe { (api.new_object)(ctx.raw) };
        if raw == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(Self::from(raw))
        }
    }

    /// Create a plain object with prototype `proto` (null prototype when
    /// `proto` is `null`/`undefined`).
    pub fn new_object_with_proto(ctx: ZqCtx<'cx>, proto: ZqValue<'cx>) -> Result<Self, ZqError> {
        let api = ZqApi::singleton();
        let raw = unsafe { (api.new_object_with_proto)(ctx.raw, proto.raw()) };
        if raw == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(Self::from(raw))
        }
    }

    /// Create an Array from `elements`.
    pub fn new_array(ctx: ZqCtx<'cx>, elements: &[ZqValue<'cx>]) -> Result<Self, ZqError> {
        let api = ZqApi::singleton();
        // `ZqValue` is `repr(transparent)` over `ZqRawValue`, so the slice can
        // be reinterpreted without copying.
        let raw = unsafe {
            (api.new_array)(
                ctx.raw,
                elements.len(),
                elements.as_ptr().cast::<ZqRawValue>(),
            )
        };
        if raw == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(Self::from(raw))
        }
    }

    /// Create a Symbol with description `desc`.
    pub fn new_symbol(ctx: ZqCtx<'cx>, desc: &str) -> Result<Self, ZqError> {
        let api = ZqApi::singleton();
        let raw = unsafe { (api.new_symbol)(ctx.raw, desc.as_ptr(), desc.len()) };
        if raw == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(Self::from(raw))
        }
    }

    /// Create a BigInt from an `i64`.
    pub fn new_bigint_i64(ctx: ZqCtx<'cx>, v: i64) -> Result<Self, ZqError> {
        let api = ZqApi::singleton();
        let raw = unsafe { (api.new_bigint_i64)(ctx.raw, v) };
        if raw == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(Self::from(raw))
        }
    }

    /// Create a BigInt from a `u64`.
    pub fn new_bigint_u64(ctx: ZqCtx<'cx>, v: u64) -> Result<Self, ZqError> {
        let api = ZqApi::singleton();
        let raw = unsafe { (api.new_bigint_u64)(ctx.raw, v) };
        if raw == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(Self::from(raw))
        }
    }

    /// Create (but do not throw) an `Error` value of `err_type`.
    pub fn new_error(ctx: ZqCtx<'cx>, err_type: ZqErrorType, msg: &str) -> Result<Self, ZqError> {
        let api = ZqApi::singleton();
        let raw = unsafe { (api.new_error)(ctx.raw, err_type as i32, msg.as_ptr(), msg.len()) };
        if raw == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(Self::from(raw))
        }
    }
}

impl<'cx> ZqValue<'cx> {
    pub fn undefined() -> Self {
        let api = ZqApi::singleton();
        ZqValue::from(api.const_values[ZQ_CONST_VALUE_SLOT_UNDEFINED])
    }

    pub fn null() -> Self {
        let api = ZqApi::singleton();
        ZqValue::from(api.const_values[ZQ_CONST_VALUE_SLOT_NULL])
    }

    pub fn bool(b: bool) -> Self {
        let api = ZqApi::singleton();
        ZqValue::from(if b {
            api.const_values[ZQ_CONST_VALUE_SLOT_TRUE]
        } else {
            api.const_values[ZQ_CONST_VALUE_SLOT_FALSE]
        })
    }

    pub fn int32(i: i32) -> Self {
        let api = ZqApi::singleton();
        Self::from(if i == 0 {
            api.const_values[ZQ_CONST_VALUE_SLOT_ZERO]
        } else {
            unsafe { (api.new_number)(i as f64) }
        })
    }

    pub fn number(n: f64) -> Self {
        let api = ZqApi::singleton();
        let num = if n.is_nan() {
            api.const_values[ZQ_CONST_VALUE_SLOT_NAN]
        } else if n == 0.0 && n.is_sign_positive() {
            api.const_values[ZQ_CONST_VALUE_SLOT_ZERO]
        } else if n.is_infinite() {
            if n.is_sign_positive() {
                api.const_values[ZQ_CONST_VALUE_SLOT_INFINITY]
            } else {
                api.const_values[ZQ_CONST_VALUE_SLOT_MINUS_INFINITY]
            }
        } else if n.fract() == 0.0 && n >= i32::MIN as f64 && n <= i32::MAX as f64 {
            unsafe { (api.new_number)(n) }
        } else {
            unsafe { (api.new_number)(n) }
        };

        ZqValue::from(num)
    }

    #[inline(always)]
    pub fn raw(&self) -> ZqRawValue {
        self.raw
    }

    /// The engine's exception sentinel handle (`const_values[SLOT_EXCEPTION]`).
    /// Trampolines return it to signal "this call threw".
    #[inline(always)]
    pub fn exception_value() -> ZqRawValue {
        ZqApi::singleton().const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION]
    }

    /// The kind of JS value.
    pub fn kind(&self) -> ZqValueType {
        let api = ZqApi::singleton();
        unsafe {
            let t = (api.get_type_of)(self.raw);
            debug_assert!(
                (crate::ffi::ZQ_VALUE_TYPE_UNDEFINED..=crate::ffi::ZQ_VALUE_TYPE_EXCEPTION)
                    .contains(&t)
            );
            std::mem::transmute::<i32, ZqValueType>(t)
        }
    }

    /// true if the value is JS `undefined`.
    #[inline]
    pub fn is_undefined(&self) -> bool {
        self.raw == ZqApi::singleton().const_values[ZQ_CONST_VALUE_SLOT_UNDEFINED]
    }

    /// true if the value is JS `null`.
    #[inline]
    pub fn is_null(&self) -> bool {
        self.raw == ZqApi::singleton().const_values[ZQ_CONST_VALUE_SLOT_NULL]
    }

    /// true if the value is `undefined` or `null`.
    #[inline]
    pub fn is_nullish(&self) -> bool {
        let api = ZqApi::singleton();
        self.raw == api.const_values[ZQ_CONST_VALUE_SLOT_UNDEFINED]
            || self.raw == api.const_values[ZQ_CONST_VALUE_SLOT_NULL]
    }

    /// true if the value is the boolean `true` or `false`.
    #[inline]
    pub fn is_bool(&self) -> bool {
        let api = ZqApi::singleton();
        self.raw == api.const_values[ZQ_CONST_VALUE_SLOT_TRUE]
            || self.raw == api.const_values[ZQ_CONST_VALUE_SLOT_FALSE]
    }

    /// true if the value is the exception sentinel (`const_values[SLOT_EXCEPTION]`).
    #[inline]
    pub fn is_exception(&self) -> bool {
        self.raw == ZqApi::singleton().const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION]
    }

    /// true if the value is the canonical `NaN`.
    #[inline]
    pub fn is_nan(&self) -> bool {
        self.raw == ZqApi::singleton().const_values[ZQ_CONST_VALUE_SLOT_NAN]
    }

    /// Coerce to number (f64)
    pub fn to_f64(self, ctx: ZqCtx) -> Result<f64, ZqError> {
        let api = ZqApi::singleton();
        let mut ret = 0.0_f64;
        let r = unsafe { (api.to_number)(ctx.raw, self.raw, &mut ret) };
        if r == 0 { Ok(ret) } else { Err(ZqError) }
    }

    /// Coerce to number (i64).
    ///
    /// Numbers saturate. A BigInt is converted exactly; it errors (with a
    /// `RangeError`) when it does not fit in `i64`.
    pub fn to_i64(self, ctx: ZqCtx) -> Result<i64, ZqError> {
        let api = ZqApi::singleton();
        let mut ret = 0_i64;
        let r = unsafe { (api.to_int64)(ctx.raw, self.raw, &mut ret) };
        if r == 0 { Ok(ret) } else { Err(ZqError) }
    }

    /// Coerce to an ECMAScript `ToInt32` (wrapping modulo 2^32).
    ///
    /// For `ToUint32`, use `to_i32(...) as u32`.
    pub fn to_i32(self, ctx: ZqCtx) -> Result<i32, ZqError> {
        let api = ZqApi::singleton();
        let mut ret = 0_i32;
        let r = unsafe { (api.to_int32)(ctx.raw, self.raw, &mut ret) };
        if r == 0 { Ok(ret) } else { Err(ZqError) }
    }

    /// Strict equality (`===`). Never throws.
    pub fn strict_eq(&self, other: &ZqValue<'_>) -> bool {
        let api = ZqApi::singleton();
        unsafe { (api.is_strictly_equal)(self.raw, other.raw) != 0 }
    }

    /// Coerce to bool
    pub fn to_bool(self, ctx: ZqCtx<'cx>) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.to_boolean)(ctx.raw, self.raw) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Coerce to a JS string value.
    pub fn to_js_string(self, ctx: ZqCtx) -> Result<Self, ZqError> {
        let api = ZqApi::singleton();
        let sval = unsafe { (api.to_string)(ctx.raw, self.raw) };
        if sval == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(Self::from(sval))
        }
    }

    /// Read string content as UTF-8 into a Rust `String`.
    ///
    /// Uses the engine's measure-then-copy `read_string_utf8`; no buffer
    /// ownership crosses the ABI, so it is safe regardless of the host's
    /// global allocator (e.g. mimalloc in the `aike` binary).
    pub fn to_utf8_string(&self, ctx: ZqCtx<'cx>) -> Result<String, ZqError> {
        let api = ZqApi::singleton();
        // A null buf is measure mode: returns the byte count, or -1 if `self`
        // is not a string (and sets an exception).
        let len = unsafe { (api.read_string_utf8)(ctx.raw, self.raw, std::ptr::null_mut(), 0, 0) };
        if len < 0 {
            return Err(ZqError);
        }
        let len = len as usize;
        // Uninitialised capacity: the engine fills the whole buffer below, so
        // there is no need to zero it first as `vec![0u8; len]` would.
        let mut buf = Vec::<u8>::with_capacity(len);
        let n = if len == 0 {
            0
        } else {
            unsafe { (api.read_string_utf8)(ctx.raw, self.raw, buf.as_mut_ptr(), len, 0) }
        };
        if n < 0 {
            return Err(ZqError);
        }

        // SAFETY: `read_string_utf8` wrote exactly `n` bytes (n == len when
        // buf_len == len, which is guaranteed by the preceding measure call), so
        // exposing the first `n` bytes never reads uninitialised memory.
        unsafe { buf.set_len(n as usize) };

        // The engine already replaces invalid sequences with U+FFFD (allow_invalid
        // = false), so `buf` is valid UTF-8; move it into the String (no realloc)
        // and only fall back to a lossy copy in the defensive error case.
        Ok(String::from_utf8(buf)
            .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()))
    }

    /// Get Symbol's description and encode to utf8
    pub fn symbol_description(&self, ctx: ZqCtx) -> Result<String, ZqError> {
        // Symbol.prototype.description getter (ECMA-262 §20.4.3.2).
        self.get_str(ctx, "description")?.to_utf8_string(ctx)
    }
}

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::marker::PhantomData;

use crate::{
    ZqCtx, ZqError,
    ffi::{
        ZQ_CONST_VALUE_SLOT_EXCEPTION, ZQ_CONST_VALUE_SLOT_FALSE, ZQ_CONST_VALUE_SLOT_INFINITY,
        ZQ_CONST_VALUE_SLOT_MINUS_INFINITY, ZQ_CONST_VALUE_SLOT_NAN, ZQ_CONST_VALUE_SLOT_NULL,
        ZQ_CONST_VALUE_SLOT_TRUE, ZQ_CONST_VALUE_SLOT_UNDEFINED, ZQ_CONST_VALUE_SLOT_ZERO, ZqApi,
        ZqCallInfoS, ZqRawValue,
    },
    traits::IntoZqValue,
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
            unsafe { (api.new_int32)(i) }
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
            unsafe { (api.new_int32)(n as i32) }
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

    /// The type of JS value
    pub fn value_type(&self) -> ZqValueType {
        let api = ZqApi::singleton();
        unsafe {
            let t = (api.value_get_type)(self.raw);
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
    pub fn is_undefined_or_null(&self) -> bool {
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

    /// Coerce to number value
    pub fn to_number(self, ctx: ZqCtx) -> Result<f64, ZqError> {
        let api = ZqApi::singleton();
        let mut ret = 0.0_f64;
        let r = unsafe { (api.value_to_float64)(ctx.raw, self.raw, &mut ret) };
        if r == 0 { Ok(ret) } else { Err(ZqError) }
    }

    /// Coerce to number (f64)
    pub fn to_f64(self, ctx: ZqCtx) -> Result<f64, ZqError> {
        let api = ZqApi::singleton();
        let mut ret = 0.0_f64;
        let r = unsafe { (api.value_to_float64)(ctx.raw, self.raw, &mut ret) };
        if r == 0 { Ok(ret) } else { Err(ZqError) }
    }

    /// Coerce to number (i64)
    pub fn to_i64(self, ctx: ZqCtx) -> Result<i64, ZqError> {
        let api = ZqApi::singleton();
        let mut ret = 0_i64;
        let r = unsafe { (api.value_to_int64)(ctx.raw, self.raw, &mut ret) };
        if r == 0 { Ok(ret) } else { Err(ZqError) }
    }

    /// Coerce to number (i32), narrowing from the i64 conversion.
    pub fn to_i32(self, ctx: ZqCtx) -> Result<i32, ZqError> {
        self.to_i64(ctx).map(|n| n as i32)
    }

    /// Coerce to bool
    pub fn to_bool(self, ctx: ZqCtx<'cx>) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.value_to_bool)(ctx.raw, self.raw) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Coerce to string
    pub fn to_string(self, ctx: ZqCtx) -> Result<Self, ZqError> {
        let api = ZqApi::singleton();
        let sval = unsafe { (api.value_to_string)(ctx.raw, self.raw) };
        if sval == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(Self::from(sval))
        }
    }

    /// Read string content as UTF-8 into a Rust `String`.
    ///
    /// Uses the engine's measure-then-copy `string_read_utf8`; no buffer
    /// ownership crosses the ABI, so it is safe regardless of the host's
    /// global allocator (e.g. mimalloc in the `aike` binary).
    pub fn string_encode_utf8(&self, ctx: ZqCtx<'cx>) -> Result<String, ZqError> {
        let api = ZqApi::singleton();
        // A null buf is measure mode: returns the byte count, or -1 if `self`
        // is not a string (and sets an exception).
        let len = unsafe { (api.string_read_utf8)(ctx.raw, self.raw, std::ptr::null_mut(), 0, 0) };
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
            unsafe { (api.string_read_utf8)(ctx.raw, self.raw, buf.as_mut_ptr(), len, 0) }
        };
        if n < 0 {
            return Err(ZqError);
        }

        // SAFETY: `string_read_utf8` wrote exactly `n` bytes (n == len when
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
    pub fn symbol_description_utf8(&self, ctx: ZqCtx) -> Result<String, ZqError> {
        let api = ZqApi::singleton();
        let sval = unsafe { (api.symbol_get_description)(ctx.raw, self.raw) };
        if sval == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Self::from(sval).string_encode_utf8(ctx)
        }
    }

    /// For native object value with payload, returns payload ref
    pub fn payload<T: 'static>(&self, ctx: ZqCtx) -> Option<&T> {
        let p = ctx.get_native_ptr(*self);
        if p.is_null() {
            None
        } else {
            Some(unsafe { &*(p.cast::<T>()) })
        }
    }

    /// For native object value with payload, returns payload mut ref
    pub fn payload_mut<T: 'static>(&mut self, ctx: ZqCtx) -> Option<&mut T> {
        let p = ctx.get_native_ptr(*self);
        if p.is_null() {
            None
        } else {
            Some(unsafe { &mut *(p.cast::<T>()) })
        }
    }

    /// Get a property (works for objects, arrays, typed arrays, ...).
    pub fn get<T: IntoZqValue<'cx>>(
        &self,
        ctx: ZqCtx<'cx>,
        key: T,
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into_zq_value(ctx);
        let v = unsafe { (api.get_property)(ctx.raw, self.raw, key.raw()) };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Get a property by UTF-8 key without allocating an intermediate JS
    /// string (the host resolves the key to an interned atom).
    pub fn get_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let v = unsafe { (api.get_property_str)(ctx.raw, self.raw, key.as_ptr(), key.len()) };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Set a property.
    pub fn set<T: IntoZqValue<'cx>, V: IntoZqValue<'cx>>(
        &self,
        ctx: ZqCtx<'cx>,
        key: T,
        val: V,
    ) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into_zq_value(ctx);
        let val = val.into_zq_value(ctx);
        let r = unsafe { (api.set_property)(ctx.raw, self.raw, key.raw(), val.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Set a property by UTF-8 key without allocating an intermediate JS string.
    pub fn set_str<V: IntoZqValue<'cx>>(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
        val: V,
    ) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let val = val.into_zq_value(ctx);
        let r = unsafe {
            (api.set_property_str)(ctx.raw, self.raw, key.as_ptr(), key.len(), val.raw())
        };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Check whether a property exists.
    pub fn has<T: IntoZqValue<'cx>>(&self, cx: ZqCtx<'cx>, key: T) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into_zq_value(cx);
        let r = unsafe { (api.has_property)(cx.raw, self.raw, key.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Check whether a property exists (UTF-8 key, no intermediate JS string).
    pub fn has_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.has_property_str)(ctx.raw, self.raw, key.as_ptr(), key.len()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Delete a property.
    pub fn delete<T: IntoZqValue<'cx>>(&self, ctx: ZqCtx<'cx>, key: T) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into_zq_value(ctx);
        let r = unsafe { (api.delete_property)(ctx.raw, self.raw, key.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Delete a property by UTF-8 key without allocating an intermediate JS string.
    pub fn delete_str<T: IntoZqValue<'cx>>(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
    ) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.delete_property_str)(ctx.raw, self.raw, key.as_ptr(), key.len()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Get length property of value
    pub fn get_length_prop(&self, cx: ZqCtx<'cx>) -> Result<usize, ZqError> {
        let api = ZqApi::singleton();
        let len = unsafe { (api.get_length_prop)(cx.raw, self.raw) };
        if len < 0 {
            Err(ZqError)
        } else {
            Ok(len as usize)
        }
    }

    /// Call the value as a function: `val(this, args...)`.
    pub fn call(
        &self,
        ctx: ZqCtx<'cx>,
        this: ZqValue<'cx>,
        args: &[ZqValue<'cx>],
    ) -> Result<ZqValue<'cx>, ZqError> {
        ctx.call_function(*self, this, args)
    }

    /// Call the value as constructor
    pub fn call_constructor(
        &self,
        ctx: ZqCtx<'cx>,
        args: &[ZqValue<'cx>],
    ) -> Result<ZqValue<'cx>, ZqError> {
        ctx.call_constructor(*self, args)
    }

    /// Invoke `self[method](...args)` with `self` as `this`.
    pub fn invoke(
        &self,
        ctx: ZqCtx<'cx>,
        method: ZqValue<'cx>,
        args: &[ZqValue],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let mut argv: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let v = unsafe {
            (api.invoke)(
                ctx.raw,
                self.raw,
                method.raw(),
                argv.len(),
                argv.as_mut_ptr(),
            )
        };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Invoke `self[method](...args)` by UTF-8 name with `self` as `this`.
    pub fn invoke_str(
        &self,
        ctx: ZqCtx<'cx>,
        method: &str,
        args: &[ZqValue],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let mut argv: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let v = unsafe {
            (api.invoke_str)(
                ctx.raw,
                self.raw,
                method.as_ptr(),
                method.len(),
                argv.len(),
                argv.as_mut_ptr(),
            )
        };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// `Array.prototype.push`.
    pub fn array_push(&self, ctx: ZqCtx, val: ZqValue) -> Result<usize, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.array_push)(ctx.raw, self.raw, val.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r as usize) }
    }
}

/// Safe wrapper around FFI callback info. Provides typed access to `this` and arguments.
#[derive(Clone, Copy)]
pub struct ZqCallInfo<'cx> {
    pub(crate) raw: *mut ZqCallInfoS,
    _marker: PhantomData<&'cx ()>,
}

impl<'cx> ZqCallInfo<'cx> {
    pub fn new(raw: *mut ZqCallInfoS) -> Self {
        Self {
            raw,
            _marker: PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        unsafe { (*self.raw).argc }
    }

    /// The registration magic (see `#[zq_function(magic = N)]` /
    /// `#[zq_method(magic = N)]`), for dispatching one body to several JS names.
    pub fn magic(&self) -> i32 {
        unsafe { (*self.raw).magic }
    }

    pub fn this(&self) -> ZqValue<'cx> {
        unsafe { ZqValue::from((*self.raw).this) }
    }

    pub fn arg(&self, nth: usize) -> ZqValue<'cx> {
        if nth >= self.len() {
            ZqValue::undefined()
        } else {
            ZqValue::from(unsafe { *(*self.raw).argv.add(nth) })
        }
    }
}

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::marker::PhantomData;

use crate::{
    ZqCtx, ZqError,
    ffi::{ZqApi, ZqCallInfoS, ZqRawValue},
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
        ZqValue::from(api.value_undefined)
    }

    pub fn null() -> Self {
        let api = ZqApi::singleton();
        ZqValue::from(api.value_null)
    }

    pub fn bool(b: bool) -> Self {
        let api = ZqApi::singleton();
        ZqValue::from(if b { api.value_true } else { api.value_false })
    }

    pub fn number(n: f64) -> Self {
        let api = ZqApi::singleton();
        let num = if n.is_nan() {
            api.value_nan
        } else if n == 1.0 {
            api.value_one
        } else if n == -1.0 {
            api.value_negative_one
        } else if n == 0.0 && n.is_sign_positive() {
            api.value_zero
        } else if n.is_infinite() {
            if n.is_sign_positive() {
                api.value_infinity
            } else {
                api.value_minus_infinity
            }
        } else {
            unsafe { (api.new_number)(n) }
        };

        ZqValue::from(num)
    }

    #[inline(always)]
    pub fn raw(&self) -> ZqRawValue {
        self.raw
    }

    /// The engine's exception sentinel handle (`value_exception`).
    /// Trampolines return it to signal "this call threw".
    #[inline(always)]
    pub fn exception_value() -> ZqRawValue {
        ZqApi::singleton().value_exception
    }

    /// The type of JS value
    pub fn value_type(&self) -> ZqValueType {
        let api = ZqApi::singleton();
        unsafe {
            let t = (api.get_value_type)(self.raw);
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
        self.raw == ZqApi::singleton().value_undefined
    }

    /// true if the value is JS `null`.
    #[inline]
    pub fn is_null(&self) -> bool {
        self.raw == ZqApi::singleton().value_null
    }

    /// true if the value is `undefined` or `null`.
    #[inline]
    pub fn is_undefined_or_null(&self) -> bool {
        let api = ZqApi::singleton();
        self.raw == api.value_undefined || self.raw == api.value_null
    }

    /// true if the value is the boolean `true` or `false`.
    #[inline]
    pub fn is_bool(&self) -> bool {
        let api = ZqApi::singleton();
        self.raw == api.value_true || self.raw == api.value_false
    }

    /// true if the value is the exception sentinel (`value_exception`).
    #[inline]
    pub fn is_exception(&self) -> bool {
        self.raw == ZqApi::singleton().value_exception
    }

    /// true if the value is the canonical `NaN`.
    #[inline]
    pub fn is_nan(&self) -> bool {
        self.raw == ZqApi::singleton().value_nan
    }

    /// ToBoolean: whether the value is truthy (needs a live env).
    pub fn is_truthy(&self, ctx: ZqCtx<'cx>) -> bool {
        ctx.value_to_bool(*self)
    }

    /// Read the value as `f64` (ToNumber; NaN for non-numeric values).
    pub fn as_f64(&self, ctx: ZqCtx<'cx>) -> Result<f64, ZqError> {
        Ok(ctx.value_to_float64(*self))
    }

    /// Read the value as `i32` (truncating ToNumber).
    pub fn as_i32(&self, ctx: ZqCtx<'cx>) -> Result<i32, ZqError> {
        Ok(ctx.value_to_float64(*self) as i32)
    }

    /// Read the value as bool (ToBoolean).
    pub fn as_bool(&self, cx: ZqCtx<'cx>) -> Result<bool, ZqError> {
        Ok(cx.value_to_bool(*self))
    }

    /// Read the value as a Rust `String` (ToString).
    pub fn as_string(&self, ctx: ZqCtx) -> Result<String, ZqError> {
        let api = ZqApi::singleton();
        let sval = unsafe { (api.value_to_string)(ctx.raw, self.raw) };
        if sval == api.value_exception {
            Err(ZqError)
        } else {
            Ok(ctx.read_string(sval))
        }
    }

    /// Read a symbol's description ("" for a description-less symbol).
    pub fn symbol_description(&self, ctx: ZqCtx) -> Result<String, ZqError> {
        let api = ZqApi::singleton();
        let sval = unsafe { (api.get_symbol_description)(ctx.raw, self.raw) };
        if sval == api.value_exception {
            Err(ZqError)
        } else {
            Ok(ctx.read_string(sval))
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
        if v == api.value_exception {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Get a property (works for objects, arrays, typed arrays, ...).
    pub fn get_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Result<ZqValue<'cx>, ZqError> {
        let s = ctx.new_string(key)?;
        self.get(ctx, s)
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
        let r = unsafe { (api.set_property)(ctx.raw, self.raw, key.raw(), val.raw(), 0, 0) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Set a property.
    pub fn set_str<V: IntoZqValue<'cx>>(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
        val: V,
    ) -> Result<bool, ZqError> {
        let s = ctx.new_string(key)?;
        self.set(ctx, s, val)
    }

    /// Check whether a property exists.
    pub fn has<T: IntoZqValue<'cx>>(&self, cx: ZqCtx<'cx>, key: T) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into_zq_value(cx);
        let r = unsafe { (api.has_property)(cx.raw, self.raw, key.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Check whether a property exists.
    pub fn has_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Result<bool, ZqError> {
        let s = ctx.new_string(key)?;
        self.has(ctx, s)
    }

    /// Delete a property.
    pub fn delete<T: IntoZqValue<'cx>>(&self, ctx: ZqCtx<'cx>, key: T) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into_zq_value(ctx);
        let r = unsafe { (api.delete_property)(ctx.raw, self.raw, key.raw(), 0) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    pub fn delete_str<T: IntoZqValue<'cx>>(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
    ) -> Result<bool, ZqError> {
        let s = ctx.new_string(key)?;
        self.delete(ctx, s)
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

    /// `Array.prototype.push`.
    pub fn array_push(&self, ctx: ZqCtx, val: ZqValue) -> Result<usize, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.array_push)(ctx.raw, self.raw, val.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r as usize) }
    }
}

// ── ZqCallInfo ──

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
        unsafe { (*self.raw).argc as usize }
    }

    /// The registration magic (see `#[zq_function(magic = N)]` /
    /// `#[zq_method(magic = N)]`), for dispatching one body to several JS names.
    pub fn magic(&self) -> i32 {
        unsafe { (*self.raw).magic }
    }

    pub fn this(&self) -> ZqValue<'cx> {
        unsafe { ZqValue::from((*self.raw).this) }
    }

    pub fn arg(&self, i: usize) -> ZqValue<'cx> {
        if i >= self.len() {
            return ZqValue::from(0);
        }
        ZqValue::from(unsafe { *(*self.raw).args.add(i) })
    }
}

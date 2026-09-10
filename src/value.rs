// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::marker::PhantomData;

use crate::{
    ZqCtx, ZqError,
    ffi::{ZqCallInfoS, ZqRawValue},
    traits::{FromZqValue, IntoZqValue},
    zapi_singleton,
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
    #[inline(always)]
    pub fn raw(&self) -> ZqRawValue {
        self.raw
    }

    /// The type of JS value
    pub fn value_type(&self) -> ZqValueType {
        let api = zapi_singleton();
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
        self.raw == zapi_singleton().value_undefined
    }

    /// true if the value is JS `null`.
    #[inline]
    pub fn is_null(&self) -> bool {
        self.raw == zapi_singleton().value_null
    }

    /// true if the value is `undefined` or `null`.
    #[inline]
    pub fn is_undefined_or_null(&self) -> bool {
        let api = zapi_singleton();
        self.raw == api.value_undefined || self.raw == api.value_null
    }

    /// true if the value is the boolean `true` or `false`.
    #[inline]
    pub fn is_bool(&self) -> bool {
        let api = zapi_singleton();
        self.raw == api.value_true || self.raw == api.value_false
    }

    /// true if the value is the exception sentinel (`value_exception`).
    #[inline]
    pub fn is_exception(&self) -> bool {
        self.raw == zapi_singleton().value_exception
    }

    /// true if the value is the canonical `NaN`.
    #[inline]
    pub fn is_nan(&self) -> bool {
        self.raw == zapi_singleton().value_nan
    }

    /// ToBoolean: whether the value is truthy (needs a live env).
    pub fn is_truthy(&self, cx: ZqCtx<'cx>) -> bool {
        cx.value_to_bool(*self)
    }

    /// Read the value as `f64` (ToNumber; NaN for non-numeric values).
    pub fn as_f64(&self, cx: ZqCtx<'cx>) -> Result<f64, ZqError> {
        Ok(cx.value_to_float64(*self))
    }

    /// Read the value as `i32` (truncating ToNumber).
    pub fn as_i32(&self, cx: ZqCtx<'cx>) -> Result<i32, ZqError> {
        Ok(cx.value_to_float64(*self) as i32)
    }

    /// Read the value as bool (ToBoolean).
    pub fn as_bool(&self, cx: ZqCtx<'cx>) -> Result<bool, ZqError> {
        Ok(cx.value_to_bool(*self))
    }

    /// Read the value as a Rust `String` (ToString).
    pub fn as_string(&self, cx: ZqCtx<'cx>) -> Result<String, ZqError> {
        let api = zapi_singleton();
        let sval = unsafe { (api.value_to_string)(cx.raw, self.raw) };
        Ok(cx.read_string(sval))
    }

    /// Read a symbol's description ("" for a description-less symbol).
    pub fn symbol_description(&self, cx: ZqCtx<'cx>) -> Result<String, ZqError> {
        let api = zapi_singleton();
        let sval = unsafe { (api.get_symbol_description)(cx.raw, self.raw) };
        Ok(cx.read_string(sval))
    }

    /// Get a property (works for objects, arrays, typed arrays, ...).
    pub fn get<T: IntoZqValue<'cx>>(
        &self,
        cx: ZqCtx<'cx>,
        key: T,
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = zapi_singleton();
        let key = key.into_zq_value(cx);
        Ok(ZqValue::from(unsafe {
            (api.get_property)(cx.raw, self.raw, key.raw())
        }))
    }

    /// Set a property.
    pub fn set<T: IntoZqValue<'cx>, V: IntoZqValue<'cx>>(
        &self,
        cx: ZqCtx<'cx>,
        key: T,
        val: V,
    ) -> Result<(), ZqError> {
        let api = zapi_singleton();
        let key = key.into_zq_value(cx);
        let val = val.into_zq_value(cx);
        unsafe { (api.set_property)(cx.raw, self.raw, key.raw(), val.raw(), 0, 0) };
        Ok(())
    }

    /// Check whether a property exists.
    pub fn has<T: IntoZqValue<'cx>>(&self, cx: ZqCtx<'cx>, key: T) -> Result<bool, ZqError> {
        let api = zapi_singleton();
        let key = key.into_zq_value(cx);
        Ok(unsafe { (api.has_property)(cx.raw, self.raw, key.raw()) != 0 })
    }

    /// Delete a property.
    pub fn delete<T: IntoZqValue<'cx>>(&self, cx: ZqCtx<'cx>, key: T) -> Result<bool, ZqError> {
        let api = zapi_singleton();
        let key = key.into_zq_value(cx);
        Ok(unsafe { (api.delete_property)(cx.raw, self.raw, key.raw(), 0) != 0 })
    }

    /// Call the value as a function: `val(this, args...)`.
    pub fn call(
        &self,
        cx: ZqCtx<'cx>,
        this: ZqValue<'cx>,
        args: &[ZqValue<'cx>],
    ) -> Result<ZqValue<'cx>, ZqError> {
        cx.call_function(*self, this, args)
    }

    /// Array / typed-array length.
    pub fn array_len(&self, cx: ZqCtx<'cx>) -> Result<u32, ZqError> {
        let api = zapi_singleton();
        let len = unsafe { (api.array_len)(cx.raw, self.raw) };
        Ok(cx.value_to_float64(ZqValue::from(len)) as u32)
    }

    /// Array / typed-array element at `idx`.
    pub fn array_get(&self, cx: ZqCtx<'cx>, idx: usize) -> Result<ZqValue<'cx>, ZqError> {
        self.get(cx, cx.number(idx as f64))
    }

    /// Set array / typed-array element at `idx`.
    pub fn array_set<V: IntoZqValue<'cx>>(
        &self,
        cx: ZqCtx<'cx>,
        idx: usize,
        val: V,
    ) -> Result<(), ZqError> {
        self.set(cx, cx.number(idx as f64), val)
    }

    /// `Array.prototype.push`.
    pub fn array_push(&self, cx: ZqCtx<'cx>, val: ZqValue<'cx>) -> Result<(), ZqError> {
        let api = zapi_singleton();
        unsafe { (api.array_push)(cx.raw, self.raw, val.raw()) };
        Ok(())
    }

    pub fn payload_mut<T: 'static>(&mut self, cx: ZqCtx<'cx>) -> Option<&mut T> {
        let p = cx.get_native_ptr(*self);
        if p.is_null() {
            None
        } else {
            Some(unsafe { &mut *(p as *mut T) })
        }
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

    pub fn this(&self, _cx: ZqCtx<'cx>) -> ZqValue<'cx> {
        unsafe { ZqValue::from((*self.raw).this) }
    }

    pub fn arg(&self, _cx: ZqCtx<'cx>, i: usize) -> ZqValue<'cx> {
        if i >= self.len() {
            return ZqValue::from(0);
        }
        ZqValue::from(unsafe { *(*self.raw).args.add(i) })
    }

    pub fn arg_as<T: FromZqValue<'cx>>(&self, cx: ZqCtx<'cx>, i: usize) -> Result<T, ZqError> {
        let v = self.arg(cx, i);
        T::from_zq_value(cx, v)
    }
}

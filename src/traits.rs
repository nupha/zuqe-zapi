// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use crate::{ZqCtx, ZqError, ZqValue, ffi::ZqRawValue};

/// Unwrap a JS value into a Rust type.
///
/// Used by `#[zq_function]` parameter unpacking. On failure, call
/// `throw_type_error` and return `Err`.
pub trait FromZqValue<'cx>: Sized {
    fn from_zq_value(cx: ZqCtx<'cx>, val: ZqValue<'cx>) -> Result<Self, ZqError>;
}

/// Convert a Rust type into a JS value.
///
/// Used by `#[zq_function]` return value conversion.
pub trait IntoZqValue<'cx> {
    fn into_zq_value(self, cx: ZqCtx<'cx>) -> ZqValue<'cx>;
}

// ── FromZqValue impls ──

impl<'cx> FromZqValue<'cx> for ZqValue<'cx> {
    fn from_zq_value(_cx: ZqCtx<'cx>, val: ZqValue<'cx>) -> Result<Self, ZqError> {
        Ok(val)
    }
}

impl<'cx> FromZqValue<'cx> for ZqRawValue {
    fn from_zq_value(_cx: ZqCtx<'cx>, val: ZqValue<'cx>) -> Result<Self, ZqError> {
        Ok(val.raw())
    }
}

impl<'cx> FromZqValue<'cx> for i32 {
    fn from_zq_value(cx: ZqCtx<'cx>, val: ZqValue<'cx>) -> Result<Self, ZqError> {
        Ok(cx.value_to_float64(val) as i32)
    }
}

impl<'cx> FromZqValue<'cx> for i64 {
    fn from_zq_value(cx: ZqCtx<'cx>, val: ZqValue<'cx>) -> Result<Self, ZqError> {
        Ok(cx.value_to_float64(val) as i64)
    }
}

impl<'cx> FromZqValue<'cx> for f64 {
    fn from_zq_value(cx: ZqCtx<'cx>, val: ZqValue<'cx>) -> Result<Self, ZqError> {
        Ok(cx.value_to_float64(val))
    }
}

impl<'cx> FromZqValue<'cx> for bool {
    fn from_zq_value(cx: ZqCtx<'cx>, val: ZqValue<'cx>) -> Result<Self, ZqError> {
        Ok(cx.value_to_bool(val))
    }
}

impl<'cx> FromZqValue<'cx> for String {
    fn from_zq_value(cx: ZqCtx<'cx>, val: ZqValue<'cx>) -> Result<Self, ZqError> {
        val.as_string(cx)
    }
}

impl<'cx, T: FromZqValue<'cx>> FromZqValue<'cx> for Option<T> {
    fn from_zq_value(cx: ZqCtx<'cx>, val: ZqValue<'cx>) -> Result<Self, ZqError> {
        if val.is_undefined() || val.is_null() {
            Ok(None)
        } else {
            T::from_zq_value(cx, val).map(Some)
        }
    }
}

// ── IntoZqValue impls ──

impl<'cx> IntoZqValue<'cx> for ZqValue<'cx> {
    fn into_zq_value(self, _cx: ZqCtx<'cx>) -> ZqValue<'cx> {
        self
    }
}

impl<'cx> IntoZqValue<'cx> for ZqRawValue {
    fn into_zq_value(self, _: ZqCtx<'cx>) -> ZqValue<'cx> {
        ZqValue::from(self)
    }
}

impl<'cx> IntoZqValue<'cx> for i32 {
    fn into_zq_value(self, _: ZqCtx<'cx>) -> ZqValue<'cx> {
        ZqValue::int32(self)
    }
}

impl<'cx> IntoZqValue<'cx> for i64 {
    fn into_zq_value(self, _: ZqCtx<'cx>) -> ZqValue<'cx> {
        ZqValue::number(self as f64)
    }
}

impl<'cx> IntoZqValue<'cx> for f64 {
    fn into_zq_value(self, _: ZqCtx<'cx>) -> ZqValue<'cx> {
        ZqValue::number(self)
    }
}

impl<'cx> IntoZqValue<'cx> for bool {
    fn into_zq_value(self, _: ZqCtx<'cx>) -> ZqValue<'cx> {
        ZqValue::bool(self)
    }
}

impl<'cx, T: IntoZqValue<'cx>> IntoZqValue<'cx> for Option<T> {
    fn into_zq_value(self, cx: ZqCtx<'cx>) -> ZqValue<'cx> {
        match self {
            Some(v) => v.into_zq_value(cx),
            None => ZqValue::undefined(),
        }
    }
}

impl<'cx, T: IntoZqValue<'cx>, E: IntoZqValue<'cx>> IntoZqValue<'cx> for Result<T, E> {
    fn into_zq_value(self, cx: ZqCtx<'cx>) -> ZqValue<'cx> {
        match self {
            Ok(v) => v.into_zq_value(cx),
            Err(e) => e.into_zq_value(cx),
        }
    }
}

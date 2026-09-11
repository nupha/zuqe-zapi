// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use crate::{ZqCtx, ZqValue, ffi::ZqRawValue};

/// Convert a Rust type into a JS value.
///
/// Used by `#[zq_function]` return value conversion.
pub trait IntoZqValue<'cx> {
    fn into_zq_value(self, cx: ZqCtx<'cx>) -> ZqValue<'cx>;
}

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

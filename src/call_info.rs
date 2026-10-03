// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::marker::PhantomData;

use crate::{ZqCtx, ZqError, ZqValue, ffi::ZqCallInfoS};

/// Safe wrapper around FFI callback info. Provides typed access to `this` and arguments.
#[derive(Clone, Copy)]
pub struct ZqCallInfo<'cx> {
    pub(crate) raw: *mut ZqCallInfoS,
    _marker: PhantomData<&'cx ()>,
}

impl<'cx> ZqCallInfo<'cx> {
    #[inline(always)]
    pub fn new(raw: *mut ZqCallInfoS) -> Self {
        Self {
            raw,
            _marker: PhantomData,
        }
    }

    /// Number of arguments passed to this call.
    #[inline(always)]
    pub fn len(&self) -> usize {
        unsafe { (*self.raw).argc }
    }

    /// Whether the call received no arguments.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The registration magic (see `#[zq_function(magic = N)]` /
    /// `#[zq_method(magic = N)]`), for dispatching one body to several JS names.
    #[inline(always)]
    pub fn magic(&self) -> i32 {
        unsafe { (*self.raw).magic }
    }

    #[inline(always)]
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

    /// `arg(nth)` as an `f64` (`ToNumber`).
    pub fn arg_f64(&self, ctx: ZqCtx<'cx>, nth: usize) -> Result<f64, ZqError> {
        self.arg(nth).to_f64(ctx)
    }

    /// `arg(nth)` as an `i32` (`ToInt32`).
    pub fn arg_i32(&self, ctx: ZqCtx<'cx>, nth: usize) -> Result<i32, ZqError> {
        self.arg(nth).to_i32(ctx)
    }

    /// `arg(nth)` as a `bool` (`ToBoolean`).
    pub fn arg_bool(&self, ctx: ZqCtx<'cx>, nth: usize) -> Result<bool, ZqError> {
        self.arg(nth).to_bool(ctx)
    }

    /// `arg(nth)` as a Rust `String` (must already be a JS string).
    ///
    /// Strict: a non-string argument yields `Err` **and leaves the host's
    /// `TypeError` pending** (see [`ZqValue::string_read_utf8`]). For argument
    /// parsing that should coerce (`String(x)`) or tolerate a non-string, use
    /// [`ZqValue::coerce_string`] / [`ZqValue::try_string_read_utf8`] on `arg(nth)`
    /// instead.
    pub fn arg_string(&self, ctx: ZqCtx<'cx>, nth: usize) -> Result<String, ZqError> {
        self.arg(nth).string_read_utf8(ctx)
    }

    /// `arg(nth)` as a Rust `String`, constrained to `allowed`.
    ///
    /// `arg_name` is the parameter's public name, used in the error. A missing
    /// argument is `undefined` and therefore not a member: pass an
    /// `allowed` that contains `"undefined"` for an optional parameter.
    ///
    /// Forwards an unknown value as a `RangeError` rather than to the host,
    /// which for many commands is a hard protocol error (or worse).
    pub fn arg_enum(
        &self,
        ctx: ZqCtx<'cx>,
        nth: usize,
        allowed: &[&str],
        arg_name: &str,
    ) -> Result<String, ZqError> {
        let v = self.arg(nth).string_read_utf8(ctx)?;
        if allowed.contains(&v.as_str()) {
            Ok(v)
        } else {
            Err(ctx.throw_range(&format!(
                "`{arg_name}` must be one of {}; got {v:?}",
                allowed.join("|")
            )))
        }
    }

    /// Iterate over the arguments.
    pub fn args(&self) -> impl Iterator<Item = ZqValue<'cx>> + '_ {
        (0..self.len()).map(|i| self.arg(i))
    }
}

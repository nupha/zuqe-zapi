use std::marker::PhantomData;

use crate::{ZqCtx, ZqError, ZqValue, ffi::ZqCallInfoS};

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
    pub fn arg_string(&self, ctx: ZqCtx<'cx>, nth: usize) -> Result<String, ZqError> {
        self.arg(nth).to_utf8_string(ctx)
    }

    /// Iterate over the arguments.
    pub fn args(&self) -> impl Iterator<Item = ZqValue<'cx>> + '_ {
        (0..self.len()).map(|i| self.arg(i))
    }
}

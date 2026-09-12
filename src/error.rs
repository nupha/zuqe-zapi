// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use crate::{ZqCtx, ZqValue, ffi::ZqApi};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZqErrorType {
    Eval = 0,
    Range = 1,
    Reference = 2,
    Syntax = 3,
    Type = 4,
    Uri = 5,
    Internal = 6,
    Aggregate = 7,
}

impl ZqErrorType {
    /// Convert from the raw `i32` discriminant. Returns `None` for unknown values.
    pub fn from_raw(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::Eval),
            1 => Some(Self::Range),
            2 => Some(Self::Reference),
            3 => Some(Self::Syntax),
            4 => Some(Self::Type),
            5 => Some(Self::Uri),
            6 => Some(Self::Internal),
            7 => Some(Self::Aggregate),
            _ => None,
        }
    }
}

/// Marker type indicating an error occurred.
///
/// This is an opaque sentinel — it carries no error details itself.
/// Use [`crate::ZqCtx::take_exception`] or [`crate::ZqCtx::exception`]
/// to obtain the actual JS error object from the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZqError;

impl std::fmt::Display for ZqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error")
    }
}

impl std::error::Error for ZqError {}

impl<'cx> ZqCtx<'cx> {
    /// Throw a JS error of `err_type` with `err_msg`, and return the opaque
    /// error marker so the throw can be used as the error expression itself:
    ///
    /// ```ignore
    /// return Err(cx.throw(ZqErrorType::Type, "bad argument"));
    /// some_val.ok_or_else(|| cx.throw(ZqErrorType::Range, "out of range"))?;
    /// ```
    ///
    /// Built from the two primitives: `new_error` then `throw_value`.
    #[cold]
    #[must_use]
    pub fn throw(&self, err_type: ZqErrorType, err_msg: &str) -> ZqError {
        if let Ok(e) = ZqValue::error(*self, err_type, err_msg) {
            self.throw_value(e);
        }
        ZqError
    }

    /// Raise `val` as the pending exception (an arbitrary value, not only an
    /// Error). Pair with [`ZqCtx::new_error`] to throw a typed error.
    pub fn throw_value(&self, val: ZqValue<'cx>) {
        let api = ZqApi::singleton();
        unsafe { (api.throw_value)(self.raw, val.raw()) };
    }

    /// Throw an `EvalError`. Sugar for [`ZqCtx::throw`].
    #[cold]
    #[must_use]
    pub fn throw_eval(&self, msg: &str) -> ZqError {
        self.throw(ZqErrorType::Eval, msg)
    }

    /// Throw a `RangeError`. Sugar for [`ZqCtx::throw`].
    #[cold]
    #[must_use]
    pub fn throw_range(&self, msg: &str) -> ZqError {
        self.throw(ZqErrorType::Range, msg)
    }

    /// Throw a `ReferenceError`. Sugar for [`ZqCtx::throw`].
    #[cold]
    #[must_use]
    pub fn throw_reference(&self, msg: &str) -> ZqError {
        self.throw(ZqErrorType::Reference, msg)
    }

    /// Throw a `SyntaxError`. Sugar for [`ZqCtx::throw`].
    #[cold]
    #[must_use]
    pub fn throw_syntax(&self, msg: &str) -> ZqError {
        self.throw(ZqErrorType::Syntax, msg)
    }

    /// Throw a `TypeError`. Sugar for [`ZqCtx::throw`].
    #[cold]
    #[must_use]
    pub fn throw_type(&self, msg: &str) -> ZqError {
        self.throw(ZqErrorType::Type, msg)
    }

    /// Throw a `URIError`. Sugar for [`ZqCtx::throw`].
    #[cold]
    #[must_use]
    pub fn throw_uri(&self, msg: &str) -> ZqError {
        self.throw(ZqErrorType::Uri, msg)
    }

    /// Throw an `InternalError`. Sugar for [`ZqCtx::throw`].
    #[cold]
    #[must_use]
    pub fn throw_internal(&self, msg: &str) -> ZqError {
        self.throw(ZqErrorType::Internal, msg)
    }

    /// Throw an `AggregateError`. Sugar for [`ZqCtx::throw`].
    #[cold]
    #[must_use]
    pub fn throw_aggregate(&self, msg: &str) -> ZqError {
        self.throw(ZqErrorType::Aggregate, msg)
    }

    /// Peek the pending JS exception without consuming it.
    /// Returns the exception value, or `undefined` if none is pending.
    pub fn exception(&self) -> ZqValue<'cx> {
        let api = ZqApi::singleton();
        ZqValue::from(unsafe { (api.exception)(self.raw) })
    }

    /// Take the pending JS exception, clearing `cur_exception` and returning
    /// the exception value (GC-rooted until scope ends).
    /// Returns `undefined` if no exception was pending.
    pub fn take_exception(&self) -> ZqValue<'cx> {
        let api = ZqApi::singleton();
        ZqValue::from(unsafe { (api.take_exception)(self.raw) })
    }
}

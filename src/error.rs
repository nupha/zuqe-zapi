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
    /// Create a JS error object from a `ZqError` and return its raw handle.
    /// Used by macro trampolines to throw proper JS errors when `ZqError`
    /// carries a message but no pre-built exception handle.
    #[cold]
    pub fn throw(&self, err_type: ZqErrorType, err_msg: &str) -> ZqError {
        let api = ZqApi::singleton();
        unsafe {
            (api.throw)(
                self.raw,
                err_type as i32,
                err_msg.as_ptr(),
                err_msg.len() as i32,
            );
        }
        ZqError
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

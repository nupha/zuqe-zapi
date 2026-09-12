// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

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

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

pub mod channel;
pub mod class;
pub mod ctx;
pub mod error;
pub mod ffi;
pub mod gc;
pub mod iter;
pub mod traits;
pub mod value;

// Internal module for proc-macro generated code.
#[doc(hidden)]
pub mod __private;

pub use {
    __private::module_path,
    channel::ZqWakeSender,
    class::Class,
    ctx::{ZqCtx, ZqWakeHandle},
    error::{ZqError, ZqStatus},
    ffi::{ZqEnv, ZqFieldDef, ZqRawValue, ZqType},
    gc::{ZqGcTrace, ZqGcTraceCtx},
    iter::ZqAsyncIterable,
    traits::{FromZqValue, IntoZqValue},
    value::{ZqCallInfo, ZqValue},
};

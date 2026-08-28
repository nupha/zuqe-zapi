// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

pub mod class;
pub mod ctx;
pub mod error;
pub mod ffi;
pub mod gc;
pub mod traits;
pub mod value;

// Internal module for proc-macro generated code.
#[doc(hidden)]
pub mod __private;

pub mod prelude;

pub use class::Class;
pub use ctx::ZqCtx;
pub use error::{ZqError, ZqStatus};
pub use ffi::{ZqEnv, ZqFieldDef, ZqRawValue, ZqType};
pub use gc::{ZqGcTrace, ZqGcTraceCtx};
/// Canonical filesystem path of the dylib this extension was loaded from
/// (`None` when the host predates the feature). Set before
/// `zq_module_register` runs; readable anywhere in the extension.
pub use __private::module_path;
pub use traits::{FromZqValue, IntoZqValue};
pub use value::{ZqCallInfo, ZqValue};

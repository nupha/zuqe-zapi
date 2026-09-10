// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::ffi::c_void;

use crate::{
    ZqError,
    ffi::{ZqApi, ZqGcTraceCtxS, ZqRawValue},
};

#[derive(Debug, Clone, Copy)]
#[repr(transparent)]
pub struct ZqGcTraceCtx {
    raw: *mut ZqGcTraceCtxS,
}

impl From<*mut ZqGcTraceCtxS> for ZqGcTraceCtx {
    #[inline(always)]
    fn from(raw: *mut ZqGcTraceCtxS) -> Self {
        Self { raw }
    }
}

impl ZqGcTraceCtx {
    pub fn raw(&self) -> *mut ZqGcTraceCtxS {
        self.raw
    }

    /// Reports `val` to the GC so its graph stays reachable during tracing.
    pub fn mark(&self, val: ZqRawValue) -> Result<(), ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.gc_mark)(self.raw, val) };
        if r < 0 { Err(ZqError) } else { Ok(()) }
    }
}

pub trait ZqGcTrace {
    fn trace(&self, ctx: &mut ZqGcTraceCtx);
}

impl ZqGcTrace for () {
    fn trace(&self, _: &mut ZqGcTraceCtx) {}
}
impl ZqGcTrace for i32 {
    fn trace(&self, _: &mut ZqGcTraceCtx) {}
}
impl ZqGcTrace for i64 {
    fn trace(&self, _: &mut ZqGcTraceCtx) {}
}
impl ZqGcTrace for f64 {
    fn trace(&self, _: &mut ZqGcTraceCtx) {}
}
impl ZqGcTrace for bool {
    fn trace(&self, _: &mut ZqGcTraceCtx) {}
}
impl ZqGcTrace for String {
    fn trace(&self, _: &mut ZqGcTraceCtx) {}
}

impl ZqGcTrace for ZqRawValue {
    #[inline(always)]
    fn trace(&self, gcx: &mut ZqGcTraceCtx) {
        let _ = gcx.mark(*self);
    }
}

impl<T: ZqGcTrace> ZqGcTrace for Option<T> {
    fn trace(&self, ctx: &mut ZqGcTraceCtx) {
        if let Some(v) = self {
            v.trace(ctx);
        }
    }
}

impl<T: ZqGcTrace> ZqGcTrace for Box<T> {
    fn trace(&self, ctx: &mut ZqGcTraceCtx) {
        self.as_ref().trace(ctx);
    }
}

impl<T: ZqGcTrace> ZqGcTrace for Vec<T> {
    fn trace(&self, ctx: &mut ZqGcTraceCtx) {
        for item in self {
            item.trace(ctx);
        }
    }
}

impl ZqGcTrace for *mut c_void {
    fn trace(&self, _ctx: &mut ZqGcTraceCtx) {}
}

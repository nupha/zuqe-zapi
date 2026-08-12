use std::ffi::c_void;

use crate::{
    __private::global_api,
    ffi::{ZqGcTraceCtxS, ZqRawValue},
    ZqError, ZqStatus,
};

#[derive(Debug, Clone, Copy)]
pub struct ZqGcTraceCtx {
    raw: *mut ZqGcTraceCtxS,
}

impl ZqGcTraceCtx {
    /// Wraps the `trace_ctx` pointer handed to a class `gc_trace` callback.
    ///
    /// # Safety
    /// `raw` must be the trace context received by a `ZqClassDef.gc_trace` callback.
    pub unsafe fn from_raw(raw: *mut ZqGcTraceCtxS) -> Self {
        Self { raw }
    }

    pub fn raw(&self) -> *mut ZqGcTraceCtxS {
        self.raw
    }

    /// Reports `val` to the GC so its graph stays reachable during tracing.
    pub fn mark(&self, val: ZqRawValue) -> Result<(), ZqError> {
        let api = global_api().ok_or(ZqError::new(ZqStatus::Error, Some("no api".into())))?;
        let r = unsafe { (api.gc_add_trace)(std::ptr::null_mut(), self.raw, val) };
        if r < 0 {
            Err(ZqError::new(
                ZqStatus::Error,
                Some("gc_add_trace failed".into()),
            ))
        } else {
            Ok(())
        }
    }
}

pub trait ZqGcTrace {
    fn trace(&self, ctx: &mut ZqGcTraceCtx);
}

impl ZqGcTrace for () {
    fn trace(&self, _ctx: &mut ZqGcTraceCtx) {}
}
impl ZqGcTrace for i32 {
    fn trace(&self, _ctx: &mut ZqGcTraceCtx) {}
}
impl ZqGcTrace for i64 {
    fn trace(&self, _ctx: &mut ZqGcTraceCtx) {}
}
impl ZqGcTrace for f64 {
    fn trace(&self, _ctx: &mut ZqGcTraceCtx) {}
}
impl ZqGcTrace for bool {
    fn trace(&self, _ctx: &mut ZqGcTraceCtx) {}
}
impl ZqGcTrace for String {
    fn trace(&self, _ctx: &mut ZqGcTraceCtx) {}
}

impl ZqGcTrace for ZqRawValue {
    fn trace(&self, ctx: &mut ZqGcTraceCtx) {
        // 0 (undefined) holds no reachable graph.
        if *self != 0 {
            let _ = ctx.mark(*self);
        }
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

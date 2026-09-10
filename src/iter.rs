use std::ffi::c_void;

use crate::{ZqCtx, ZqError, ZqRawCtx, ZqRawValue, ZqValue, ffi::ZqApi};

/// Result of fetching the next value from an async iterator.
pub enum AsyncIterResult<'a> {
    /// A value is ready to yield.
    Value(ZqValue<'a>),
    /// No more data
    Done,
    /// Data not ready, retry next tick.
    Wait,
}

pub trait ZqAsyncIterable {
    fn next<'a>(&mut self, ctx: ZqCtx<'a>) -> Result<AsyncIterResult<'a>, ZqError>;
}

impl<'cx> ZqCtx<'cx> {
    pub fn new_async_iterator<T: ZqAsyncIterable>(&self, iter: T) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let boxed = Box::new(iter);
        let opaque = Box::into_raw(boxed) as *mut c_void;

        let raw = unsafe {
            (api.new_async_iterator)(
                self.raw,
                async_iter_next_trampoline::<T>,
                None,
                Some(drop_boxed::<T>),
                opaque,
            )
        };
        if raw == api.value_exception {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(raw))
        }
    }
}

extern "C" fn async_iter_next_trampoline<T: ZqAsyncIterable>(
    raw_ctx: ZqRawCtx,
    opaque: *mut c_void,
    ret_val: *mut ZqRawValue,
) -> i32 {
    let ctx = ZqCtx::from(raw_ctx);
    let iter = unsafe { &mut *(opaque as *mut T) };

    match iter.next(ctx) {
        Ok(AsyncIterResult::Value(val)) => {
            unsafe { *ret_val = val.raw() };
            0
        }
        Ok(AsyncIterResult::Done) => 1,
        Ok(AsyncIterResult::Wait) => 2,
        Err(_) => {
            // assert exception is set to current_exception
            -1
        }
    }
}

extern "C" fn drop_boxed<T>(ptr: *mut c_void) {
    if !ptr.is_null() {
        unsafe { drop(Box::from_raw(ptr as *mut T)) };
    }
}

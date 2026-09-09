use std::ffi::c_void;

use crate::{__private::zapi_singleton, ZqCtx, ZqEnv, ZqError, ZqRawValue, ZqValue};

pub trait ZqAsyncIterable {
    fn next<'a>(&'a mut self, ctx: ZqCtx<'a>) -> Result<Option<ZqValue<'a>>, ZqError>;
}

impl<'cx> ZqCtx<'cx> {
    pub fn new_async_iterator<T: ZqAsyncIterable>(&self, iter: T) -> Result<ZqValue<'cx>, ZqError> {
        extern "C" fn trampoline<T: ZqAsyncIterable>(
            env: ZqEnv,
            opaque: *mut c_void,
        ) -> ZqRawValue {
            let ctx = ZqCtx::from_raw(env);
            let iter = unsafe { &mut *(opaque as *mut T) };

            match iter.next(ctx) {
                Ok(Some(val)) => val.raw(),
                Ok(None) => ctx.undefined().raw(),
                Err(e) => {
                    let _ = ctx.throw_error(&e);
                    zapi_singleton().value_exception
                }
            }
        }

        extern "C" fn drop_fn<T>(ptr: *mut c_void) {
            if !ptr.is_null() {
                unsafe { drop(Box::from_raw(ptr as *mut T)) };
            }
        }

        let boxed = Box::new(iter);
        let opaque = Box::into_raw(boxed) as *mut c_void;
        let api = zapi_singleton();
        let raw = unsafe {
            (api.new_async_iterator)(self.env, trampoline::<T>, None, Some(drop_fn::<T>), opaque)
        };
        if raw == api.value_exception {
            Err(ZqError::type_error("new_async_iterator failed"))
        } else {
            Ok(ZqValue::from(raw))
        }
    }
}

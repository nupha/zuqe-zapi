use std::marker::PhantomData;

use crate::ffi::ZqClassDef;

pub struct Class<'cx, T: 'cx> {
    def: *const ZqClassDef,
    _marker: PhantomData<&'cx T>,
}

impl<'cx, T> Class<'cx, T> {
    pub unsafe fn from_def(def: *const ZqClassDef) -> Self {
        Self {
            def,
            _marker: PhantomData,
        }
    }

    pub fn def(&self) -> *const ZqClassDef {
        self.def
    }
}

impl<'cx, T> Clone for Class<'cx, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'cx, T> Copy for Class<'cx, T> {}

impl<'cx, T> std::fmt::Debug for Class<'cx, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Class").field("def", &self.def).finish()
    }
}

// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use std::marker::PhantomData;

use crate::ffi::ZqClassDef;

pub struct Class<'cx, T: 'cx> {
    def: *const ZqClassDef,
    _marker: PhantomData<&'cx T>,
}

impl<'cx, T> Class<'cx, T> {
    /// Wrap a raw `ZqClassDef` pointer.
    ///
    /// # Safety
    ///
    /// `def` must point to a `ZqClassDef` that stays valid for the lifetime of
    /// the returned `Class` (in practice a `'static` class definition registered
    /// by `#[zq_class]`). The pointer is never dereferenced here; callers pass it
    /// back to the host, which assumes it refers to the class `T` denotes.
    #[inline(always)]
    pub unsafe fn from_def(def: *const ZqClassDef) -> Self {
        Self {
            def,
            _marker: PhantomData,
        }
    }

    #[inline(always)]
    pub fn def(&self) -> *const ZqClassDef {
        self.def
    }
}

impl<'cx, T> Clone for Class<'cx, T> {
    #[inline(always)]
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

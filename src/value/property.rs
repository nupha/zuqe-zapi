// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use crate::{
    ZqCtx, ZqError,
    ffi::{ZQ_CONST_VALUE_SLOT_EXCEPTION, ZqApi, ZqPropertyDescriptorS, ZqRawValue},
};

use super::{ZqPropertyDescriptor, ZqValue, ZqValueType};

/// Turn a strict property read into an "absent or unusable" answer, dropping the
/// exception the read left behind.
///
/// Every `get_*` accessor reports a throwing getter (or a wrong-typed value) as
/// `Err` **with the JS exception still pending**. A caller that treats that as
/// "not provided" must clear it, or the host's end-of-tick assertion fires.
fn clear_on_err<T>(cx: ZqCtx<'_>, r: Result<Option<T>, ZqError>) -> Option<T> {
    match r {
        Ok(v) => v,
        Err(_) => {
            let _ = cx.take_exception();
            None
        }
    }
}

impl<'cx> ZqValue<'cx> {
    /// Get a property (works for objects, arrays, typed arrays, ...).
    pub fn get(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into();
        let v = unsafe { (api.get_property)(ctx.raw, self.raw, key.raw()) };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Get a property by UTF-8 key without allocating an intermediate JS
    /// string (the host resolves the key to an interned atom).
    pub fn get_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let v = unsafe { (api.get_property_str)(ctx.raw, self.raw, key.as_ptr(), key.len()) };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Set a property.
    pub fn set(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
        val: impl Into<ZqValue<'cx>>,
    ) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into();
        let val = val.into();
        let r = unsafe { (api.set_property)(ctx.raw, self.raw, key.raw(), val.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Set a property by UTF-8 key without allocating an intermediate JS string.
    pub fn set_str(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
        val: impl Into<ZqValue<'cx>>,
    ) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let val = val.into();
        let r = unsafe {
            (api.set_property_str)(ctx.raw, self.raw, key.as_ptr(), key.len(), val.raw())
        };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Check whether a property exists.
    pub fn has(&self, ctx: ZqCtx<'cx>, key: impl Into<ZqValue<'cx>>) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into();
        let r = unsafe { (api.has_property)(ctx.raw, self.raw, key.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Check whether a property exists (UTF-8 key, no intermediate JS string).
    pub fn has_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.has_property_str)(ctx.raw, self.raw, key.as_ptr(), key.len()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Delete a property.
    pub fn delete(&self, ctx: ZqCtx<'cx>, key: impl Into<ZqValue<'cx>>) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into();
        let r = unsafe { (api.delete_property)(ctx.raw, self.raw, key.raw()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// Delete a property by UTF-8 key without allocating an intermediate JS string.
    pub fn delete_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.delete_property_str)(ctx.raw, self.raw, key.as_ptr(), key.len()) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    // ── property definition (descriptor) ──

    /// Define `key` as a data property with attributes `flags` (`ZQ_PROP_*`).
    pub fn define(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
        val: impl Into<ZqValue<'cx>>,
        flags: u32,
    ) -> Result<(), ZqError> {
        let api = ZqApi::singleton();
        let key = key.into();
        let val = val.into();
        let r = unsafe { (api.define_property)(ctx.raw, self.raw, key.raw(), val.raw(), flags) };
        if r < 0 { Err(ZqError) } else { Ok(()) }
    }

    /// Define a UTF-8-keyed data property with attributes `flags`.
    pub fn define_str(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
        val: impl Into<ZqValue<'cx>>,
        flags: u32,
    ) -> Result<(), ZqError> {
        let api = ZqApi::singleton();
        let val = val.into();
        let r = unsafe {
            (api.define_property_str)(ctx.raw, self.raw, key.as_ptr(), key.len(), val.raw(), flags)
        };
        if r < 0 { Err(ZqError) } else { Ok(()) }
    }

    /// Define `key` as an accessor property. Pass `ZqValue::undefined()` for an
    /// absent getter/setter.
    pub fn define_accessor(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
        get: impl Into<ZqValue<'cx>>,
        set: impl Into<ZqValue<'cx>>,
        flags: u32,
    ) -> Result<(), ZqError> {
        let api = ZqApi::singleton();
        let key = key.into();
        let get = get.into();
        let set = set.into();
        let r = unsafe {
            (api.define_accessor)(ctx.raw, self.raw, key.raw(), get.raw(), set.raw(), flags)
        };
        if r < 0 { Err(ZqError) } else { Ok(()) }
    }

    /// Define a UTF-8-keyed accessor property.
    pub fn define_accessor_str(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
        get: impl Into<ZqValue<'cx>>,
        set: impl Into<ZqValue<'cx>>,
        flags: u32,
    ) -> Result<(), ZqError> {
        let api = ZqApi::singleton();
        let get = get.into();
        let set = set.into();
        let r = unsafe {
            (api.define_accessor_str)(
                ctx.raw,
                self.raw,
                key.as_ptr(),
                key.len(),
                get.raw(),
                set.raw(),
                flags,
            )
        };
        if r < 0 { Err(ZqError) } else { Ok(()) }
    }

    // ── type predicates ──

    /// `IsArray(self)` (honours Proxy).
    pub fn is_array(&self, ctx: ZqCtx<'cx>) -> Result<bool, ZqError> {
        let api = ZqApi::singleton();
        let r = unsafe { (api.is_array)(ctx.raw, self.raw) };
        if r < 0 { Err(ZqError) } else { Ok(r != 0) }
    }

    /// `IsCallable(self)`.
    pub fn is_callable(&self) -> bool {
        let api = ZqApi::singleton();
        unsafe { (api.is_callable)(self.raw) != 0 }
    }

    /// Whether `self` has a `[[Construct]]` internal method.
    pub fn is_constructor(&self) -> bool {
        let api = ZqApi::singleton();
        unsafe { (api.is_constructor)(self.raw) != 0 }
    }

    // ── own property introspection ──

    /// Read the own property descriptor of `self[key]`.
    pub fn own_property_descriptor(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
    ) -> Result<Option<ZqPropertyDescriptor<'cx>>, ZqError> {
        let api = ZqApi::singleton();
        let key = key.into();
        let mut d = ZqPropertyDescriptorS {
            kind: 0,
            flags: 0,
            value: 0,
            get: 0,
            set: 0,
        };
        let r = unsafe { (api.get_own_property_descriptor)(ctx.raw, self.raw, key.raw(), &mut d) };
        if r < 0 {
            Err(ZqError)
        } else if r == 0 {
            Ok(None)
        } else {
            Ok(Some(ZqPropertyDescriptor::from_raw(&d)))
        }
    }

    /// Read the own property descriptor of `self[key]` by UTF-8 key.
    pub fn own_property_descriptor_str(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
    ) -> Result<Option<ZqPropertyDescriptor<'cx>>, ZqError> {
        let api = ZqApi::singleton();
        let mut d = ZqPropertyDescriptorS {
            kind: 0,
            flags: 0,
            value: 0,
            get: 0,
            set: 0,
        };
        let r = unsafe {
            (api.get_own_property_descriptor_str)(
                ctx.raw,
                self.raw,
                key.as_ptr(),
                key.len(),
                &mut d,
            )
        };
        if r < 0 {
            Err(ZqError)
        } else if r == 0 {
            Ok(None)
        } else {
            Ok(Some(ZqPropertyDescriptor::from_raw(&d)))
        }
    }

    /// All own property keys as a fresh JS Array. `include_symbols` appends
    /// symbol keys.
    pub fn own_property_names(
        &self,
        ctx: ZqCtx<'cx>,
        include_symbols: bool,
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let v = unsafe { (api.get_own_property_names)(ctx.raw, self.raw, include_symbols as i32) };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Get the array-like `length` (`LengthOfArrayLike`).
    pub fn length(&self, ctx: ZqCtx<'cx>) -> Result<usize, ZqError> {
        let api = ZqApi::singleton();
        let len = unsafe { (api.get_length)(ctx.raw, self.raw) };
        if len < 0 {
            Err(ZqError)
        } else {
            Ok(len as usize)
        }
    }

    /// Call the value as a function: `val(this, args...)`.
    pub fn call(
        &self,
        ctx: ZqCtx<'cx>,
        this: impl Into<ZqValue<'cx>>,
        args: &[ZqValue<'cx>],
    ) -> Result<ZqValue<'cx>, ZqError> {
        ctx.call_function(*self, this, args)
    }

    /// Call the value as constructor
    pub fn call_constructor(
        &self,
        ctx: ZqCtx<'cx>,
        args: &[ZqValue<'cx>],
    ) -> Result<ZqValue<'cx>, ZqError> {
        ctx.call_constructor(*self, args)
    }

    /// Invoke `self[method](...args)` with `self` as `this`.
    pub fn invoke(
        &self,
        ctx: ZqCtx<'cx>,
        method: impl Into<ZqValue<'cx>>,
        args: &[ZqValue],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let method = method.into();
        let mut argv: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let v = unsafe {
            (api.invoke)(
                ctx.raw,
                self.raw,
                method.raw(),
                argv.len(),
                argv.as_mut_ptr(),
            )
        };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Invoke `self[method](...args)` by UTF-8 name with `self` as `this`.
    pub fn invoke_str(
        &self,
        ctx: ZqCtx<'cx>,
        method: &str,
        args: &[ZqValue],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let mut argv: Vec<ZqRawValue> = args.iter().map(|v| v.raw()).collect();
        let v = unsafe {
            (api.invoke_str)(
                ctx.raw,
                self.raw,
                method.as_ptr(),
                method.len(),
                argv.len(),
                argv.as_mut_ptr(),
            )
        };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// `Array.prototype.push` — implemented as `arr.push(val)`.
    pub fn push(&self, ctx: ZqCtx<'cx>, val: impl Into<ZqValue<'cx>>) -> Result<usize, ZqError> {
        let r = self.invoke_str(ctx, "push", &[val.into()])?;
        r.to_i64(ctx).map(|n| n as usize)
    }

    // ── convenience sugar ──

    /// Get `self[idx]` by integer index.
    pub fn get_index(&self, ctx: ZqCtx<'cx>, idx: u32) -> Result<ZqValue<'cx>, ZqError> {
        self.get(ctx, idx)
    }

    /// Set `self[idx] = val` by integer index.
    pub fn set_index(
        &self,
        ctx: ZqCtx<'cx>,
        idx: u32,
        val: impl Into<ZqValue<'cx>>,
    ) -> Result<bool, ZqError> {
        self.set(ctx, idx, val)
    }

    /// Get `self[key]` as a Rust `String`; `None` when absent or nullish.
    pub fn get_string(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
    ) -> Result<Option<String>, ZqError> {
        let v = self.get(ctx, key)?;
        if v.is_nullish() {
            Ok(None)
        } else {
            v.string_read_utf8(ctx).map(Some)
        }
    }

    /// Like [`get_string`](Self::get_string) with a UTF-8 key.
    pub fn get_string_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Result<Option<String>, ZqError> {
        let v = self.get_str(ctx, key)?;
        if v.is_nullish() {
            Ok(None)
        } else {
            v.string_read_utf8(ctx).map(Some)
        }
    }

    /// Get `self[key]` as an `f64`; `None` when absent or nullish.
    pub fn get_f64(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
    ) -> Result<Option<f64>, ZqError> {
        let v = self.get(ctx, key)?;
        if v.is_nullish() {
            Ok(None)
        } else {
            v.to_f64(ctx).map(Some)
        }
    }

    /// Like [`get_f64`](Self::get_f64) with a UTF-8 key.
    pub fn get_f64_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Result<Option<f64>, ZqError> {
        let v = self.get_str(ctx, key)?;
        if v.is_nullish() {
            Ok(None)
        } else {
            v.to_f64(ctx).map(Some)
        }
    }

    /// Get `self[key]` as a `bool`; `None` when absent or nullish.
    pub fn get_bool(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
    ) -> Result<Option<bool>, ZqError> {
        let v = self.get(ctx, key)?;
        if v.is_nullish() {
            Ok(None)
        } else {
            v.to_bool(ctx).map(Some)
        }
    }

    /// Like [`get_bool`](Self::get_bool) with a UTF-8 key.
    pub fn get_bool_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Result<Option<bool>, ZqError> {
        let v = self.get_str(ctx, key)?;
        if v.is_nullish() {
            Ok(None)
        } else {
            v.to_bool(ctx).map(Some)
        }
    }

    /// Get `self[key]` as an object; `None` when absent/nullish, error when the
    /// property is not an object.
    pub fn get_object(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
    ) -> Result<Option<ZqValue<'cx>>, ZqError> {
        let v = self.get(ctx, key)?;
        if v.is_nullish() {
            Ok(None)
        } else if v.kind() == ZqValueType::Object {
            Ok(Some(v))
        } else {
            Err(ctx.throw_type("property is not an object"))
        }
    }

    /// Like [`get_object`](Self::get_object) with a UTF-8 key (no intermediate
    /// JS string).
    pub fn get_object_str(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
    ) -> Result<Option<ZqValue<'cx>>, ZqError> {
        let v = self.get_str(ctx, key)?;
        if v.is_nullish() {
            Ok(None)
        } else if v.kind() == ZqValueType::Object {
            Ok(Some(v))
        } else {
            Err(ctx.throw_type("property is not an object"))
        }
    }

    /// Get `self[key]` as an Array; `None` when absent/nullish, error when the
    /// property is not an array.
    pub fn get_array(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
    ) -> Result<Option<ZqValue<'cx>>, ZqError> {
        let v = self.get(ctx, key)?;
        if v.is_nullish() {
            Ok(None)
        } else if v.is_array(ctx)? {
            Ok(Some(v))
        } else {
            Err(ctx.throw_type("property is not an array"))
        }
    }

    /// Like [`get_array`](Self::get_array) with a UTF-8 key.
    pub fn get_array_str(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
    ) -> Result<Option<ZqValue<'cx>>, ZqError> {
        let v = self.get_str(ctx, key)?;
        if v.is_nullish() {
            Ok(None)
        } else if v.is_array(ctx)? {
            Ok(Some(v))
        } else {
            Err(ctx.throw_type("property is not an array"))
        }
    }

    // ── exception-clearing variants for optional properties ──
    //
    // Reading an *optional* option must never leave an exception behind: a
    // throwing getter, or a value of the wrong type, would otherwise survive
    // the call and trip the host's "call returned Ok but left an exception set"
    // assertion (debug builds abort). Each `try_get_*` swallows the failure
    // *and clears it*, so a caller that means "absent or unusable" can write
    // `opts.try_get_object_str(cx, "clip")` and move on.

    /// [`get_string`](Self::get_string) that clears the exception on failure.
    pub fn try_get_string(&self, ctx: ZqCtx<'cx>, key: impl Into<ZqValue<'cx>>) -> Option<String> {
        clear_on_err(ctx, self.get_string(ctx, key))
    }

    /// [`get_string_str`](Self::get_string_str) that clears the exception on failure.
    pub fn try_get_string_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Option<String> {
        clear_on_err(ctx, self.get_string_str(ctx, key))
    }

    /// [`get_f64`](Self::get_f64) that clears the exception on failure.
    pub fn try_get_f64(&self, ctx: ZqCtx<'cx>, key: impl Into<ZqValue<'cx>>) -> Option<f64> {
        clear_on_err(ctx, self.get_f64(ctx, key))
    }

    /// [`get_f64_str`](Self::get_f64_str) that clears the exception on failure.
    pub fn try_get_f64_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Option<f64> {
        clear_on_err(ctx, self.get_f64_str(ctx, key))
    }

    /// [`get_bool`](Self::get_bool) that clears the exception on failure.
    pub fn try_get_bool(&self, ctx: ZqCtx<'cx>, key: impl Into<ZqValue<'cx>>) -> Option<bool> {
        clear_on_err(ctx, self.get_bool(ctx, key))
    }

    /// [`get_bool_str`](Self::get_bool_str) that clears the exception on failure.
    pub fn try_get_bool_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Option<bool> {
        clear_on_err(ctx, self.get_bool_str(ctx, key))
    }

    /// [`get_object`](Self::get_object) that clears the exception on failure.
    pub fn try_get_object(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
    ) -> Option<ZqValue<'cx>> {
        clear_on_err(ctx, self.get_object(ctx, key))
    }

    /// [`get_object_str`](Self::get_object_str) that clears the exception on failure.
    pub fn try_get_object_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Option<ZqValue<'cx>> {
        clear_on_err(ctx, self.get_object_str(ctx, key))
    }

    /// [`get_array`](Self::get_array) that clears the exception on failure.
    pub fn try_get_array(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
    ) -> Option<ZqValue<'cx>> {
        clear_on_err(ctx, self.get_array(ctx, key))
    }

    /// [`get_array_str`](Self::get_array_str) that clears the exception on failure.
    pub fn try_get_array_str(&self, ctx: ZqCtx<'cx>, key: &str) -> Option<ZqValue<'cx>> {
        clear_on_err(ctx, self.get_array_str(ctx, key))
    }

    // ── constrained reads ──

    /// Get an optional string property constrained to `allowed`, throwing a
    /// `RangeError` naming the property, the rejected value and the whole set
    /// when it is present but not a member.
    ///
    /// `None` for an absent or nullish property; use for protocol enums, where
    /// forwarding an unknown value is a protocol error at best and, for some
    /// commands, fatal.
    pub fn get_enum(
        &self,
        ctx: ZqCtx<'cx>,
        key: &str,
        allowed: &[&str],
    ) -> Result<Option<String>, ZqError> {
        let Some(v) = self.get_string_str(ctx, key)? else {
            return Ok(None);
        };
        if allowed.contains(&v.as_str()) {
            Ok(Some(v))
        } else {
            Err(ctx.throw_range(&format!(
                "`{key}` must be one of {}; got {v:?}",
                allowed.join("|")
            )))
        }
    }

    /// `Some(self)` when this value is an object (not a callable), else `None`.
    /// Cheaper than `get_object(ctx, key)` for an already-obtained value.
    pub fn as_object(&self) -> Option<ZqValue<'cx>> {
        if self.kind() == ZqValueType::Object {
            Some(*self)
        } else {
            None
        }
    }

    /// `Some(self)` when this value is an Array, else `None`.
    pub fn as_array(&self, ctx: ZqCtx<'cx>) -> Result<Option<ZqValue<'cx>>, ZqError> {
        if self.is_array(ctx)? {
            Ok(Some(*self))
        } else {
            Ok(None)
        }
    }

    /// `Some(self)` when this value is a callable, else `None`.
    pub fn as_function(&self) -> Option<ZqValue<'cx>> {
        if self.is_callable() {
            Some(*self)
        } else {
            None
        }
    }

    /// Set `key` to `val`, returning `self` for fluent chaining.
    pub fn with(
        &self,
        ctx: ZqCtx<'cx>,
        key: impl Into<ZqValue<'cx>>,
        val: impl Into<ZqValue<'cx>>,
    ) -> Result<&Self, ZqError> {
        self.set(ctx, key, val)?;
        Ok(self)
    }

    /// Create an Array from any iterator of values.
    pub fn array_from_iter<I, T>(ctx: ZqCtx<'cx>, elements: I) -> Result<Self, ZqError>
    where
        I: IntoIterator<Item = T>,
        T: Into<ZqValue<'cx>>,
    {
        let vals: Vec<ZqValue<'cx>> = elements.into_iter().map(Into::into).collect();
        Self::new_array(ctx, &vals)
    }
}

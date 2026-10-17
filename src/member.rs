// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

//! Members added to a class prototype at runtime.
//!
//! **This is the fallback, not the way to write it.** A member whose name is
//! known when the extension compiles belongs in `#[zq_method]`, or in a
//! `#[zq_class(prototype_members = ..)]` const table when the names come from
//! data. Reach for this module only when the names genuinely do not exist until
//! the host runs — a reflection table over a foreign API, a config file.
//!
//! Why the declaration forms come first: they are installed while the class is
//! being set up, so a name that collides with something already there refuses
//! the *whole class*. Installing at runtime can only refuse the call, which
//! means members listed before the offending one are already on the prototype.
//!
//! **Every name must be new.** The host compares the table against what the
//! class already has and throws a `TypeError` naming the member; it never
//! overwrites. Picking the right names stays the caller's job, but a mistake is
//! an error rather than a silent replacement.

use crate::ffi::{
    ZQ_MEMBER_BOOL, ZQ_MEMBER_FLOAT, ZQ_MEMBER_GETSET, ZQ_MEMBER_INT, ZQ_MEMBER_METHOD,
    ZQ_MEMBER_STR, ZQ_MEMBER_UNDEFINED, ZqBodyFn, ZqMemberDef,
};

/// Default property flags for an installed member: writable and configurable.
///
/// Configurable is what lets a later install replace this one. Non-enumerable,
/// which is what a method or a constant should be.
const DEFAULT_FLAGS: u32 = crate::ffi::ZQ_PROP_WRITABLE | crate::ffi::ZQ_PROP_CONFIGURABLE;

/// The value of a constant member.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZqMemberValue<'a> {
    /// A method or accessor carries no value.
    None,
    Str(&'a str),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// One member to install on a class prototype.
#[derive(Clone, Copy)]
pub struct ZqMember<'a> {
    /// The property name. Must be new on the class it is installed on: the
    /// host throws a `TypeError` naming it rather than replacing anything.
    pub name: &'a str,
    /// Function arity for a method. Advisory; JS does not enforce it.
    pub arity: u8,
    /// Dispatch value, yielded by `cc.magic()` in the body.
    pub magic: i32,
    /// `ZQ_PROP_*`. Written and overwritten by [`ZqMember::flags`].
    pub flags: u32,
    /// Method body.
    pub body: Option<ZqBodyFn>,
    /// Accessor getter.
    pub get: Option<ZqBodyFn>,
    /// Accessor setter.
    pub set: Option<ZqBodyFn>,
    /// Constant value.
    pub value: ZqMemberValue<'a>,
}

impl<'a> ZqMember<'a> {
    /// A method. `magic` is what the body reads back as `cc.magic()`.
    ///
    /// When every name shares one body, numbering by hand is the one place two
    /// things can drift. Write the table yourself so `magic` is the index:
    ///
    /// ```ignore
    /// let members: Vec<ZqMember> = names
    ///     .iter()
    ///     .enumerate()
    ///     .map(|(i, n)| ZqMember::method(n, body, i as i32))
    ///     .collect();
    /// ```
    pub fn method(name: &'a str, body: ZqBodyFn, magic: i32) -> Self {
        Self {
            name,
            arity: 0,
            magic,
            flags: DEFAULT_FLAGS,
            body: Some(body),
            get: None,
            set: None,
            value: ZqMemberValue::None,
        }
    }

    /// An accessor. Either half may be absent; both absent installs nothing
    /// useful, so pass at least one.
    pub fn accessor(name: &'a str, get: Option<ZqBodyFn>, set: Option<ZqBodyFn>) -> Self {
        Self {
            name,
            arity: 0,
            magic: 0,
            flags: DEFAULT_FLAGS,
            body: None,
            get,
            set,
            value: ZqMemberValue::None,
        }
    }

    /// A constant string.
    pub fn const_str(name: &'a str, value: &'a str) -> Self {
        Self::constant(name, ZqMemberValue::Str(value))
    }

    /// A constant integer.
    pub fn const_int(name: &'a str, value: i64) -> Self {
        Self::constant(name, ZqMemberValue::Int(value))
    }

    /// A constant float.
    pub fn const_float(name: &'a str, value: f64) -> Self {
        Self::constant(name, ZqMemberValue::Float(value))
    }

    /// A constant boolean.
    pub fn const_bool(name: &'a str, value: bool) -> Self {
        Self::constant(name, ZqMemberValue::Bool(value))
    }

    /// A member whose value is `undefined`.
    pub fn constant(name: &'a str, value: ZqMemberValue<'a>) -> Self {
        Self {
            name,
            arity: 0,
            magic: 0,
            flags: DEFAULT_FLAGS,
            body: None,
            get: None,
            set: None,
            value,
        }
    }

    /// Overwrite the property flags.
    pub fn flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }

    /// Overwrite the declared arity.
    pub fn arity(mut self, arity: u8) -> Self {
        self.arity = arity;
        self
    }
}

/// The wire form. `name` and `str_val` point into buffers owned by the caller
/// and must outlive the install call.
pub(crate) fn to_def(
    m: &ZqMember<'_>,
    name: *const core::ffi::c_char,
    str_val: *const core::ffi::c_char,
) -> ZqMemberDef {
    // A member is a method when it carries a body, an accessor when it carries
    // getter/setter halves, and a constant when it carries a value.
    let kind = match m.value {
        ZqMemberValue::None if m.body.is_some() => ZQ_MEMBER_METHOD,
        ZqMemberValue::None => ZQ_MEMBER_GETSET,
        ZqMemberValue::Str(_) => ZQ_MEMBER_STR,
        ZqMemberValue::Int(_) => ZQ_MEMBER_INT,
        ZqMemberValue::Float(_) => ZQ_MEMBER_FLOAT,
        ZqMemberValue::Bool(_) => ZQ_MEMBER_BOOL,
    };
    ZqMemberDef {
        name,
        kind,
        arity: m.arity,
        magic: m.magic,
        flags: m.flags,
        body: m.body,
        get: m.get,
        set: m.set,
        str_val,
        i64_val: match m.value {
            ZqMemberValue::Int(v) => v,
            _ => 0,
        },
        f64_val: match m.value {
            ZqMemberValue::Float(v) => v,
            _ => 0.0,
        },
        u8_val: match m.value {
            ZqMemberValue::Bool(v) => v as u8,
            _ => 0,
        },
    }
}

// ── the const-table form ──────────────────────────────────────────────────
//
// `#[zq_class(prototype_members = ..)]` splices a `&'static [ZqMemberDef]`
// into the class's entry array at compile time, so the members need const
// constructors and a const conversion. Names come from `c"..."` because the
// wire form is a NUL-terminated C string, and a `&str` cannot grow one in a
// const context. The runtime API above takes a plain `&str` instead.

impl ZqMemberDef {
    /// A method. `magic` is what the body reads back as `cc.magic()`.
    pub const fn method(
        name: &'static std::ffi::CStr,
        body: ZqBodyFn,
        arity: u8,
        magic: i32,
    ) -> Self {
        Self::new(name, ZQ_MEMBER_METHOD).with_body(body, arity, magic)
    }

    /// An accessor. Either half may be absent.
    pub const fn accessor(
        name: &'static std::ffi::CStr,
        get: Option<ZqBodyFn>,
        set: Option<ZqBodyFn>,
        magic: i32,
    ) -> Self {
        Self {
            get,
            set,
            ..Self::new(name, ZQ_MEMBER_GETSET).with_magic(magic)
        }
    }

    /// A constant string.
    pub const fn const_str(name: &'static std::ffi::CStr, value: &'static std::ffi::CStr) -> Self {
        Self {
            str_val: value.as_ptr(),
            ..Self::new(name, ZQ_MEMBER_STR)
        }
    }

    /// A constant integer.
    pub const fn const_int(name: &'static std::ffi::CStr, value: i64) -> Self {
        Self {
            i64_val: value,
            ..Self::new(name, ZQ_MEMBER_INT)
        }
    }

    /// A constant float.
    pub const fn const_float(name: &'static std::ffi::CStr, value: f64) -> Self {
        Self {
            f64_val: value,
            ..Self::new(name, ZQ_MEMBER_FLOAT)
        }
    }

    /// A constant boolean.
    pub const fn const_bool(name: &'static std::ffi::CStr, value: bool) -> Self {
        Self {
            u8_val: value as u8,
            ..Self::new(name, ZQ_MEMBER_BOOL)
        }
    }

    /// A member whose value is `undefined`.
    pub const fn undefined(name: &'static std::ffi::CStr) -> Self {
        Self::new(name, ZQ_MEMBER_UNDEFINED)
    }

    const fn new(name: &'static std::ffi::CStr, kind: u8) -> Self {
        Self {
            name: name.as_ptr(),
            kind,
            arity: 0,
            magic: 0,
            flags: DEFAULT_FLAGS,
            body: None,
            get: None,
            set: None,
            str_val: std::ptr::null(),
            i64_val: 0,
            f64_val: 0.0,
            u8_val: 0,
        }
    }

    const fn with_body(mut self, body: ZqBodyFn, arity: u8, magic: i32) -> Self {
        self.body = Some(body);
        self.arity = arity;
        self.magic = magic;
        self
    }

    const fn with_magic(mut self, magic: i32) -> Self {
        self.magic = magic;
        self
    }

    /// The `ZqFieldDef` the host installs this member from.
    pub const fn to_field(&self) -> crate::ffi::ZqFieldDef {
        let kind = match self.kind {
            ZQ_MEMBER_METHOD => crate::ffi::ZQ_FIELD_FUNC,
            ZQ_MEMBER_GETSET => crate::ffi::ZQ_FIELD_GETSET,
            ZQ_MEMBER_STR => crate::ffi::ZQ_FIELD_STR,
            ZQ_MEMBER_INT => crate::ffi::ZQ_FIELD_INT,
            ZQ_MEMBER_FLOAT => crate::ffi::ZQ_FIELD_FLOAT,
            ZQ_MEMBER_BOOL => crate::ffi::ZQ_FIELD_BOOL,
            ZQ_MEMBER_UNDEFINED => crate::ffi::ZQ_FIELD_UNDEFINED,
            other => other,
        };
        crate::ffi::ZqFieldDef {
            name: self.name,
            kind,
            arity: self.arity,
            magic: self.magic,
            prop_flags: self.flags,
            body: self.body,
            get: self.get,
            set: self.set,
            class_def: std::ptr::null(),
            value_fn: None,
            str_val: self.str_val,
            i64_val: self.i64_val,
            f64_val: self.f64_val,
            u8_val: self.u8_val,
        }
    }
}

/// Convert a `ZqMemberDef` table into the `ZqFieldDef` array the host installs.
/// `N` must be `members.len()`.
pub const fn member_fields<const N: usize>(members: &[ZqMemberDef]) -> [crate::ffi::ZqFieldDef; N] {
    let mut out = [crate::ffi::ZqFieldDef::EMPTY; N];
    let mut i = 0;
    while i < members.len() {
        out[i] = members[i].to_field();
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn noop(
        _env: crate::ffi::ZqRawCtx,
        _info: *mut crate::ffi::ZqCallInfoS,
    ) -> crate::ffi::ZqRawValue {
        0
    }

    #[test]
    fn a_method_is_callable_by_default() {
        let m = ZqMember::method("close", noop, 7);
        assert!(m.body.is_some());
        assert_eq!(m.magic, 7);
        assert_eq!(m.flags, DEFAULT_FLAGS);
        // Configurable: a later install of the same name must be able to
        // replace this one.
        assert_ne!(m.flags & crate::ffi::ZQ_PROP_CONFIGURABLE, 0);
    }

    /// The wire kind is derived from what the member carries, not stored
    /// separately: one accessor kind covers get-only, set-only and both, and
    /// the host tells the halves apart by which pointer is non-null.
    #[test]
    fn the_wire_kind_follows_what_the_member_carries() {
        let one_sided = to_def(
            &ZqMember::accessor("x", Some(noop), None),
            c"x".as_ptr(),
            std::ptr::null(),
        );
        assert_eq!(one_sided.kind, ZQ_MEMBER_GETSET);
        assert!(one_sided.get.is_some() && one_sided.set.is_none());

        let with_method = to_def(
            &ZqMember::method("y", noop, 3),
            c"y".as_ptr(),
            std::ptr::null(),
        );
        assert_eq!(with_method.kind, ZQ_MEMBER_METHOD);
        assert!(with_method.body.is_some());
    }

    #[test]
    fn constants_carry_no_body() {
        for m in [
            ZqMember::const_str("a", "v"),
            ZqMember::const_int("a", 1),
            ZqMember::const_float("a", 1.0),
            ZqMember::const_bool("a", true),
        ] {
            assert!(m.body.is_none());
            assert!(!matches!(m.value, ZqMemberValue::None));
        }
    }
}

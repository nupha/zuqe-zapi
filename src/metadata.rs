// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

//! What a compiled extension publishes, in a form a tool can read.
//!
//! `ZqFieldDef` is the ABI: raw pointers, no documentation, no arity beyond one
//! byte, and no way to ask "what names does this module define?". That is all a
//! runtime needs, which is why nothing here is on the FFI.
//!
//! The proc macros emit one `MEMBERS` slice per `#[zq_function]` /
//! `#[zq_class]` so a `zapi-dts` generator — or a build step that reconciles a
//! hand-written `.d.ts` against reality — can list the real surface instead of
//! trusting a declaration nobody checks.

/// What kind of JS-visible member a [`ZqMemberMeta`] entry describes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZqMemberKind {
    /// A `#[zq_function]` module export.
    Function,
    /// A `#[zq_class]` export.
    Class,
    /// A `#[zq_export]` module value, built once at module evaluation.
    Value,
    /// A `#[zq_method]` instance member.
    Method,
    /// A `#[zq_static_method]` constructor member.
    StaticMethod,
    /// A getter half of a `get` accessor.
    Getter,
    /// A setter half of a `set` accessor.
    Setter,
    /// Both halves of a `get, set` accessor.
    GetSet,
    /// A static getter.
    StaticGetter,
    /// A static setter.
    StaticSetter,
    /// Both halves of a static accessor.
    StaticGetSet,
}

/// One JS-visible member of an extension.
#[derive(Clone, Copy, Debug)]
pub struct ZqMemberMeta {
    /// The name as JS spells it.
    pub name: &'static str,
    pub kind: ZqMemberKind,
    /// Declared arity; `0` when the macro did not pin one. Advisory only — JS
    /// does not enforce it.
    pub arity: u8,
    /// The doc comment attached to the Rust item, lines joined with `\n`.
    /// `None` when the item has none.
    pub doc: Option<&'static str>,
}

impl ZqMemberMeta {
    /// Whether this member is callable from JS as a function.
    pub fn is_function(&self) -> bool {
        matches!(
            self.kind,
            ZqMemberKind::Function | ZqMemberKind::Method | ZqMemberKind::StaticMethod
        )
    }

    /// Whether this member is an accessor rather than a plain member.
    pub fn is_accessor(&self) -> bool {
        matches!(
            self.kind,
            ZqMemberKind::Getter
                | ZqMemberKind::Setter
                | ZqMemberKind::GetSet
                | ZqMemberKind::StaticGetter
                | ZqMemberKind::StaticSetter
                | ZqMemberKind::StaticGetSet
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: ZqMemberMeta = ZqMemberMeta {
        name: "close",
        kind: ZqMemberKind::Method,
        arity: 0,
        doc: None,
    };

    #[test]
    fn a_method_is_callable_and_not_an_accessor() {
        assert!(M.is_function());
        assert!(!M.is_accessor());
    }

    #[test]
    fn accessors_are_not_callable() {
        let get = ZqMemberMeta {
            kind: ZqMemberKind::Getter,
            ..M
        };
        assert!(!get.is_function());
        assert!(get.is_accessor());
    }

    #[test]
    fn a_class_neither_is_callable_nor_an_accessor() {
        let class = ZqMemberMeta {
            kind: ZqMemberKind::Class,
            ..M
        };
        assert!(!class.is_function());
        assert!(!class.is_accessor());
    }

    #[test]
    fn a_value_neither_is_callable_nor_an_accessor() {
        let value = ZqMemberMeta {
            kind: ZqMemberKind::Value,
            ..M
        };
        assert!(!value.is_function());
        assert!(!value.is_accessor());
    }
}

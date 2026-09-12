use super::ZqValue;
use crate::ffi::{ZQ_PROP_KIND_ACCESSOR, ZqPropertyDescriptorS};

/// A property descriptor read back by [`ZqValue::own_property_descriptor`].
#[derive(Debug, Clone, Copy)]
pub struct ZqPropertyDescriptor<'cx> {
    /// `ZQ_PROP_*` attributes.
    pub flags: u32,
    /// Data property value (`undefined` for an accessor).
    pub value: ZqValue<'cx>,
    /// Accessor getter (`undefined` for a data property).
    pub get: ZqValue<'cx>,
    /// Accessor setter (`undefined` for a data property).
    pub set: ZqValue<'cx>,
    /// True for an accessor property, false for a data property.
    pub is_accessor: bool,
}

impl<'cx> ZqPropertyDescriptor<'cx> {
    pub(super) fn from_raw(d: &ZqPropertyDescriptorS) -> Self {
        Self {
            flags: d.flags,
            value: ZqValue::from(d.value),
            get: ZqValue::from(d.get),
            set: ZqValue::from(d.set),
            is_accessor: d.kind == ZQ_PROP_KIND_ACCESSOR,
        }
    }
}

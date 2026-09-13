// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: Copyright (c) 2026 John Ray <996351336@qq.com>

use crate::{
    ZqCtx, ZqError,
    ffi::{
        ZQ_BUF_ARRAY_BUFFER, ZQ_BUF_DATAVIEW, ZQ_BUF_SHARED_ARRAY_BUFFER,
        ZQ_BUF_TYPED_ARRAY_BIGINT64, ZQ_BUF_TYPED_ARRAY_BIGUINT64, ZQ_BUF_TYPED_ARRAY_FLOAT16,
        ZQ_BUF_TYPED_ARRAY_FLOAT32, ZQ_BUF_TYPED_ARRAY_FLOAT64, ZQ_BUF_TYPED_ARRAY_INT8,
        ZQ_BUF_TYPED_ARRAY_INT16, ZQ_BUF_TYPED_ARRAY_INT32, ZQ_BUF_TYPED_ARRAY_UINT8,
        ZQ_BUF_TYPED_ARRAY_UINT8C, ZQ_BUF_TYPED_ARRAY_UINT16, ZQ_BUF_TYPED_ARRAY_UINT32,
        ZQ_CONST_VALUE_SLOT_EXCEPTION, ZqApi,
    },
};

use super::ZqValue;

/// Kind of a JavaScript binary buffer (`ZQ_BUF_*`): an ArrayBuffer,
/// SharedArrayBuffer, DataView, or typed array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ZqBufKind {
    ArrayBuffer = ZQ_BUF_ARRAY_BUFFER,
    SharedArrayBuffer = ZQ_BUF_SHARED_ARRAY_BUFFER,
    DataView = ZQ_BUF_DATAVIEW,
    TypedArrayInt8 = ZQ_BUF_TYPED_ARRAY_INT8,
    TypedArrayUint8 = ZQ_BUF_TYPED_ARRAY_UINT8,
    TypedArrayUint8Clamped = ZQ_BUF_TYPED_ARRAY_UINT8C,
    TypedArrayInt16 = ZQ_BUF_TYPED_ARRAY_INT16,
    TypedArrayUint16 = ZQ_BUF_TYPED_ARRAY_UINT16,
    TypedArrayInt32 = ZQ_BUF_TYPED_ARRAY_INT32,
    TypedArrayUint32 = ZQ_BUF_TYPED_ARRAY_UINT32,
    TypedArrayFloat16 = ZQ_BUF_TYPED_ARRAY_FLOAT16,
    TypedArrayFloat32 = ZQ_BUF_TYPED_ARRAY_FLOAT32,
    TypedArrayFloat64 = ZQ_BUF_TYPED_ARRAY_FLOAT64,
    TypedArrayBigInt64 = ZQ_BUF_TYPED_ARRAY_BIGINT64,
    TypedArrayBigUint64 = ZQ_BUF_TYPED_ARRAY_BIGUINT64,
}

impl ZqBufKind {
    /// Decode a raw `ZQ_BUF_*` discriminant; `None` for `ZQ_BUF_NONE`/unknown.
    pub fn from_raw(kind: i32) -> Option<Self> {
        Some(match kind {
            ZQ_BUF_ARRAY_BUFFER => Self::ArrayBuffer,
            ZQ_BUF_SHARED_ARRAY_BUFFER => Self::SharedArrayBuffer,
            ZQ_BUF_DATAVIEW => Self::DataView,
            ZQ_BUF_TYPED_ARRAY_INT8 => Self::TypedArrayInt8,
            ZQ_BUF_TYPED_ARRAY_UINT8 => Self::TypedArrayUint8,
            ZQ_BUF_TYPED_ARRAY_UINT8C => Self::TypedArrayUint8Clamped,
            ZQ_BUF_TYPED_ARRAY_INT16 => Self::TypedArrayInt16,
            ZQ_BUF_TYPED_ARRAY_UINT16 => Self::TypedArrayUint16,
            ZQ_BUF_TYPED_ARRAY_INT32 => Self::TypedArrayInt32,
            ZQ_BUF_TYPED_ARRAY_UINT32 => Self::TypedArrayUint32,
            ZQ_BUF_TYPED_ARRAY_FLOAT16 => Self::TypedArrayFloat16,
            ZQ_BUF_TYPED_ARRAY_FLOAT32 => Self::TypedArrayFloat32,
            ZQ_BUF_TYPED_ARRAY_FLOAT64 => Self::TypedArrayFloat64,
            ZQ_BUF_TYPED_ARRAY_BIGINT64 => Self::TypedArrayBigInt64,
            ZQ_BUF_TYPED_ARRAY_BIGUINT64 => Self::TypedArrayBigUint64,
            _ => return None,
        })
    }
}

mod sealed {
    pub trait Sealed {}
}

/// Element types accepted by [`ZqValue::new_typed_array`].
///
/// Sealed: only the plain integer / float types below implement it, so the
/// slice can always be reinterpreted as raw bytes without padding hazards.
/// Bytes are host-native; on mainstream little-endian hosts this matches the
/// layout JavaScript typed arrays use.
pub trait ZqTypedArrayElem: sealed::Sealed + Copy {
    /// The typed array kind for this element type.
    const KIND: ZqBufKind;

    /// Reinterpret the slice as raw bytes (no copy).
    #[inline]
    fn as_bytes(slice: &[Self]) -> &[u8] {
        // SAFETY: sealed impls are all integer/float POD with no padding
        // bytes, so the whole slice is initialized and readable as `u8`.
        unsafe {
            std::slice::from_raw_parts(slice.as_ptr().cast::<u8>(), std::mem::size_of_val(slice))
        }
    }
}

macro_rules! buf_elem {
    ($ty:ty, $kind:ident) => {
        impl sealed::Sealed for $ty {}
        impl ZqTypedArrayElem for $ty {
            const KIND: ZqBufKind = ZqBufKind::$kind;
        }
    };
}

buf_elem!(i8, TypedArrayInt8);
buf_elem!(u8, TypedArrayUint8);
buf_elem!(i16, TypedArrayInt16);
buf_elem!(u16, TypedArrayUint16);
buf_elem!(i32, TypedArrayInt32);
buf_elem!(u32, TypedArrayUint32);
buf_elem!(f32, TypedArrayFloat32);
buf_elem!(f64, TypedArrayFloat64);

impl<'cx> ZqValue<'cx> {
    /// Kind of this binary buffer, or `None` when it is not one.
    ///
    /// Reads the engine's class identity directly, unlike `constructor.name`,
    /// so it cannot be spoofed by a redefined `constructor` property.
    pub fn buf_kind(&self, ctx: ZqCtx<'cx>) -> Result<Option<ZqBufKind>, ZqError> {
        let api = ZqApi::singleton();
        let kind = unsafe { (api.buf_kind)(ctx.raw, self.raw) };
        Ok(ZqBufKind::from_raw(kind))
    }

    /// Copy the raw bytes of this binary buffer (host byte order) into a `Vec`.
    ///
    /// Works for ArrayBuffer, SharedArrayBuffer, DataView and typed arrays.
    /// Measure-then-copy: two FFI calls regardless of length. Errors when the
    /// value is not a binary buffer.
    pub fn buf_bytes(&self, ctx: ZqCtx<'cx>) -> Result<Vec<u8>, ZqError> {
        let api = ZqApi::singleton();
        let len = unsafe { (api.buf_read_bytes)(ctx.raw, self.raw, std::ptr::null_mut(), 0, 0) };
        if len < 0 {
            return Err(ZqError);
        }
        let mut bytes = vec![0u8; len as usize];
        if len == 0 {
            return Ok(bytes);
        }
        let copied =
            unsafe { (api.buf_read_bytes)(ctx.raw, self.raw, bytes.as_mut_ptr(), bytes.len(), 0) };
        if copied < 0 {
            return Err(ZqError);
        }
        bytes.truncate(copied as usize);
        Ok(bytes)
    }

    /// Create a binary buffer of `kind` by copying `bytes`.
    ///
    /// For typed arrays `bytes.len()` must be a multiple of the element size;
    /// the host throws a `RangeError` otherwise.
    pub fn new_buf(
        ctx: ZqCtx<'cx>,
        kind: ZqBufKind,
        bytes: &[u8],
    ) -> Result<ZqValue<'cx>, ZqError> {
        let api = ZqApi::singleton();
        let v =
            unsafe { (api.new_buf_from_bytes)(ctx.raw, kind as i32, bytes.as_ptr(), bytes.len()) };
        if v == api.const_values[ZQ_CONST_VALUE_SLOT_EXCEPTION] {
            Err(ZqError)
        } else {
            Ok(ZqValue::from(v))
        }
    }

    /// Create an `ArrayBuffer` by copying `bytes`.
    pub fn new_array_buffer(ctx: ZqCtx<'cx>, bytes: &[u8]) -> Result<ZqValue<'cx>, ZqError> {
        Self::new_buf(ctx, ZqBufKind::ArrayBuffer, bytes)
    }

    /// Create a typed array by copying `data`; the element type `T` selects the
    /// typed array class (e.g. `u8` → `Uint8Array`, `f32` → `Float32Array`).
    pub fn typed_array<T: ZqTypedArrayElem>(
        ctx: ZqCtx<'cx>,
        data: &[T],
    ) -> Result<ZqValue<'cx>, ZqError> {
        Self::new_buf(ctx, T::KIND, T::as_bytes(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::ZQ_BUF_NONE;

    #[test]
    fn kind_round_trips_every_discriminant() {
        for kind in [
            ZqBufKind::ArrayBuffer,
            ZqBufKind::SharedArrayBuffer,
            ZqBufKind::DataView,
            ZqBufKind::TypedArrayInt8,
            ZqBufKind::TypedArrayUint8,
            ZqBufKind::TypedArrayUint8Clamped,
            ZqBufKind::TypedArrayInt16,
            ZqBufKind::TypedArrayUint16,
            ZqBufKind::TypedArrayInt32,
            ZqBufKind::TypedArrayUint32,
            ZqBufKind::TypedArrayFloat16,
            ZqBufKind::TypedArrayFloat32,
            ZqBufKind::TypedArrayFloat64,
            ZqBufKind::TypedArrayBigInt64,
            ZqBufKind::TypedArrayBigUint64,
        ] {
            assert_eq!(ZqBufKind::from_raw(kind as i32), Some(kind));
        }
        assert_eq!(ZqBufKind::from_raw(ZQ_BUF_NONE), None);
        assert_eq!(ZqBufKind::from_raw(999), None);
    }

    #[test]
    fn typed_array_elem_maps_kind_and_bytes() {
        assert_eq!(<i8 as ZqTypedArrayElem>::KIND, ZqBufKind::TypedArrayInt8);
        assert_eq!(<u8 as ZqTypedArrayElem>::KIND, ZqBufKind::TypedArrayUint8);
        assert_eq!(<i16 as ZqTypedArrayElem>::KIND, ZqBufKind::TypedArrayInt16);
        assert_eq!(<u16 as ZqTypedArrayElem>::KIND, ZqBufKind::TypedArrayUint16);
        assert_eq!(<i32 as ZqTypedArrayElem>::KIND, ZqBufKind::TypedArrayInt32);
        assert_eq!(<u32 as ZqTypedArrayElem>::KIND, ZqBufKind::TypedArrayUint32);
        assert_eq!(
            <f32 as ZqTypedArrayElem>::KIND,
            ZqBufKind::TypedArrayFloat32
        );
        assert_eq!(
            <f64 as ZqTypedArrayElem>::KIND,
            ZqBufKind::TypedArrayFloat64
        );

        let data = [1i32, 2, 3];
        let bytes = <i32 as ZqTypedArrayElem>::as_bytes(&data);
        assert_eq!(bytes.len(), 12);
        assert_eq!(&bytes[0..4], &1i32.to_ne_bytes());
        assert_eq!(&bytes[8..12], &3i32.to_ne_bytes());
    }
}

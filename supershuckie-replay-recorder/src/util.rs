use alloc::borrow::Cow;
use alloc::format;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::mem::transmute;
use core::ffi::CStr;
use num_enum::TryFromPrimitive;
use zstd_sys::{ZSTD_decompress, ZSTD_getErrorName, ZSTD_isError, ZSTD_maxCLevel, ZSTD_minCLevel};
use crate::replay_file::ReplayHeaderBlake3Hash;

/// Describes an enum that may or may not be valid.
#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(transparent)]
pub struct MaybeEnum<T: Sized + TryFromPrimitive<Primitive: Copy + Clone> + Copy + Clone> {
    inner: T::Primitive
}

impl<T: Sized + TryFromPrimitive<Primitive: Copy + Clone> + Copy + Clone> MaybeEnum<T> {
    /// Instantiate from a valid value.
    pub fn new(value: T) -> MaybeEnum<T> where T: Into<T::Primitive> {
        Self { inner: value.into() }
    }

    /// Get the value if it is valid.
    pub fn get(self) -> Result<T, T::Primitive> {
        T::try_from_primitive(self.inner).map_err(|_| self.inner)
    }

    /// Get the value or return its default if it is not valid.
    pub fn get_or_default(self) -> T where T: Default {
        self.get().unwrap_or(T::default())
    }
}

impl<T: Sized + TryFromPrimitive<Primitive: Copy + Clone> + Copy + Clone + Default + Into<T::Primitive>> Default for MaybeEnum<T> {
    fn default() -> MaybeEnum<T> {
        Self::new(T::default())
    }
}

/// Reinterpret a reference to `F` as `T`.
///
/// # Panics
///
/// Panics if `size_of::<F>() != size_of::<T>()`
///
/// # Safety
///
/// To avoid UB, the following must be true:
/// * `F` and `T` must have the same alignment.
/// * Data on `F` can be safely
pub(crate) const unsafe fn reinterpret_ref<F: Copy, T: Copy>(from: &F) -> &T {
    assert!(size_of::<F>() == size_of::<T>(), "reinterpret_ref cannot be used for different sized types");
    unsafe { transmute(from) }
}

pub(crate) fn compress_data(data: &[u8], compression_level: i32) -> Result<Vec<u8>, Cow<'static, str>> {
    // SAFETY: This function is safe.
    let bound = unsafe { zstd_sys::ZSTD_compressBound(data.len()) };

    // Reserve everything.
    //
    // Internally the vector should now have enough capacity.
    let mut v: Vec<u8> = Vec::new();
    v.try_reserve_exact(bound).map_err(|_| Cow::Borrowed("could not reserve memory for compression buffer"))?;

    // SAFETY: These are safe.
    let level = unsafe { compression_level.clamp(ZSTD_minCLevel() as i32, ZSTD_maxCLevel() as i32) };

    // SAFETY: We've reserved everything and we've supplied the correct arguments
    let compressed_data_len = unsafe {
        zstd_sys::ZSTD_compress(
            v.as_mut_ptr() as *mut c_void,
            v.capacity(),
            data.as_ptr() as *const c_void,
            data.len(),
            level
        )
    };

    // SAFETY: This function is safe.
    if unsafe { ZSTD_isError(compressed_data_len) } != 0 {
        let error_name = unsafe { CStr::from_ptr(ZSTD_getErrorName(compressed_data_len)).to_string_lossy() };
        return Err(Cow::Owned(format!("zstd error: {compressed_data_len} - {error_name}")))
    }

    assert!(compressed_data_len <= bound, "compressed_data_len 0x{compressed_data_len:X} exceeds buffer len 0x{bound:X}");

    // SAFETY: compressed data was initialized
    unsafe { v.set_len(compressed_data_len) };

    Ok(v)
}

pub(crate) fn decompress_data(data: &[u8], uncompressed_size: usize) -> Result<Vec<u8>, Cow<'static, str>> {
    let mut decompressed_data: Vec<u8> = Vec::new();
    if decompressed_data.try_reserve_exact(uncompressed_size).is_err() {
        return Err(Cow::Borrowed("failed to allocate RAM to decompress compressed blob"))
    }

    // SAFETY: Everything's reserved
    let decompressed_len = unsafe {
        ZSTD_decompress(
            decompressed_data.as_mut_ptr() as *mut c_void,
            uncompressed_size,
            data.as_ptr() as *mut c_void,
            data.len()
        )
    };

    if decompressed_len != uncompressed_size {
        // SAFETY: This function is safe.
        return if unsafe { ZSTD_isError(decompressed_len) } != 0 {
            let error_name = unsafe { CStr::from_ptr(ZSTD_getErrorName(decompressed_len)).to_string_lossy() };
            Err(Cow::Owned(format!("zstd error: {decompressed_len} - {error_name}")))
        } else {
            Err(Cow::Owned(format!("Uncompressed size is incorrect (expected {uncompressed_size} but was {decompressed_len})")))
        }
    }

    // SAFETY: It's been initialized.
    unsafe { decompressed_data.set_len(uncompressed_size) };
    Ok(decompressed_data)
}

/// Hash the given data.
pub fn blake3_hash(data: &[u8]) -> ReplayHeaderBlake3Hash {
    *blake3::hash(data).as_bytes()
}

pub(crate) unsafe fn launder_reference<T>(what: &T) -> &'static T {
    unsafe { transmute::<&T, &'static T>(what) }
}

/// Diff two buffers and return the result.
///
/// Return `None` if a diff cannot be made.
#[must_use]
pub fn fast_diff(a_buf: &[u8], b_buf: &[u8]) -> Option<Vec<u64>> {
    // can't use this algorithm if the buffer is too large or bufs aren't the same size
    if a_buf.len() != b_buf.len() || a_buf.len() > u32::MAX as usize {
        return None
    }

    let (a_chunks, a_extra) = a_buf.as_chunks::<4>();
    let (b_chunks, b_extra) = b_buf.as_chunks::<4>();

    let mut diff = Vec::new();

    let mut offset = 0usize;

    for (a,b) in a_chunks.iter().copied().zip(b_chunks.iter().copied()) {
        let this_offset = offset;
        offset += a.len();

        let a_int = u32::from_le_bytes(a);
        let b_int = u32::from_le_bytes(b);

        if a_int != b_int {
            diff.push(((this_offset as u64) << 32) | (b_int as u64));

            // if the diff is more than 25% the size of the original state, it's probably a bad idea
            if diff.len() > a_buf.len() / 4 / 2 / 4 {
                return None;
            }
        }
    }

    // last bit might not be 4 bytes
    if !b_extra.is_empty() && a_extra != b_extra {
        let b_int = u32::from_le_bytes([
            b_extra[0],
            b_extra.get(1).copied().unwrap_or_default(),
            b_extra.get(2).copied().unwrap_or_default(),
            0
        ]);

        diff.push(((offset as u64) << 32) | (b_int as u64));
    }

    Some(diff)
}

/// Apply a diff.
///
/// Return `None` if the diff is wrong.
#[must_use]
pub fn apply_diff(buff: &[u8], diff: &[u64]) -> Option<Vec<u8>> {
    let max_size = (buff.len() + 3) / 4 * 4;
    let mut output = Vec::with_capacity(max_size);
    output.extend_from_slice(buff);
    output.resize(max_size, 0);

    for &d in diff {
        let offset = (d >> 32) as usize;
        let data = (d as u32).to_le_bytes();

        output.get_mut(offset..offset+4)?.copy_from_slice(data.as_slice());
    }

    output.truncate(buff.len());
    Some(output)
}

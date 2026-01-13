use std::borrow::Cow;
use std::ffi::{c_char, CStr};
use std::ptr::null_mut;
use crate::PokeAByteError;

pub struct PokeAByteSharedMemory {
    memory: &'static mut [u8]
}

// macOS mmap is limited to 4 MiB. As such, we cannot (presently) support larger memory mapped files
// here. Note, however, that 4 MiB is sufficient even for NDS games, so we're probably fine as-is.
#[cfg(target_os = "macos")]
pub(crate) const MACOS_MAX_MMAP_MEMORY_LENGTH: usize = 1024 * 1024 * 4;

unsafe extern "C" {
    fn supershuckie_pokeabyte_try_create_shared_memory(len: usize, error: *mut *mut c_char) -> *mut u8;
    fn supershuckie_pokeabyte_close_shared_memory();
}

impl PokeAByteSharedMemory {
    /// # Safety
    ///
    /// The memory returned is not guaranteed to be initialized and must be zero-initialized
    /// manually.
    pub(crate) unsafe fn new(len: usize) -> Result<PokeAByteSharedMemory, PokeAByteError> {
        let mut error = null_mut();
        let memory = unsafe {
            let ram = supershuckie_pokeabyte_try_create_shared_memory(len, &mut error);
            if ram.is_null() {
                return Err(PokeAByteError::SharedMemoryFailure { explanation: Cow::Owned(format!("Error sharing {len} bytes: {}", CStr::from_ptr(error).to_str().unwrap())) })
            }
            std::slice::from_raw_parts_mut(ram, len)
        };

        Ok(Self {
            memory
        })
    }

    /// # Safety
    ///
    /// There is no protection against data races from other processes. It is not recommended to use
    /// this for anything except reading bytes.
    #[inline]
    pub unsafe fn get_memory(&self) -> &[u8] {
        self.memory
    }

    /// # Safety
    ///
    /// There is no protection against data races from other processes. It is not recommended to use
    /// this for anything except reading and writing bytes.
    #[inline]
    pub unsafe fn get_memory_mut(&mut self) -> &mut [u8] {
        self.memory
    }
}

impl Drop for PokeAByteSharedMemory {
    fn drop(&mut self) {
        unsafe { supershuckie_pokeabyte_close_shared_memory() };
    }
}


unsafe impl Sync for PokeAByteSharedMemory {}
unsafe impl Send for PokeAByteSharedMemory {}

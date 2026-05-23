#![no_std]
extern crate alloc;

use alloc::vec::Vec;

#[repr(C)]
struct MGBACoreRaw(());

unsafe extern "C" {
    fn mgba_rs_core_new(
        rom: *const u8,
        rom_size: usize,
        sram: *const u8,
        sram_size: usize,
        bios: *const u8,
        bios_size: usize
    ) -> *mut MGBACoreRaw;
    fn mgba_rs_core_free(core: *mut MGBACoreRaw);
    fn mgba_rs_core_run_frame(core: *mut MGBACoreRaw);
    fn mgba_rs_core_reset(core: *mut MGBACoreRaw);
    fn mgba_rs_core_get_pixels(core: *const MGBACoreRaw) -> *const u32;
    fn mgba_rs_core_set_input(core: *mut MGBACoreRaw, input: u16);
    fn mgba_rs_core_get_sram(core: *const MGBACoreRaw, size: &mut usize) -> *const u8;
    fn mgba_rs_core_create_save_state(core: *const MGBACoreRaw, data: *mut u8, data_size: usize) -> usize;
    fn mgba_rs_core_load_save_state(core: *mut MGBACoreRaw, data: *const u8, data_size: usize) -> bool;
    fn mgba_rs_core_get_ewram(core: *mut MGBACoreRaw) -> *mut [u8; 0x40000];
    fn mgba_rs_core_get_iwram(core: *mut MGBACoreRaw) -> *mut [u8; 0x8000];
    fn mgba_rs_core_audio_sample_rate(core: *const MGBACoreRaw) -> u32;
    fn mgba_rs_core_audio_available(core: *mut MGBACoreRaw) -> usize;
    fn mgba_rs_core_audio_read(core: *mut MGBACoreRaw, out: *mut i16, samples: usize) -> usize;
}

pub struct Core {
    inner: *mut MGBACoreRaw
}

unsafe impl Sync for Core {}
unsafe impl Send for Core {}

impl Core {
    pub fn new(rom: &[u8], sram: &[u8], bios: &[u8]) -> Option<Self> {
        let inner = unsafe { mgba_rs_core_new(rom.as_ptr(), rom.len(), sram.as_ptr(), sram.len(), bios.as_ptr(), bios.len()) };
        if inner.is_null() {
            return None
        }
        Some(Self { inner })
    }

    #[inline]
    pub fn reset(&mut self) {
        unsafe { mgba_rs_core_reset(self.inner) }
    }

    #[inline]
    pub fn run_frame(&mut self) {
        unsafe { mgba_rs_core_run_frame(self.inner) }
    }

    #[inline]
    pub fn get_pixels(&self) -> &[u32; 240 * 160] {
        let pixels = unsafe { mgba_rs_core_get_pixels(self.inner) };
        unsafe { &*(pixels as *const _) }
    }

    #[inline]
    pub fn set_input(&mut self, input: u16) {
        unsafe { mgba_rs_core_set_input(self.inner, input) }
    }

    #[inline]
    pub fn get_sram(&self) -> &[u8] {
        let mut size = 0;
        let ptr = unsafe { mgba_rs_core_get_sram(self.inner, &mut size) };
        unsafe { core::slice::from_raw_parts(ptr, size) }
    }

    #[inline]
    pub fn create_save_state(&self) -> Option<Vec<u8>> {
        let mut v = Vec::with_capacity(32 * 1024 * 1024);

        match unsafe { mgba_rs_core_create_save_state(self.inner, v.as_mut_ptr(), v.capacity()) } {
            0 => None,
            n => Some({
                unsafe { v.set_len(n); }
                v
            })
        }
    }

    #[inline]
    pub fn load_save_state(&self, state: &[u8]) -> bool {
        unsafe { mgba_rs_core_load_save_state(self.inner, state.as_ptr(), state.len()) }
    }

    #[inline]
    pub fn get_ewram(&self) -> &[u8] {
        unsafe { &*mgba_rs_core_get_ewram(self.inner) }.as_slice()
    }

    #[inline]
    pub fn get_ewram_mut(&mut self) -> &mut [u8] {
        unsafe { &mut *mgba_rs_core_get_ewram(self.inner) }.as_mut_slice()
    }

    #[inline]
    pub fn get_iwram(&self) -> &[u8] {
        unsafe { &*mgba_rs_core_get_iwram(self.inner) }.as_slice()
    }

    #[inline]
    pub fn get_iwram_mut(&mut self) -> &mut [u8] {
        unsafe { &mut *mgba_rs_core_get_iwram(self.inner) }.as_mut_slice()
    }

    #[inline]
    pub fn audio_sample_rate(&self) -> u32 {
        unsafe { mgba_rs_core_audio_sample_rate(self.inner) }
    }

    #[inline]
    pub fn audio_available_samples(&mut self) -> usize {
        unsafe { mgba_rs_core_audio_available(self.inner) }
    }

    #[inline]
    pub fn read_audio(&mut self, out: &mut [i16]) -> usize {
        let frames = out.len() / 2;
        if frames == 0 {
            return 0;
        }
        unsafe { mgba_rs_core_audio_read(self.inner, out.as_mut_ptr(), frames) }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        unsafe { mgba_rs_core_free(self.inner) }
    }
}

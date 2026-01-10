use crate::emulator::{EmulatorCore, Input, RunTime, ScreenData, ScreenDataEncoding};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU32, Ordering};
use safeboy::rgb_encoder::encode_a8r8g8b8;
use safeboy::{BorderMode, DirectAccessRegion, Gameboy, GameboyCallbacks, InputButton, RtcMode, RunnableInstanceFunctions, RunningGameboy, TurboMode, VBlankType};
pub use safeboy::Model;
use spin::Lazy;
use supershuckie_replay_recorder::blake3_hash;
use supershuckie_replay_recorder::replay_file::{ReplayConsoleType, ReplayHeaderBlake3Hash};

/// Game Boy and Game Boy Color emulator.
///
/// Uses [SameBoy](https://sameboy.github.io) as the underlying core.
pub struct GameBoyColor {
    core: Gameboy,
    turbo_mode: TurboMode,
    callback_data: Arc<GameBoyCallbackData>,

    rom_checksum: ReplayHeaderBlake3Hash,
    bios_checksum: ReplayHeaderBlake3Hash,
}

struct GameBoyCallbackData {
    run_frames: AtomicU32,
    screen: UnsafeCell<ScreenData>
}

unsafe impl Send for GameBoyCallbackData {}
unsafe impl Sync for GameBoyCallbackData {}

impl GameBoyColor {
    /// Instantiate a `GameBoyColor` emulator from the given ROM.
    pub fn new_from_rom(
        rom: &[u8],
        bios: &[u8],
        sram: Option<&[u8]>,
        model: Model
    ) -> Self {
        let mut core = Gameboy::new(model);
        core.set_rtc_mode(RtcMode::Accurate);
        core.load_boot_rom(bios);
        core.load_rom(rom);

        if let Some(sram) = sram {
            core.load_sram(sram);
        };

        core.set_rgb_encoder(encode_a8r8g8b8);
        core.set_rendering_enabled(true);
        core.set_border_mode(BorderMode::Never);

        let dimensions = core.get_pixel_buffer();
        let screen_data = ScreenData {
            pixels: dimensions.pixels.to_owned(),
            width: dimensions.width as usize,
            height: dimensions.height as usize,
            encoding: ScreenDataEncoding::A8R8G8B8
        };

        let callback_data = Arc::new(GameBoyCallbackData {
            run_frames: AtomicU32::new(0),
            screen: UnsafeCell::new(screen_data)
        });

        core.set_callbacks(Some(Box::new(CallbackHandler { callback_data: callback_data.clone() })));

        let mut r = Self {
            turbo_mode: TurboMode::Disabled,
            callback_data,
            core,
            rom_checksum: blake3_hash(rom),
            bios_checksum: blake3_hash(bios),
        };
        r.hard_reset();
        r
    }
}

struct CallbackHandler {
    callback_data: Arc<GameBoyCallbackData>
}

impl GameboyCallbacks for CallbackHandler {
    fn vblank(&mut self, instance: &mut RunningGameboy, _vblank_type: VBlankType) {
        // SAFETY: Nothing else can currently access this Arc since GameBoyColor is currently
        //         mutably borrowed.
        let screen = unsafe { &mut *self.callback_data.screen.get() };

        screen.pixels.copy_from_slice(instance.get_pixel_buffer_pixels());
        self.callback_data.run_frames.fetch_add(1, Ordering::Relaxed);
    }
}

/// Returns the region and offset.
fn pokeabyte_protocol_region_from_address(address: u32) -> Option<(DirectAccessRegion, usize)> {
    match address {
        // VRAM
        0x8000..=0x9FFF => Some((DirectAccessRegion::VRAM, address as usize - 0x8000)),

        // WRAM bank #0
        0xC000..=0xDFFF => Some((DirectAccessRegion::RAM, address as usize - 0xC000)),

        // WRAM bank #1 (not the actual address)
        0x10000..=0x11FFF => Some((DirectAccessRegion::RAM, address as usize - 0x10000 + 0x2000)),

        // HRAM
        0xFF80..=0xFFFE => Some((DirectAccessRegion::HRAM, address as usize - 0xFF80)),

        _ => None
    }
}

impl EmulatorCore for GameBoyColor {
    fn run(&mut self) -> RunTime {
        self.core.run();
        let frames = self.callback_data.run_frames.swap(0, Ordering::Relaxed) as u64;
        RunTime { frames }
    }

    fn run_unlocked(&mut self) -> RunTime {
        self.core.set_turbo_mode(TurboMode::Enabled);
        let timing = self.run();
        self.core.set_turbo_mode(self.turbo_mode);
        timing
    }

    fn read_ram(&self, address: u32, into: &mut [u8]) -> Result<(), &'static str> {
        let Some((region, offset)) = pokeabyte_protocol_region_from_address(address) else {
            return Err("invalid or unknown address");
        };
        let Some(offset_end) = offset.checked_add(into.len()) else {
            return Err("invalid length");
        };

        let region = self.core.direct_access(region);
        let Some(data) = region.data.get(offset..offset_end) else {
            return Err("address+length overflows");
        };
        into.copy_from_slice(data);
        Ok(())
    }

    fn write_ram(&mut self, address: u32, from: &[u8]) -> Result<(), &'static str> {
        let Some((region, offset)) = pokeabyte_protocol_region_from_address(address) else {
            return Err("invalid or unknown address");
        };
        let Some(offset_end) = offset.checked_add(from.len()) else {
            return Err("invalid length");
        };
        let region = self.core.direct_access_mut(region);
        let Some(data) = region.data.get_mut(offset..offset_end) else {
            return Err("address+length overflows");
        };
        data.copy_from_slice(from);
        Ok(())
    }

    #[inline]
    fn set_speed(&mut self, speed: f64) {
        self.core.set_clock_multiplier(speed);
    }

    fn save_sram(&self) -> Vec<u8> {
        self.core.save_sram()
    }

    fn create_save_state(&self) -> Vec<u8> {
        self.core.create_save_state()
    }

    fn load_save_state(&mut self, state: &[u8]) -> Result<(), String> {
        self.core.load_save_state(state).map_err(|e| alloc::format!("{e:?}"))
    }

    fn encode_input(&self, input: Input, into: &mut Vec<u8>) {
        let mask = (input.a as u8) << InputButton::A
            | (input.b as u8) << InputButton::B
            | (input.start as u8) << InputButton::Start
            | (input.select as u8) << InputButton::Select
            | (input.d_up as u8) << InputButton::Up
            | (input.d_down as u8) << InputButton::Down
            | (input.d_left as u8) << InputButton::Left
            | (input.d_right as u8) << InputButton::Right;
        into.push(mask);
    }

    #[inline]
    fn set_input_encoded(&mut self, input: &[u8]) {
        debug_assert!(input.len() == 1, "set_input_encoded with wrong number of bytes {}", input.len());
        self.core.set_input_button_mask(input[0]);
    }

    #[inline]
    fn get_screens(&self) -> &[ScreenData] {
        // SAFETY: This is going to return a reference with the same lifetime as `self`, thus once
        //         we have to mutably borrow again, the borrow will end.
        let screen_data = unsafe { &*self.callback_data.screen.get() };
        core::slice::from_ref(screen_data)
    }

    #[inline]
    fn swap_screen_data(&mut self, screens: &mut [ScreenData]) {
        assert_eq!(screens.len(), 1, "Invalid screen count");
        let first_screen = &mut screens[0];

        // SAFETY: This won't leave this function.
        let screen_data = unsafe { &mut *self.callback_data.screen.get() };

        assert_eq!(first_screen.pixels.len(), screen_data.pixels.len());
        core::mem::swap(&mut first_screen.pixels, &mut screen_data.pixels);
    }

    #[inline]
    fn hard_reset(&mut self) {
        self.core.reset();

        // skip the intro
        if self.core.is_hle_sgb() {
            let mut state = self.create_save_state();
            state[0x1AB66] = 201;
            state[0x1AB67] = 0;
            let _ = self.load_save_state(&state);
        }
    }

    fn replay_console_type(&self) -> Option<ReplayConsoleType> {
        match self.core.is_cgb() {
            true => Some(ReplayConsoleType::GameBoyColor),
            false => match self.core.is_sgb() {
                false => Some(ReplayConsoleType::GameBoy),
                true => Some(ReplayConsoleType::SuperGameBoy2)
            }
        }
    }

    #[inline]
    fn rom_checksum(&self) -> &ReplayHeaderBlake3Hash {
        &self.rom_checksum
    }

    #[inline]
    fn bios_checksum(&self) -> &ReplayHeaderBlake3Hash {
        &self.bios_checksum
    }

    #[inline]
    fn core_name(&self) -> &'static str {
        if self.core.is_hle_sgb() {
            GB_VERSION_WITH_HACKS.as_str()
        }
        else {
            safeboy::GB_VERSION
        }
    }
}

static GB_VERSION_WITH_HACKS: Lazy<String> = Lazy::new(|| {
    alloc::format!("{} with SGB intro skipped", safeboy::GB_VERSION)
});

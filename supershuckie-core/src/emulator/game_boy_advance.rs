use crate::emulator::{EmulatorCore, Input, RunTime, ScreenData, ScreenDataEncoding};
use alloc::{borrow::ToOwned, string::String, vec::Vec};
use std::prelude::rust_2015::Box;
use mgba_rs::Core;
use supershuckie_replay_recorder::blake3_hash;
use supershuckie_replay_recorder::replay_file::{ReplayConsoleType, ReplayHeaderBlake3Hash};
use crate::{MonotonicTimestampProvider, TimestampMicros};

/// Emulator instance using mGBA.
pub struct GameBoyAdvance {
    core: Core,
    rom_checksum: ReplayHeaderBlake3Hash,
    bios_checksum: ReplayHeaderBlake3Hash,
    screen: ScreenData,
    last_frame_microseconds: TimestampMicros,
    microseconds_per_frames: TimestampMicros,
    clock: Box<dyn MonotonicTimestampProvider>
}

// ~59.7 Hz
const DEFAULT_MICROSECONDS_PER_FRAME: TimestampMicros = 16742;

impl GameBoyAdvance {
    /// Instantiate from a ROM.
    pub fn new_from_rom(rom: &[u8], sram: Option<&[u8]>, bios: &[u8], clock: Box<dyn MonotonicTimestampProvider>) -> Self {
        Self {
            rom_checksum: blake3_hash(rom),
            screen: ScreenData {
                pixels: alloc::vec![0u32; 240*160],
                width: 240,
                height: 160,
                encoding: ScreenDataEncoding::A8R8G8B8
            },
            last_frame_microseconds: 0,
            bios_checksum: blake3_hash(bios),
            microseconds_per_frames: DEFAULT_MICROSECONDS_PER_FRAME,
            core: Core::new(rom, sram.unwrap_or(&[]), bios).expect("failed to make a core (TODO: HANDLE THIS ERROR)"),
            clock,
        }
    }
}

impl EmulatorCore for GameBoyAdvance {
    fn run(&mut self) -> RunTime {
        let expected_next = self.last_frame_microseconds + self.microseconds_per_frames;
        let now = self.clock.get_timestamp_microseconds();
        if now < expected_next {
            return RunTime {
                frames: 0
            }
        }

        let rval = self.run_unlocked();

        // if our clock is way too far behind, limit it a bit
        let several_frames_ago = self.clock
            .get_timestamp_microseconds()
            .saturating_sub(self.microseconds_per_frames * 16);

        self.last_frame_microseconds = (self.last_frame_microseconds + self.microseconds_per_frames)
            .max(several_frames_ago);

        rval
    }

    fn run_unlocked(&mut self) -> RunTime {
        self.core.run_frame();

        // TODO: implement A8B8G8R8 so we don't have to do this swizzling
        for (a, &pixel) in self.screen.pixels.iter_mut().zip(self.core.get_pixels().iter()) {
            let r = (pixel >> 16) & 0xFF;
            let g = pixel & 0xFF00;
            let b = (pixel << 16) & 0xFF0000;

            *a = 0xFF000000 | r | g | b;
        }

        RunTime {
            frames: 1
        }
    }

    fn read_ram(&self, address: u32, into: &mut [u8]) -> Result<(), &'static str> {
        let ram_requested = match address {
            0x2000000..0x2040000 => self.core.get_ewram().get((address - 0x2000000) as usize..).expect("failed to get EWRAM"),
            0x3000000..0x3008000 => self.core.get_iwram().get((address - 0x3000000) as usize..).expect("failed to get IWRAM"),
            _ => return Err("unknown address")
        };

        let Some(ram_slice) = ram_requested.get(..into.len()) else {
            return Err("invalid range (went outside of the region)")
        };

        into.copy_from_slice(ram_slice);
        Ok(())
    }

    fn write_ram(&mut self, address: u32, from: &[u8]) -> Result<(), &'static str> {
        let ram_requested = match address {
            0x2000000..0x2040000 => self.core.get_ewram_mut().get_mut((address - 0x2000000) as usize..).expect("failed to get EWRAM mutably"),
            0x3000000..0x3008000 => self.core.get_iwram_mut().get_mut((address - 0x3000000) as usize..).expect("failed to get IWRAM mutably"),
            _ => return Err("unknown address")
        };

        let Some(ram_slice) = ram_requested.get_mut(..from.len()) else {
            return Err("invalid range (went outside of the region)")
        };

        ram_slice.copy_from_slice(from);
        Ok(())
    }

    #[inline]
    fn set_speed(&mut self, speed: f64) {
        self.microseconds_per_frames = (DEFAULT_MICROSECONDS_PER_FRAME as f64 / speed).clamp(0.0, u32::MAX as f64) as u64;
    }

    #[inline]
    fn save_sram(&self) -> Vec<u8> {
        self.core.get_sram().to_vec()
    }

    fn audio_sample_rate(&self) -> Option<u32> {
        Some(self.core.audio_sample_rate())
    }

    fn audio_available_samples(&mut self) -> usize {
        self.core.audio_available_samples()
    }

    fn read_audio(&mut self, out: &mut [i16]) -> usize {
        self.core.read_audio(out)
    }

    #[inline]
    fn create_save_state(&self) -> Vec<u8> {
        self.core.create_save_state().expect("failed to create GBA save state")
    }

    #[inline]
    fn load_save_state(&mut self, state: &[u8]) -> Result<(), String> {
        if self.core.load_save_state(state) {
            Ok(())
        }
        else {
            Err("loading save state to mGBA failed".to_owned())
        }
    }

    fn encode_input(&self, input: Input, into: &mut Vec<u8>) {
        let mut value = 0u16;

        value |= (input.a as u16) << 0;
        value |= (input.b as u16) << 1;
        value |= (input.d_left as u16) << 5;
        value |= (input.d_right as u16) << 4;
        value |= (input.d_up as u16) << 6;
        value |= (input.d_down as u16) << 7;
        value |= (input.l as u16) << 9;
        value |= (input.r as u16) << 8;
        value |= (input.select as u16) << 2;
        value |= (input.start as u16) << 3;

        into.clear();
        into.extend_from_slice(value.to_le_bytes().as_slice());
    }

    #[inline]
    fn set_input_encoded(&mut self, input: &[u8]) {
        let [a, b] = input else {
            return self.set_input_encoded(&[0,0])
        };
        self.core.set_input(u16::from_le_bytes([*a, *b]))
    }

    #[inline]
    fn get_screens(&self) -> &[ScreenData] {
        core::slice::from_ref(&self.screen)
    }

    #[inline]
    fn swap_screen_data(&mut self, screens: &mut [ScreenData]) {
        assert_eq!(screens.len(), 1, "expected one screen to be swapped");
        core::mem::swap(&mut screens[0].pixels, &mut self.screen.pixels)
    }

    #[inline]
    fn hard_reset(&mut self) {
        self.core.reset()
    }

    #[inline]
    fn replay_console_type(&self) -> Option<ReplayConsoleType> {
        Some(ReplayConsoleType::GameBoyAdvance)
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
        "mGBA 0.10.5-76d93515629031047ca2b7bcb6237089adcb36d7 with BIOS skipped"
    }
}

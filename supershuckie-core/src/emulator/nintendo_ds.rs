use alloc::vec::Vec;
use crate::emulator::{EmulatorCore, Input, RunTime, ScreenData, ScreenDataEncoding};
use alloc::string::String;
use alloc::borrow::ToOwned;
use melonds_rs::Core;
use supershuckie_replay_recorder::blake3_hash;
use supershuckie_replay_recorder::replay_file::{ReplayConsoleType, ReplayHeaderBlake3Hash};
use alloc::boxed::Box;
use crate::{MonotonicTimestampProvider, TimestampMicros};

/// A NDS emulator.
///
/// Uses [melonDS](http://melonds.kuribo64.net) as the underlying core.
pub struct NintendoDS {
    screens: [ScreenData; 2],
    rom_checksum: ReplayHeaderBlake3Hash,
    core: Core,
    last_frame_microseconds: TimestampMicros,
    microseconds_per_frames: TimestampMicros,
    clock: Box<dyn MonotonicTimestampProvider>
}

impl NintendoDS {
    /// Instantiate from a ROM.
    pub fn new_from_rom(rom: &[u8], sram: Option<&[u8]>, clock: Box<dyn MonotonicTimestampProvider>, jit: bool) -> Self {
        Self {
            rom_checksum: blake3_hash(rom),
            screens: core::array::from_fn(|_| ScreenData {
                pixels: alloc::vec![0u32; 256*192],
                width: 256,
                height: 192,
                encoding: ScreenDataEncoding::A8R8G8B8
            }),
            core: Core::new(rom, sram.unwrap_or(&[]), jit).expect("failed to make a core (TODO: HANDLE THIS ERROR)"),
            last_frame_microseconds: 0,
            microseconds_per_frames: DEFAULT_MICROSECONDS_PER_FRAME,
            clock
        }
    }

    /// Set the date.
    #[inline]
    pub fn set_date(
        &mut self,
        year: u16,
        month: u8,
        day: u8,
        hour: u8,
        minute: u8,
        second: u8
    ) {
        self.core.set_date(year, month, day, hour, minute, second);
    }
}

const DEFAULT_MICROSECONDS_PER_FRAME: u64 = 1000000 / 60;

#[allow(unused_variables)]
impl EmulatorCore for NintendoDS {
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

        let pixels_a = self.core.get_pixels(0).expect("no pixels????");
        let pixels_b = self.core.get_pixels(1).expect("no pixels????");

        self.screens[0].pixels.copy_from_slice(pixels_a.as_slice());
        self.screens[1].pixels.copy_from_slice(pixels_b.as_slice());

        RunTime {
            frames: 1
        }
    }

    fn read_ram(&self, address: u32, into: &mut [u8]) -> Result<(), &'static str> {
        let offset = address.checked_sub(0x2000000)
            .ok_or("no such ram address")? as usize;
        let range = offset..(offset + into.len());
        let range_data = self.core.get_main_ram().get(range).ok_or("out of range read")?;
        into.copy_from_slice(range_data);

        Ok(())
    }

    fn write_ram(&mut self, address: u32, from: &[u8]) -> Result<(), &'static str> {
        let offset = address.checked_sub(0x2000000)
            .ok_or("no such ram address")? as usize;
        let range = offset..(offset + from.len());
        let range_data = self.core.get_main_ram_mut().get_mut(range).ok_or("out of range write")?;
        range_data.copy_from_slice(from);

        Ok(())
    }

    fn set_speed(&mut self, speed: f64) {
        self.microseconds_per_frames = (DEFAULT_MICROSECONDS_PER_FRAME as f64 / speed).clamp(0.0, u32::MAX as f64) as u64;
    }

    fn save_sram(&self) -> Vec<u8> {
        self.core.get_sram().to_vec()
    }

    fn create_save_state(&self) -> Vec<u8> {
        self.core.create_save_state().expect("failed to make NDS save state???")
    }

    fn load_save_state(&mut self, state: &[u8]) -> Result<(), String> {
        self.core
            .load_save_state(state)
            .then_some(())
            .ok_or_else(|| "failed to load nds save state".to_owned())
    }

    fn encode_input(&self, input: Input, into: &mut Vec<u8>) {
        let mut value = 0u32;

        value |= (input.a as u32) << 0;
        value |= (input.b as u32) << 1;
        value |= (input.x as u32) << 10;
        value |= (input.y as u32) << 11;
        value |= (input.d_left as u32) << 5;
        value |= (input.d_right as u32) << 4;
        value |= (input.d_up as u32) << 6;
        value |= (input.d_down as u32) << 7;
        value |= (input.l as u32) << 9;
        value |= (input.r as u32) << 8;
        value |= (input.select as u32) << 2;
        value |= (input.start as u32) << 3;

        if let Some((x,y)) = input.touch {
            value |= 0x1000;
            value |= (x as u32) << 16;
            value |= (y as u32) << 24;
        }

        into.clear();
        into.extend_from_slice(value.to_le_bytes().as_slice());
    }

    fn set_input_encoded(&mut self, input: &[u8]) {
        self.core.set_input(u32::from_le_bytes(input.try_into().unwrap_or([0u8; 4])))
    }

    fn get_screens(&self) -> &[ScreenData] {
        &self.screens
    }

    fn swap_screen_data(&mut self, screens: &mut [ScreenData]) {
        assert_eq!(screens.len(), self.screens.len(), "expected two screens to be swapped");
        for (a, b) in self.screens.iter_mut().zip(screens.iter_mut()) {
            core::mem::swap(&mut a.pixels, &mut b.pixels)
        }
    }

    fn hard_reset(&mut self) {
        self.core.reset();
    }

    fn replay_console_type(&self) -> Option<ReplayConsoleType> {
        Some(ReplayConsoleType::NintendoDS)
    }

    fn rom_checksum(&self) -> &ReplayHeaderBlake3Hash {
        &self.rom_checksum
    }

    fn bios_checksum(&self) -> &ReplayHeaderBlake3Hash {
        &const { unsafe { core::mem::zeroed() } }
    }

    fn core_name(&self) -> &'static str {
        "melonDS 1.1 [SUPERSHUCKIE-EXPERIMENTAL-0]"
    }
}

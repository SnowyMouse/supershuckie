//! Functionality for emulator cores.

mod game_boy_color;
mod null;
mod nintendo_ds;
mod game_boy_advance;

use alloc::string::String;

pub use game_boy_color::*;
pub use null::*;
pub use nintendo_ds::*;
pub use game_boy_advance::*;

use alloc::vec::Vec;
use std::num::NonZeroU64;
use supershuckie_replay_recorder::ByteVec;
use supershuckie_replay_recorder::replay_file::{ReplayConsoleType, ReplayHeaderBlake3Hash, ReplayPatchFormat};
use supershuckie_replay_recorder::replay_file::record::{ReplayFileRecorderSettings, ReplayFileSink};

/// Emulator core functionality.
pub trait EmulatorCore: Send + 'static {
    /// Run the smallest amount of time.
    fn run(&mut self) -> RunTime;

    /// Run the smallest amount of time without any timing.
    fn run_unlocked(&mut self) -> RunTime;

    /// Read RAM at the given address to the given data buffer.
    ///
    /// Note: The way `address` is interpreted is core-specific.
    fn read_ram(&self, address: u32, into: &mut [u8]) -> Result<(), &'static str>;

    /// Write RAM to the given address from the given data buffer.
    ///
    /// Note: The way `address` is interpreted is core-specific.
    fn write_ram(&mut self, address: u32, from: &[u8]) -> Result<(), &'static str>;

    /// Set the game speed multiplier.
    fn set_speed(&mut self, speed: f64);

    /// Create SRAM.
    fn save_sram(&self) -> Vec<u8>;

    /// Create a save state.
    fn create_save_state(&self) -> Vec<u8>;

    /// Load a save state.
    fn load_save_state(&mut self, state: &[u8]) -> Result<(), String>;

    /// Encode the input.
    ///
    /// `into` can be assumed to be empty
    fn encode_input(&self, input: Input, into: &mut Vec<u8>);

    /// Set the current input.
    ///
    /// It must be encoded by `encode_input`.
    fn set_input_encoded(&mut self, input: &[u8]);

    /// Get the screen(s).
    fn get_screens(&self) -> &[ScreenData];

    /// Swap screen data.
    ///
    /// Note: Swapping twice does not guarantee getting the original screen data back, as the
    /// implementation may copy, instead.
    fn swap_screen_data(&mut self, screens: &mut [ScreenData]);

    /// Hard reset the console.
    ///
    /// This simulates instantly turning it off and on.
    fn hard_reset(&mut self);

    /// Get the replay type.
    fn replay_console_type(&self) -> Option<ReplayConsoleType>;

    /// Get the checksum of the currently running ROM.
    fn rom_checksum(&self) -> &ReplayHeaderBlake3Hash;

    /// Get the checksum of the currently running BIOS.
    fn bios_checksum(&self) -> &ReplayHeaderBlake3Hash;

    /// Get the current core name.
    fn core_name(&self) -> &'static str;
}

/// Amount of time passed when running the emulator core.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct RunTime {
    /// Frames passed.
    pub frames: u64
}

/// Describes a current input state.
#[derive(Copy, Clone, PartialEq, Debug)]
#[allow(missing_docs)]
pub struct Input {
    pub a: bool,
    pub b: bool,
    pub start: bool,
    pub select: bool,

    pub d_up: bool,
    pub d_down: bool,
    pub d_left: bool,
    pub d_right: bool,

    pub l: bool,
    pub r: bool,
    pub x: bool,
    pub y: bool,

    pub touch: Option<(u8, u8)>
}

impl Default for Input {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Input {
    /// Instantiate an empty input.
    #[inline]
    pub const fn new() -> Self {
        Self {
            a: false,
            b: false,
            start: false,
            select: false,
            d_up: false,
            d_down: false,
            d_left: false,
            d_right: false,
            l: false,
            r: false,
            x: false,
            y: false,
            touch: None,
        }
    }

    /// Return true if the input is empty.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        !self.a
        && !self.b
        && !self.start
        && !self.select
        && !self.d_up
        && !self.d_down
        && !self.d_left
        && !self.d_right
        && !self.l
        && !self.r
        && !self.x
        && !self.y
        && self.touch.is_none()
    }
}

impl core::ops::BitOr<Input> for Input {
    type Output = Input;
    fn bitor(self, rhs: Input) -> Self::Output {
        Input {
            a: self.a | rhs.a,
            b: self.b | rhs.b,
            start: self.start | rhs.start,
            select: self.select | rhs.select,
            d_up: self.d_up | rhs.d_up,
            d_down: self.d_down | rhs.d_down,
            d_left: self.d_left | rhs.d_left,
            d_right: self.d_right | rhs.d_right,
            l: self.l | rhs.l,
            r: self.r | rhs.r,
            x: self.x | rhs.x,
            y: self.y | rhs.y,
            touch: self.touch.or(rhs.touch),
        }
    }
}

impl core::ops::BitAnd<Input> for Input {
    type Output = Input;
    fn bitand(self, rhs: Input) -> Self::Output {
        Input {
            a: self.a & rhs.a,
            b: self.b & rhs.b,
            start: self.start & rhs.start,
            select: self.select & rhs.select,
            d_up: self.d_up & rhs.d_up,
            d_down: self.d_down & rhs.d_down,
            d_left: self.d_left & rhs.d_left,
            d_right: self.d_right & rhs.d_right,
            l: self.l & rhs.l,
            r: self.r & rhs.r,
            x: self.x & rhs.x,
            y: self.y & rhs.y,
            touch: self.touch.and(rhs.touch),
        }
    }
}

impl core::ops::Not for Input {
    type Output = Input;

    fn not(self) -> Self::Output {
        Self {
            a: !self.a,
            b: !self.b,
            start: !self.start,
            select: !self.select,
            d_up: !self.d_up,
            d_down: !self.d_down,
            d_left: !self.d_left,
            d_right: !self.d_right,
            l: !self.l,
            r: !self.r,
            x: !self.x,
            y: !self.y,
            touch: None
        }
    }
}

impl core::ops::BitAndAssign for Input {
    fn bitand_assign(&mut self, rhs: Self) {
        *self = *self & rhs;
    }
}

impl core::ops::BitOrAssign for Input {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = *self | rhs;
    }
}

/// Describes screen data.
#[derive(Clone, PartialEq)]
pub struct ScreenData {
    /// Pixels, encoded.
    pub pixels: Vec<u32>,

    /// Width in pixels.
    ///
    /// Note: This is not allowed to change.
    pub width: usize,

    /// Height in pixels.
    ///
    /// Note: This is not allowed to change.
    pub height: usize,

    /// Encoding to use.
    ///
    /// Note: This is not allowed to change.
    pub encoding: ScreenDataEncoding
}

impl Default for ScreenData {
    fn default() -> Self {
        Self {
            pixels: Vec::new(),
            width: 0,
            height: 0,
            encoding: ScreenDataEncoding::A8R8G8B8
        }
    }
}

/// Describes the color encoding.
#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(u32)]
pub enum ScreenDataEncoding {
    /// 0xAARRGGBB
    A8R8G8B8
}

fn _ensure_emulator_core_is_dyn_compatible(_core: &dyn EmulatorCore) {}

/// Partial recording metadata for a SuperShuckie core replay.
pub struct PartialReplayRecordMetadata<
    FS: ReplayFileSink + Send + Sync + 'static,
    TS: ReplayFileSink + Send + Sync + 'static
> {
    /// Name of the ROM (can also be the filename)
    pub rom_name: String,

    /// Filename of the ROM
    pub rom_filename: String,

    /// Encoding settings to use
    pub settings: ReplayFileRecorderSettings,

    /// Patch format to use
    pub patch_format: ReplayPatchFormat,

    /// Checksum of the patch (can be zeroed if no patch)
    pub patch_target_checksum: ReplayHeaderBlake3Hash,

    /// Data of the patch (can be empty if no patch)
    pub patch_data: ByteVec,

    /// Number of frames between keyframes.
    ///
    /// Lower numbers will improve seeking performance but increase file and memory size.
    pub frames_per_keyframe: NonZeroU64,

    /// Final file to write to
    pub final_file: FS,

    /// Temp file tow rite to
    pub temp_file: TS,
}

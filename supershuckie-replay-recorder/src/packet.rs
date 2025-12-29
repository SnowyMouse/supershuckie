use tinyvec::TinyVec;
use alloc::string::String;
use alloc::vec::Vec;
use core::num::NonZeroU16;

mod io;
pub use io::*;

#[allow(missing_docs)]
pub type InputBuffer = TinyVec<[u8; 16]>;
#[allow(missing_docs)]
pub type UnsignedInteger = u64;
#[allow(missing_docs)]
pub type TimestampMillis = UnsignedInteger;
#[allow(missing_docs)]
pub type ByteVec = TinyVec<[u8; 16]>;

/// Describes an individual packet.
#[derive(Clone, PartialEq, Debug)]
pub enum Packet {
    /// Do nothing
    NoOp,

    /// Run emulator for one frame.
    /// 
    /// `timestamp_delta` is the time passed since the last frame.
    #[allow(missing_docs)]
    NextFrame { timestamp_delta: TimestampMillis },

    /// Write RAM to the given address
    /// 
    /// How the address is interpreted is emulator-specific
    #[allow(missing_docs)]
    WriteMemory { address: UnsignedInteger, data: ByteVec },

    /// Set the current input.
    #[allow(missing_docs)]
    ChangeInput { data: InputBuffer },

    /// Set the current speed.
    #[allow(missing_docs)]
    ChangeSpeed { speed: Speed },

    /// Hard reset the console.
    ResetConsole,

    /// Load a save state.
    #[allow(missing_docs)]
    LoadSaveState { state: ByteVec },

    /// Describes a named point in the replay.
    #[allow(missing_docs)]
    Bookmark { metadata: BookmarkMetadata },

    /// Adds a keyframe so the replay can be scanned faster.
    #[allow(missing_docs)]
    Keyframe {
        metadata: KeyframeMetadata,
        state: ByteVec
    },

    /// Adds a diffed keyframe so the replay can be scanned faster.
    ///
    /// The player will automatically convert these into regular keyframes using
    /// (`apply_diff`)[`crate::util::apply_diff`].
    #[allow(missing_docs)]
    DeltaKeyframe {
        metadata: KeyframeMetadata,
        diff: Vec<UnsignedInteger>
    },

    /// Describes a compressed blob of memory.
    #[allow(missing_docs)]
    CompressedBlob {
        keyframes: Vec<KeyframeMetadata>,
        bookmarks: Vec<BookmarkMetadata>,
        compressed_data: ByteVec,
        uncompressed_size: UnsignedInteger,
        timestamp_start: TimestampMillis,
        timestamp_end: TimestampMillis,
        elapsed_frames_start: UnsignedInteger,
        elapsed_frames_end: UnsignedInteger
    }
}

/// Speed value that uses a fixed point number.
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(transparent)]
pub struct Speed {
    /// A fixed point number that, when divided by 256, will yield the speed value.
    pub speed_over_256: NonZeroU16
}

impl Speed {
    /// Get the speed value from a multiplier.
    pub const fn from_multiplier_float(multiplier: f64) -> Self {
        Self {
            speed_over_256: match NonZeroU16::new((multiplier * 256.0) as u16) {
                Some(n) => n,
                None => NonZeroU16::new(1).expect("1 is not 0")
            }
        }
    }
    /// Convert the speed value into a multiplier.
    pub const fn into_multiplier_float(self) -> f64 {
        (self.speed_over_256.get() as f64) / 256.0
    }
}

impl Default for Speed {
    fn default() -> Self {
        Self::from_multiplier_float(1.0)
    }
}

/// Payload for keyframes, not including save state data
#[derive(Clone, PartialEq, Debug, Default)]
pub struct KeyframeMetadata {
    /// Current input
    pub input: InputBuffer,

    /// Current speed
    pub speed: Speed,

    /// Number of elapsed frames
    pub elapsed_frames: UnsignedInteger,

    /// Total elapsed milliseconds
    pub elapsed_millis: TimestampMillis
}

/// Payload for bookmarks
#[derive(Clone, PartialEq, Debug, Default)]
pub struct BookmarkMetadata {
    /// Name of the bookmark
    pub name: String,

    /// Number of elapsed frames
    pub elapsed_frames: UnsignedInteger,

    /// Total elapsed milliseconds
    pub elapsed_millis: TimestampMillis
}

use tinyvec::TinyVec;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{Display, Formatter};
use core::num::NonZeroU16;
use core::cmp::Ordering;

mod io;
pub use io::*;

#[allow(missing_docs)]
pub type InputBuffer = TinyVec<[u8; 16]>;
#[allow(missing_docs)]
pub type UnsignedInteger = u64;
#[allow(missing_docs)]
pub type SignedInteger = i64;

#[derive(Copy, Clone, PartialEq, Debug, Default, Ord, Eq)]
#[repr(transparent)]
#[allow(missing_docs)]
pub struct TimestampMillis(pub UnsignedInteger);

impl PartialOrd for TimestampMillis {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.0.cmp(&other.0))
    }
}

impl core::fmt::Display for TimestampMillis {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.0, f)
    }
}

impl From<u64> for TimestampMillis {
    #[inline]
    fn from(value: u64) -> Self {
        Self(value)
    }
}

impl PacketIO<'_> for TimestampMillis {
    fn write_packet_instructions(&self) -> PacketInstructionsVec<'_> {
        self.0.write_packet_instructions()
    }
    fn read_all(from: &mut &[u8], version: u32) -> Result<Self, PacketReadError> {
        Ok(Self(UnsignedInteger::read_all(from, version)?))
    }
}

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
    },

    /// Modifies a counter
    #[allow(missing_docs)]
    IncrementCounter {
        name: String,
        delta: SignedInteger
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

impl Display for Speed {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        Display::fmt(&self.into_multiplier_float(), f)
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
    pub elapsed_millis: TimestampMillis,

    /// Counters thus far
    pub counters: Vec<Counter>
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

/// Counter data
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Counter {
    /// Name of the counter.
    pub name: String,

    /// Value of the counter
    pub value: SignedInteger
}

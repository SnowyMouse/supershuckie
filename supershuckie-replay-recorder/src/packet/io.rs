use core::cmp::Ordering;

use crate::packet::{BookmarkMetadata, ByteVec, KeyframeMetadata, Packet, Speed, UnsignedInteger};
use crate::{InputBuffer, TimestampMillis};
use alloc::borrow::{Cow, ToOwned};
use alloc::string::String;
use alloc::vec::Vec;
use core::num::NonZeroU16;
use num_enum::TryFromPrimitive;
use tinyvec::TinyVec;

/// Describes how to write data to a stream.
#[derive(Clone, Debug, PartialEq)]
#[allow(missing_docs)]
pub enum PacketWriteCommand<'a> {
    WriteByte { byte: u8 },
    WriteSlice { bytes: &'a [u8] },
    WriteVec { bytes: ByteVec }
}

impl PacketWriteCommand<'_> {
    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Self::WriteByte { byte } => core::slice::from_ref(byte),
            Self::WriteSlice { bytes } => *bytes,
            Self::WriteVec { bytes } => bytes.as_slice()
        }
    }
}

impl Default for PacketWriteCommand<'_> {
    fn default() -> Self {
        Self::WriteByte { byte: 0 }
    }
}

/// Defines data that can be written to/from a replay stream.
pub trait PacketIO<'a>: Sized + 'a {
    /// Readable-name of the packet.
    fn name(&self) -> &'static str {
        core::any::type_name_of_val(self)
    }

    /// Get a list of write instructions.
    /// 
    /// You can use this to write the data to both buffers and streams without duplicating logic.
    fn write_packet_instructions(&'a self) -> PacketInstructionsVec<'a>;

    /// Attempt to read all bytes.
    /// 
    /// Also moves the reference `from` so it points to the next readable object (or the end of the slice if the end has been reached).
    fn read_all(from: &mut &'a[u8]) -> Result<Self, PacketReadError>;
}

/// Container for packet instructions.
pub type PacketInstructionsVec<'a> = TinyVec<[PacketWriteCommand<'a>; 32]>;

// For UnsignedIntegers, we convert to little endian bytes.
//
// We then remove any trailing 00's on the right (to do this we can just truncate to (log2(*self) + 7) / 8).
//
// We then store the length as u8 followed by the little endian bytes
impl PacketIO<'_> for UnsignedInteger {
    fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_> {
        if *self == 0 {
            return core::iter::once(PacketWriteCommand::WriteByte { byte: 0 }).collect();
        }

        let mut bytes= TinyVec::new();
        bytes.extend_from_slice(self.to_le_bytes().as_slice());

        // get number of bytes needed to read it...
        bytes.truncate((1 + self.ilog2() / 8) as usize);

        let mut writer = PacketInstructionsVec::new();
        writer.push(PacketWriteCommand::WriteByte { byte: bytes.len() as u8 });
        writer.push(PacketWriteCommand::WriteVec { bytes });
        writer
    }
    fn read_all(what: &mut &[u8]) -> Result<Self, PacketReadError> {
        let Some((&[len_byte], remaining_bytes)) = what.split_at_checked(1) else {
            return Err(PacketReadError::NotEnoughData)
        };

        // short circuit if 0
        if len_byte == 0 {
            *what = remaining_bytes;
            return Ok(0)
        }

        // Now let's try to get the bytes...
        let len = len_byte as usize;
        let mut destination = [0u8; 8];

        // since it's little endian, all of the bytes will be positioned at the start of the buffer
        let Some(destination_output) = destination.get_mut(..len) else {
            return Err(PacketReadError::ParseFail { explanation: Cow::Owned(alloc::format!("invalid UnsignedInteger (bad byte length {len})")) })
        };
        let Some((bytes, extra_bytes)) = remaining_bytes.split_at_checked(len) else {
            return Err(PacketReadError::NotEnoughData)
        };

        destination_output.copy_from_slice(bytes);
        *what = extra_bytes;

        Ok(UnsignedInteger::from_le_bytes(destination))
    }
}

impl PacketIO<'_> for usize {
    fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_> {
        let v = UnsignedInteger::try_from(*self).expect("failed to convert usize to UnsignedInteger; target architecture exceeds 64 bits?");
        static_packet_write_array_references(v.write_packet_instructions())
    }
    fn read_all(what: &mut &[u8]) -> Result<Self, PacketReadError> {
        let size = UnsignedInteger::read_all(what)?;
        usize::try_from(size)
            .map_err(|_| PacketReadError::ParseFail { explanation: Cow::Borrowed("unable to parse usize; the usize is too large for this architecture") })
    }
}

// For TinyVecs, we put the size followed by the actual bytes
impl PacketIO<'_> for ByteVec {
    fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_> {
        let mut instructions: PacketInstructionsVec = PacketInstructionsVec::new();
        instructions.extend(static_packet_write_array_references(self.len().write_packet_instructions()));
        instructions.push(PacketWriteCommand::WriteSlice { bytes: self.as_slice() });
        instructions
    }
    fn read_all(what: &mut &[u8]) -> Result<Self, PacketReadError> {
        let len = usize::read_all(what)?;
        let Some((bytes, extra)) = what.split_at_checked(len) else {
            return Err(PacketReadError::NotEnoughData)
        };

        *what = extra;

        let mut s = Self::with_capacity(len);
        s.extend_from_slice(bytes);
        Ok(s)
    }
}

impl<'a> PacketIO<'a> for &'a str {
    fn write_packet_instructions(&'a self) -> PacketInstructionsVec<'a> {
        let mut instructions = PacketInstructionsVec::new();
        instructions.extend(static_packet_write_array_references(self.len().write_packet_instructions()));
        instructions.push(PacketWriteCommand::WriteSlice { bytes: self.as_bytes() });
        instructions
    }

    fn read_all(from: &mut &'a [u8]) -> Result<Self, PacketReadError> {
        let len = usize::read_all(from)?;
        let Some((str_bytes, extra)) = from.split_at_checked(len) else {
            return Err(PacketReadError::NotEnoughData)
        };
        *from = extra;

        str::from_utf8(str_bytes)
            .map_err(|_| PacketReadError::ParseFail { explanation: Cow::Borrowed("invalid utf8 sequence") })
    }
}

impl PacketIO<'_> for String {
    fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_> {
        let mut instructions = PacketInstructionsVec::new();
        instructions.extend(static_packet_write_array_references(self.len().write_packet_instructions()));
        instructions.push(PacketWriteCommand::WriteSlice { bytes: self.as_bytes() });
        instructions
    }

    fn read_all(from: &mut &[u8]) -> Result<Self, PacketReadError> {
        <&str>::read_all(from).map(|i| i.to_owned())
    }
}

impl<'a, T: PacketIO<'a>> PacketIO<'a> for Vec<T> {
    fn write_packet_instructions(&'a self) -> PacketInstructionsVec<'a> {
        let mut instructions = PacketInstructionsVec::new();
        instructions.extend(static_packet_write_array_references(self.len().write_packet_instructions()));
        for i in self {
            instructions.extend(i.write_packet_instructions());
        }
        instructions
    }
    fn read_all(what: &mut &'a [u8]) -> Result<Self, PacketReadError> {
        let len = usize::read_all(what)?;
        let mut s = Self::with_capacity(len);
        for _ in 0..len {
            s.push(T::read_all(what)?);
        }

        Ok(s)
    }
}

/// Describes an error that occurs when failing to read a packet
#[derive(Clone, PartialEq, Debug)]
#[allow(missing_docs)]
pub enum PacketReadError {
    NotEnoughData,
    ParseFail { explanation: Cow<'static, str> }
}

// Make ArrayVec<[PacketWriteCommand<'_>; LEN]> into 'static.
// Useful for temporarily made values
fn static_packet_write_array_references<'a, 'b, const LEN: usize>(from: TinyVec<[PacketWriteCommand<'a>; LEN]>) -> TinyVec<[PacketWriteCommand<'b>; LEN]> {
    let mut result = TinyVec::new();
    for i in from {
        let bytes = i.bytes();

        if bytes.is_empty() {
            continue
        }

        if bytes.len() == 1 {
            result.push(PacketWriteCommand::WriteByte { byte: bytes[0] });
            continue
        }

        match i {
            PacketWriteCommand::WriteByte { .. } => unreachable!("already wrote a byte"),
            PacketWriteCommand::WriteVec { bytes } => result.push(PacketWriteCommand::WriteVec { bytes }),
            PacketWriteCommand::WriteSlice { bytes } => {
                let mut v = TinyVec::new();
                v.extend_from_slice(bytes);
                result.push(PacketWriteCommand::WriteVec { bytes: v });
            }
        }
    }
    result
}

/// Used in raw data to determine what kind of packet something is.
#[derive(Copy, Clone, PartialEq, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum PacketDiscriminator {
    /// Do nothing.
    NoOp = 0x00,

    /// Run for one frame.
    NextFrame = 0x80,

    /// 8-bit input
    ChangeInput8 = 0x81,

    /// 16-bit input
    ChangeInput16 = 0x82,

    /// 32-bit input
    ChangeInput32 = 0x83,

    /// Variable input
    ChangeInputVar = 0x84,

    /// Write a single byte.
    WriteMemory8 = 0x85,

    /// Write a 16-bit value.
    WriteMemory16 = 0x86,

    /// Write a 32-bit value.
    WriteMemory32 = 0x87,

    /// Write a variable amount of data.
    WriteMemoryVar = 0x88,

    /// Describes a keyframe
    Keyframe = 0xF0,

    /// Describes a bookmark
    Bookmark = 0xF1,

    /// Change speed command
    ChangeSpeed = 0xF2,

    /// Hard reset the console
    ResetConsole = 0xF3,

    /// Load the save state at the given keyframe
    LoadSaveState = 0xF4,

    /// Describes a keyframe with a diff
    DeltaKeyframe = 0xF5,

    /// Compressed blob
    CompressedBlob = 0xFE,
    
    // In case we need another 255 discriminators
    // Extended = 0xFF,
}

impl PartialEq<PacketDiscriminator> for u8 {
    fn eq(&self, other: &PacketDiscriminator) -> bool {
        *self == *other as u8
    }
}

impl PartialOrd<PacketDiscriminator> for u8 {
    fn partial_cmp(&self, other: &PacketDiscriminator) -> Option<Ordering> {
        self.partial_cmp(&(*other as u8))
    }
}

macro_rules! packet_io_for_int {
    ($int_type:tt) => {
        impl PacketIO<'_> for $int_type {
            fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_> {
                core::iter::once(PacketWriteCommand::WriteVec { bytes: (*self).to_le_bytes().as_slice().into() }).collect()
            }
            fn read_all(from: &mut &[u8]) -> Result<Self, PacketReadError> {
                let mut bytes_to_write_to = [0u8; size_of::<$int_type>()];
                let Some((bytes, new_from)) = from.split_at_checked(bytes_to_write_to.len()) else {
                    return Err(PacketReadError::NotEnoughData)
                };
                *from = new_from;
                bytes_to_write_to.copy_from_slice(bytes);
                Ok(Self::from_le_bytes(bytes_to_write_to))
            }
        }
    };
}

packet_io_for_int!(u16);
packet_io_for_int!(u32);
packet_io_for_int!(i16);
packet_io_for_int!(i32);

impl PacketIO<'_> for u8 {
    fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_> {
        core::iter::once(PacketWriteCommand::WriteByte { byte: *self }).collect()
    }
    fn read_all(from: &mut &[u8]) -> Result<Self, PacketReadError> {
        let Some((&[byte], new_from)) = from.split_at_checked(1) else {
            return Err(PacketReadError::NotEnoughData)
        };
        *from = new_from;
        Ok(byte)
    }
}

impl Packet {
    fn get_discriminator_byte(&self) -> u8 {
        match self {
            Packet::NoOp => PacketDiscriminator::NoOp as u8,
            Packet::ResetConsole => PacketDiscriminator::ResetConsole as u8,
            Packet::LoadSaveState { .. } => PacketDiscriminator::LoadSaveState as u8,
            Packet::NextFrame { .. } => PacketDiscriminator::NextFrame as u8,
            Packet::WriteMemory { data, .. } => match data.len() {
                1 => PacketDiscriminator::WriteMemory8 as u8,
                2 => PacketDiscriminator::WriteMemory16 as u8,
                4 => PacketDiscriminator::WriteMemory32 as u8,
                _ => PacketDiscriminator::WriteMemoryVar as u8,
            },
            Packet::ChangeInput { data } => match data.len() {
                1 => PacketDiscriminator::ChangeInput8 as u8,
                2 => PacketDiscriminator::ChangeInput16 as u8,
                4 => PacketDiscriminator::ChangeInput32 as u8,
                _ => PacketDiscriminator::ChangeInputVar as u8,
            },
            Packet::ChangeSpeed { .. } => PacketDiscriminator::ChangeSpeed as u8,
            Packet::Bookmark { .. } => PacketDiscriminator::Bookmark as u8,
            Packet::Keyframe { .. } => PacketDiscriminator::Keyframe as u8,
            Packet::DeltaKeyframe { .. } => PacketDiscriminator::DeltaKeyframe as u8,
            Packet::CompressedBlob { .. } => PacketDiscriminator::CompressedBlob as u8,
        }
    }
}

impl PacketIO<'_> for Packet {
    fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_> {
        let mut commands = PacketInstructionsVec::new();
        commands.push(PacketWriteCommand::WriteByte { byte: self.get_discriminator_byte() });

        // we can write the payload here
        match self {
            Packet::NoOp | Packet::ResetConsole => (),
            
            Packet::NextFrame { timestamp_delta } => {
                commands.extend(timestamp_delta.write_packet_instructions());
            },
            
            Packet::ChangeInput { data } => {
                match data.len() {
                    1 | 2 | 4 => {
                        commands.push(PacketWriteCommand::WriteVec { bytes: data.clone() });
                    },
                    _ => {
                        commands.extend(data.write_packet_instructions());
                    }
                }
            }
            
            Packet::WriteMemory { address, data } => {
                commands.extend(address.write_packet_instructions());
                match data.len() {
                    1 | 2 | 4 => {
                        commands.push(PacketWriteCommand::WriteVec { bytes: data.clone() });
                    },
                    _ => {
                        commands.extend(data.write_packet_instructions());
                    }
                }
            }

            Packet::CompressedBlob {
                keyframes,
                bookmarks,
                compressed_data,
                uncompressed_size,
                timestamp_start,
                timestamp_end,
                elapsed_frames_start,
                elapsed_frames_end
            } => {
                commands.extend(keyframes.write_packet_instructions());
                commands.extend(bookmarks.write_packet_instructions());
                commands.extend(compressed_data.write_packet_instructions());
                commands.extend(uncompressed_size.write_packet_instructions());
                commands.extend(timestamp_start.write_packet_instructions());
                commands.extend(timestamp_end.write_packet_instructions());
                commands.extend(elapsed_frames_start.write_packet_instructions());
                commands.extend(elapsed_frames_end.write_packet_instructions());
            }

            Packet::Keyframe { state, metadata } => {
                commands.extend(metadata.write_packet_instructions());
                commands.extend(state.write_packet_instructions());
            },

            Packet::DeltaKeyframe { diff, metadata } => {
                commands.extend(metadata.write_packet_instructions());
                commands.extend(diff.write_packet_instructions());
            },

            Packet::Bookmark { metadata } => {
                commands.extend(metadata.write_packet_instructions());
            },

            Packet::ChangeSpeed { speed } => {
                commands.extend(speed.write_packet_instructions());
            },

            Packet::LoadSaveState { state } => {
                (commands).extend(state.write_packet_instructions())
            }
        }

        commands
    }
    fn read_all(from: &mut &[u8]) -> Result<Self, PacketReadError> {
        let discriminator_byte = u8::read_all(from)?;

        if discriminator_byte == PacketDiscriminator::NoOp {
            return Ok(Packet::NoOp)
        }
        else if discriminator_byte == PacketDiscriminator::ResetConsole {
            return Ok(Packet::ResetConsole)
        }

        let Ok(t) = PacketDiscriminator::try_from_primitive(discriminator_byte) else {
            return Err(PacketReadError::ParseFail { explanation: Cow::Owned(alloc::format!("Unknown packet discriminator 0x{discriminator_byte:08X}")) })
        };

        macro_rules! change_input {
            ($t:ty) => {
                Ok(Packet::ChangeInput { data: {
                    let mut v = TinyVec::new();
                    v.extend_from_slice(<$t>::read_all(from)?.to_le_bytes().as_slice());
                    v
                } })
            };
        }

        macro_rules! write_memory {
            ($t:ty) => {
                Ok(Packet::WriteMemory {
                    address: UnsignedInteger::read_all(from)?,
                    data: {
                        let mut v = TinyVec::new();
                        v.extend_from_slice(<$t>::read_all(from)?.to_le_bytes().as_slice());
                        v
                    }
                })
            };
        }

        match t {
            PacketDiscriminator::NoOp | PacketDiscriminator::ResetConsole => unreachable!("{t:?} should have already been handled"),
            PacketDiscriminator::NextFrame => Ok(Packet::NextFrame { timestamp_delta: TimestampMillis::read_all(from)? }),
            PacketDiscriminator::LoadSaveState => Ok(Packet::LoadSaveState { state: ByteVec::read_all(from)? }),
            PacketDiscriminator::ChangeInput8 => change_input!(u8),
            PacketDiscriminator::ChangeInput16 => change_input!(u16),
            PacketDiscriminator::ChangeInput32 => change_input!(u32),
            PacketDiscriminator::ChangeInputVar => Ok(Packet::ChangeInput { data: ByteVec::read_all(from)? }),
            PacketDiscriminator::WriteMemory8 => write_memory!(u8),
            PacketDiscriminator::WriteMemory16 => write_memory!(u16),
            PacketDiscriminator::WriteMemory32 => write_memory!(u32),
            PacketDiscriminator::WriteMemoryVar => Ok(Packet::WriteMemory { address: UnsignedInteger::read_all(from)?, data: ByteVec::read_all(from)? }),
            PacketDiscriminator::Keyframe => Ok(Packet::Keyframe { metadata: KeyframeMetadata::read_all(from)?, state: ByteVec::read_all(from)? }),
            PacketDiscriminator::DeltaKeyframe => Ok(Packet::DeltaKeyframe { metadata: KeyframeMetadata::read_all(from)?, diff: Vec::read_all(from)? }),
            PacketDiscriminator::Bookmark => Ok(Packet::Bookmark { metadata: BookmarkMetadata::read_all(from)? }),
            PacketDiscriminator::ChangeSpeed => Ok(Packet::ChangeSpeed { speed: Speed::read_all(from)? }),
            PacketDiscriminator::CompressedBlob => Ok(Packet::CompressedBlob {
                keyframes: Vec::read_all(from)?,
                bookmarks: Vec::read_all(from)?,
                compressed_data: ByteVec::read_all(from)?,
                uncompressed_size: UnsignedInteger::read_all(from)?,
                timestamp_start: UnsignedInteger::read_all(from)?,
                timestamp_end: UnsignedInteger::read_all(from)?,
                elapsed_frames_start: UnsignedInteger::read_all(from)?,
                elapsed_frames_end: UnsignedInteger::read_all(from)?
            }),
        }
    }
}
impl PacketIO<'_> for KeyframeMetadata {
    fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_>{
        let mut write_commands = PacketInstructionsVec::new();
        write_commands.extend(self.input.write_packet_instructions());
        write_commands.extend(self.speed.write_packet_instructions());
        write_commands.extend(self.elapsed_frames.write_packet_instructions());
        write_commands.extend(self.elapsed_millis.write_packet_instructions());
        write_commands
    }

    fn read_all(from: &mut &[u8]) -> Result<Self, PacketReadError> {
        Ok(Self {
            input: InputBuffer::read_all(from)?,
            speed: Speed::read_all(from)?,
            elapsed_frames: UnsignedInteger::read_all(from)?,
            elapsed_millis: UnsignedInteger::read_all(from)?,
        })
    }
}
impl PacketIO<'_> for Speed {
    fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_> {
        self.speed_over_256.write_packet_instructions()
    }

    fn read_all(from: &mut &[u8]) -> Result<Self, PacketReadError> {
        Ok(Self { speed_over_256: NonZeroU16::read_all(from)? })
    }
}

impl PacketIO<'_> for NonZeroU16 {
    fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_> {
        static_packet_write_array_references(self.get().write_packet_instructions())
    }

    fn read_all(from: &mut &[u8]) -> Result<Self, PacketReadError> {
        Self::new(u16::read_all(from)?).ok_or_else(|| PacketReadError::ParseFail { explanation: Cow::Borrowed("read a zero u16 when NonZeroU16 was expected") })
    }
}

impl PacketIO<'_> for BookmarkMetadata {
    fn write_packet_instructions(&'_ self) -> PacketInstructionsVec<'_> {
        let mut instructions = PacketInstructionsVec::new();
        instructions.extend(self.name.write_packet_instructions());
        instructions.extend(self.elapsed_frames.write_packet_instructions());
        instructions.extend(self.elapsed_millis.write_packet_instructions());
        instructions
    }

    fn read_all(from: &mut &[u8]) -> Result<Self, PacketReadError> {
        Ok(Self {
            name: String::read_all(from)?,
            elapsed_frames: UnsignedInteger::read_all(from)?,
            elapsed_millis: TimestampMillis::read_all(from)?,
        })
    }
}

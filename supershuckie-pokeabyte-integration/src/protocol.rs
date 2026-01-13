use std::borrow::Cow;
use byteorder::{ByteOrder, LittleEndian};
use num_enum::TryFromPrimitive;
use tinyvec::ArrayVec;
use crate::PokeAByteError;

#[cfg(target_os = "macos")]
use crate::shared_memory::MACOS_MAX_MMAP_MEMORY_LENGTH;

const PROTOCOL_VERSION: u8 = 1;

#[derive(Copy, Clone, PartialEq, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum Instruction {
    NoOp = 0,
    Ping = 1,
    Setup = 2,
    Write = 3,
    Freeze = 4,
    Unfreeze = 5,
    Close = 0xFF
}

pub const METADATA_HEADER_SIZE: usize = 8;

#[derive(Copy, Clone, Debug)]
pub struct MetadataHeader {
    #[expect(unused)]
    pub protocol_version: u8,
    pub instruction: Instruction,
    pub is_response: bool
}

impl MetadataHeader {
    pub const fn new_response(instruction: Instruction) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            instruction,
            is_response: true
        }
    }

    pub const fn into_bytes(self) -> [u8; METADATA_HEADER_SIZE] {
        [PROTOCOL_VERSION, 0, 0, 0, self.instruction as u8, self.is_response as u8, 0, 0]
    }

    pub fn from_client_bytes(bytes: [u8; METADATA_HEADER_SIZE]) -> Result<Self, PokeAByteError> {
        let protocol_byte = bytes[0];
        if protocol_byte != PROTOCOL_VERSION {
            return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Owned(format!("Unknown protocol {protocol_byte} (expected {PROTOCOL_VERSION})")) })
        }

        let is_response = bytes[5];
        if is_response != 0 {
            return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Owned(format!("Bad IsResponse value {is_response}")) })
        }

        let instruction = bytes[4];
        let instruction = Instruction::try_from(instruction)
            .map_err(|_| PokeAByteError::BadPacketFromClient { explanation: Cow::Owned(format!("Bad Instruction {instruction}")) })?;

        Ok(Self {
            is_response: false,
            protocol_version: protocol_byte,
            instruction
        })
    }
}

const READ_BLOCK_SIZE: usize = 0xC;
pub const MAX_NUMBER_OF_READ_BLOCKS: usize = 128;

pub enum PokeAByteProtocolRequestPacket<'a> {
    NoOp,
    Ping,
    Setup {
        frame_skip: Option<u32>,
        blocks: ArrayVec<[PokeAByteProtocolRequestReadBlock; MAX_NUMBER_OF_READ_BLOCKS]>
    },
    Write {
        address: u64,
        data: &'a [u8]
    },
    Freeze {
        address: u64,
        data: &'a [u8]
    },
    Unfreeze {
        address: u64
    },
    Close,
}

#[derive(Default, Clone, PartialEq, Debug)]
pub struct PokeAByteProtocolRequestReadBlock {
    pub range: core::ops::Range<usize>,
    pub game_address: u32
}

impl<'a> PokeAByteProtocolRequestPacket<'a> {
    pub fn parse_bytes(bytes: &'a [u8]) -> Result<Self, PokeAByteError> {
        let Some(header) = bytes.get(..METADATA_HEADER_SIZE) else {
            return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("too small to be header") })
        };
        let header_bytes: [u8; METADATA_HEADER_SIZE] = header.try_into().unwrap();
        let header = MetadataHeader::from_client_bytes(header_bytes)?;

        match header.instruction {
            Instruction::NoOp => Ok(Self::NoOp),
            Instruction::Ping => Ok(Self::Ping),
            Instruction::Setup => {
                let Some(_setup_data) = bytes.get(..0x20 + READ_BLOCK_SIZE * MAX_NUMBER_OF_READ_BLOCKS) else {
                    return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("too small to be setup header") })
                };
                let block_count = LittleEndian::read_u32(&bytes[8..]) as usize;
                if block_count > MAX_NUMBER_OF_READ_BLOCKS {
                    return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("too many read blocks") })
                }

                let frame_skip = u32::try_from(LittleEndian::read_i32(&bytes[12..])).ok();
                let blocks = (&bytes[32..])
                    .chunks_exact(0xC)
                    .take(block_count);

                let mut blocks_into = ArrayVec::new();

                for i in blocks {
                    let memory_map_file_offset: usize = LittleEndian::read_u32(&i[0..]) as usize;
                    let game_address = LittleEndian::read_u32(&i[4..]);
                    let length: usize = LittleEndian::read_u32(&i[8..]) as usize;

                    u32::try_from(length)
                        .ok()
                        .and_then(|i| i.checked_add(game_address))
                        .ok_or_else(|| PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("length+address overflows") })?;

                    let end = length.checked_add(memory_map_file_offset)
                        .ok_or_else(|| PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("length+offset overflows") })?;

                    // On macOS, error out as shm_open fails above 4 MiB.
                    #[cfg(target_os = "macos")]
                    if end > MACOS_MAX_MMAP_MEMORY_LENGTH {
                        return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("maximum shared memory size of 4 MiB exceeded") });
                    }

                    let range = memory_map_file_offset .. end;

                    blocks_into.push(PokeAByteProtocolRequestReadBlock {
                        game_address, range
                    })
                }

                Ok(Self::Setup {
                    blocks: blocks_into,
                    frame_skip
                })
            },
            Instruction::Write => {
                let Some(_params) = bytes.get(0x8..0x14) else {
                    return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("too small to be write header") })
                };

                let address = LittleEndian::read_u64(&bytes[0x8..]);
                let length: usize = LittleEndian::read_u32(&bytes[0x10..]) as usize;

                let Some(data) = bytes.get(0x20..) else {
                    return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("failed to read data: no bytes after length") })
                };

                let Some(data) = data.get(..length) else {
                    return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("failed to read data: insufficient length") })
                };

                Ok(Self::Write { data, address })
            },
            Instruction::Freeze => {
                let Some(_params) = bytes.get(0x8..0x14) else {
                    return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("too small to be freeze header") })
                };

                let address = LittleEndian::read_u64(&bytes[0x8..]);
                let length: usize = LittleEndian::read_u32(&bytes[0x10..]) as usize;

                let Some(data) = bytes.get(0x20..) else {
                    return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("failed to read data: no bytes after length") })
                };

                let Some(data) = data.get(..length) else {
                    return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("failed to read data: insufficient length") })
                };

                Ok(Self::Freeze { address, data } )
            },
            Instruction::Unfreeze => {
                let Some(_params) = bytes.get(0x8..0x10) else {
                    return Err(PokeAByteError::BadPacketFromClient { explanation: Cow::Borrowed("too small to be unfreeze header") })
                };

                let address = LittleEndian::read_u64(&bytes[0x8..]);
                Ok(Self::Unfreeze { address })
            }
            Instruction::Close => Ok(Self::Close)
        }
    }
}

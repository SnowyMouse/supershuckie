use crate::util::{reinterpret_ref, MaybeEnum};
use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use core::ffi::CStr;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use crate::{TimestampMillis, UnsignedInteger};

/// Signature start (all replay headers must start with this)
pub const SIGNATURE_START: [u8; 4] = 0x4E49444Fu32.to_be_bytes();

/// Signature end (all replay headers must end with this)
pub const SIGNATURE_END: [u8; 4] = 0x52494E41u32.to_be_bytes();

/// Replay format version (minimum supported)
pub const REPLAY_VERSION_MINIMUM_SUPPORTED: u32 = 2;

/// Replay format version (written version)
pub const REPLAY_VERSION: u32 = 3;

// FIXME: How do we handle resuming?

/// Blake3 checksum
pub type ReplayHeaderBlake3Hash = [u8; 32];

/// Convert the hash to an uppercase ASCII string (uppercase) for displaying.
pub fn blake3_hash_to_ascii(hash: ReplayHeaderBlake3Hash) -> String {
    let mut ascii = String::with_capacity(64);

    for b in hash {
        let high = b >> 4;
        let low = b & 0xF;

        fn get_char(b: u8) -> char {
            if b <= 0x9 {
                (b'0' + b) as char
            }
            else {
                (b'A' + (b - 0xA)) as char
            }
        }

        ascii.push(get_char(high));
        ascii.push(get_char(low));
    }

    ascii
}

/// UTF-8 null-terminated 255 byte length string
pub type ReplayHeaderString = [u8; 256];

/// Raw replay header, mapping directly to the actual file.
#[derive(Copy, Clone, PartialEq, Debug)]
#[repr(C, packed(1))]
pub struct ReplayHeaderRaw {
    /// 0x000 - signature (must equal [`SIGNATURE_START`])
    pub signature_start: [u8; 4],

    /// 0x004 - replay format version
    pub replay_version: u32,

    /// 0x008 - type of the console
    pub console_type: MaybeEnum<ReplayConsoleType>,

    /// 0x00C - crop_start_* and crop_timer_offset are valid
    pub crop_start: bool,

    /// 0x00D - crop_end_* are valid
    pub crop_end: bool,

    /// 0x00C - padding
    pub _padding_0: [u8; 2],

    /// 0x010 name of the emulator core, including version
    pub emulator_core_name: ReplayHeaderString,

    /// 0x110 patch data length
    pub patch_data_length: u64,

    /// 0x118 - patch format of the ROM
    pub patch_format: MaybeEnum<ReplayPatchFormat>,

    /// 0x11C - padding
    pub _padding_1: [u8; 4],

    /// 0x120 - blake3 hash of the unpatched ROM
    pub patch_target_checksum: ReplayHeaderBlake3Hash,

    /// 0x140 internal name of the ROM
    pub rom_name: ReplayHeaderString,

    /// 0x240 - filename of the ROM
    pub rom_filename: ReplayHeaderString,

    /// 0x340 - blake3 hash of the ROM (after all patches are applied, if any)
    pub rom_checksum: ReplayHeaderBlake3Hash,

    /// 0x360 - blake3 hash of the BIOS
    pub bios_checksum: ReplayHeaderBlake3Hash,

    /// 0x380 - crop range begin (frame index)
    pub crop_start_frame: UnsignedInteger,

    /// 0x388 - crop range begin (milliseconds)
    pub crop_start_millis: TimestampMillis,

    /// 0x390 - crop range end (frame index)
    pub crop_end_frame: UnsignedInteger,

    /// 0x398 - crop range end (milliseconds)
    pub crop_end_millis: TimestampMillis,

    /// 0x390 - crop timer offset
    pub crop_timer_offset: TimestampMillis,

    /// 0x398 - padding
    pub _padding_2: [u8; 0x458 - 4],

    /// 0x7FC - signature (must equal [`SIGNATURE_END`])
    pub signature_end: [u8; 4],
}

/// Exactly enough bytes to hold [`ReplayHeaderRaw`] in binary form.
pub type ReplayHeaderBytes = [u8; 2048];

// Ensure that we can safely transmute between the two.
const _: () = assert!(size_of::<ReplayHeaderRaw>() == size_of::<ReplayHeaderBytes>());

/// Metadata to generate a replay file.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ReplayFileMetadata {
    /// Console type
    pub console_type: ReplayConsoleType,

    /// Internal ROM name (max length is 255 bytes)
    pub rom_name: String,

    /// Internal ROM filename (max length is 255 bytes)
    pub rom_filename: String,

    /// blake3 hash of the ROM (after patch)
    pub rom_checksum: ReplayHeaderBlake3Hash,

    /// blake3 hash of the BIOS
    pub bios_checksum: ReplayHeaderBlake3Hash,

    /// Name of the emulator core, including version (max length is 255 bytes)
    ///
    /// If this does not match exactly, it is recommended to warn before proceeding.
    pub emulator_core_name: String,

    /// Patch format to use
    pub patch_format: ReplayPatchFormat,

    /// blake3 hash of the target ROM (before patch)
    pub patch_target_checksum: ReplayHeaderBlake3Hash,

    /// crop start
    pub crop_start: Option<(UnsignedInteger, TimestampMillis)>,

    /// crop end
    pub crop_end: Option<(UnsignedInteger, TimestampMillis)>,

    /// timer offset
    pub timer_offset: Option<TimestampMillis>
}

impl ReplayHeaderRaw {
    /// Reinterpret the header as bytes.
    pub fn as_bytes(&self) -> &ReplayHeaderBytes {
        // SAFETY: ReplayHeaderRaw is safe to transmute to/from ReplayHeaderBytes (and intended to be done so)
        unsafe { reinterpret_ref(self) }
    }
    /// Reinterpret bytes as a raw header.
    pub fn from_bytes(bytes: &ReplayHeaderBytes) -> &ReplayHeaderRaw {
        // SAFETY: ReplayHeaderRaw is safe to transmute to/from ReplayHeaderBytes (and intended to be done so)
        //
        // Of course, there is no guarantee that we're going to get anything valid out of this,
        // but that's not UB.
        unsafe { reinterpret_ref(bytes) }
    }
    /// Parse the header.
    /// 
    /// Returns an error with a description if it is invalid.
    pub fn parse(&self) -> Result<ReplayFileMetadata, String> {
        let signature_start = self.signature_start;
        let signature_end = self.signature_end;
        let replay_version = self.replay_version;

        if signature_start != SIGNATURE_START {
            return Err(format!("Unrecognized signature_start {signature_start:X?}"));
        }
        if signature_end != SIGNATURE_END {
            return Err(format!("Unrecognized signature_end {signature_end:X?}"));
        }
        if self.replay_version < REPLAY_VERSION_MINIMUM_SUPPORTED || self.replay_version > REPLAY_VERSION {
            return Err(format!("Unrecognized replay format version {replay_version} (not in {REPLAY_VERSION_MINIMUM_SUPPORTED}..={REPLAY_VERSION})"));
        }

        fn parse_string_buffer(what: &ReplayHeaderString, name: &str) -> Result<String, String> {
            CStr::from_bytes_until_nul(what.as_slice())
                .map_err(|_| format!("{name} length exceeds 255 bytes"))?
                .to_str()
                .map_err(|_| format!("{name} is non-UTF-8 (cannot parse)"))
                .map(|s| s.to_owned())
        }

        Ok(ReplayFileMetadata {
            console_type: self.console_type.get().map_err(|i| format!("Unrecognized console_type 0x{i:08X}"))?,
            patch_format: self.patch_format.get().map_err(|i| format!("Unrecognized patch_format 0x{i:08X}"))?,

            bios_checksum: self.bios_checksum,
            rom_checksum: self.rom_checksum,
            patch_target_checksum: self.patch_target_checksum,

            rom_name: parse_string_buffer(&self.rom_name, "rom_name")?,
            rom_filename: parse_string_buffer(&self.rom_filename, "rom_filename")?,
            emulator_core_name: parse_string_buffer(&self.emulator_core_name, "emulator_core_name")?,

            crop_start: self.crop_start.then_some((self.crop_start_frame, self.crop_start_millis)),
            crop_end: self.crop_end.then_some((self.crop_end_frame, self.crop_end_millis)),
            timer_offset: self.crop_start.then_some(self.crop_timer_offset)
        })
    }
}

impl ReplayFileMetadata {
    /// Convert the parsed header into a raw header.
    pub fn as_raw_header(&self) -> Result<ReplayHeaderRaw, String> {
        fn into_str_bytes(what: &str, name: &'static str) -> Result<ReplayHeaderString, String> {
            let mut result = [0u8; 256];
            let limit = result.len() - 1;
            let result_minus_null_termination = &mut result[0..limit];
            let what_bytes = what.as_bytes();
            
            result_minus_null_termination.get_mut(0..what_bytes.len())
                .ok_or_else(|| format!("{name} exceeds {limit} bytes"))?
                .copy_from_slice(what_bytes);

            Ok(result)
        }

        Ok(ReplayHeaderRaw {
            signature_start: SIGNATURE_START,
            replay_version: REPLAY_VERSION,
            console_type: MaybeEnum::new(self.console_type),
            rom_name: into_str_bytes(&self.rom_name, "rom_name")?,
            rom_filename: into_str_bytes(&self.rom_filename, "rom_filename")?,
            rom_checksum: self.rom_checksum,
            bios_checksum: self.bios_checksum,
            emulator_core_name: into_str_bytes(&self.emulator_core_name, "emulator_core_name")?,
            patch_format: MaybeEnum::new(self.patch_format),
            patch_data_length: 0,
            patch_target_checksum: self.patch_target_checksum,
            signature_end: SIGNATURE_END,

            crop_start_frame: self.crop_start.map(|i| i.0).unwrap_or(0),
            crop_end_frame: self.crop_end.map(|i| i.0).unwrap_or(0),
            crop_start_millis: self.crop_start.map(|i| i.1).unwrap_or(0.into()),
            crop_end_millis: self.crop_end.map(|i| i.1).unwrap_or(0.into()),
            crop_timer_offset: self.timer_offset.unwrap_or(0.into()),

            crop_start: self.crop_start.is_some(),
            crop_end: self.crop_end.is_some(),

            _padding_0: [0u8; _],
            _padding_1: [0u8; _],
            _padding_2: [0u8; _]
        })
    }
}

/// Console type to use for replays.
#[derive(Copy, Clone, PartialEq, Debug, TryFromPrimitive, Default, IntoPrimitive)]
#[repr(u32)]
pub enum ReplayConsoleType {
    /// This is valid, but the user should probably not accept such a replay.
    #[default]
    Unknown,

    /// Game Boy
    GameBoy,

    /// Super Game Boy 2
    SuperGameBoy2,

    /// Game Boy Color
    GameBoyColor,

    /// Game Boy Advance
    GameBoyAdvance,

    /// Nintendo DS
    NintendoDS
}

impl ReplayConsoleType {
    /// Get the console name in human readable format
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::GameBoy => "Game Boy",
            Self::SuperGameBoy2 => "Super Game Boy 2",
            Self::GameBoyColor => "Game Boy Color",
            Self::GameBoyAdvance => "Game Boy Advance",
            Self::NintendoDS => "Nintendo DS"
        }
    }
}

impl core::fmt::Display for ReplayConsoleType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// Determines what patch format to use.
#[derive(Copy, Clone, PartialEq, Debug, TryFromPrimitive, Default, IntoPrimitive)]
#[repr(u32)]
pub enum ReplayPatchFormat {
    /// The ROM is unpatched
    #[default]
    Unpatched,

    /// The patch is in BPS format
    BPS
}

use crate::util::UTF8CString;
use crate::SETTINGS_FILE;
use num_enum::TryFromPrimitive;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::CStr;
use std::fs;
use std::fs::File;
use std::hint::unreachable_unchecked;
use std::io::{Read, Seek, SeekFrom};
use std::num::{NonZeroIsize, NonZeroU32, NonZeroU64, NonZeroU8, NonZeroUsize};
use std::path::Path;
use supershuckie_core::emulator::Input;
use supershuckie_replay_recorder::replay_file::record::ReplayFileRecorderSettings;

pub(crate) fn try_to_init_data_dir_and_get_settings(data_dir: &Path, config_dir: &Path) -> Result<Settings, String> {
    if !data_dir.exists() {
        fs::create_dir(&data_dir).map_err(|e| format!("Failed to create the data_dir: {e}"))?;
    }
    if !config_dir.exists() {
        fs::create_dir(&config_dir).map_err(|e| format!("Failed to create the config_dir: {e}"))?;
    }

    let settings_json = config_dir.join(SETTINGS_FILE);
    let mut settings_file = File::options()
        .write(true)
        .read(true)
        .create(true)
        .open(settings_json)
        .map_err(|e| format!("Failed to open the config file for write access: {e}"))?;

    settings_file.seek(SeekFrom::Start(0)).map_err(|e| format!("Failed to seek the config file: {e}"))?;

    let mut settings_str = String::new();
    settings_file.read_to_string(&mut settings_str).map_err(|e| format!("Failed to read the config file: {e}"))?;

    if settings_str.trim().is_empty() {
        settings_str = "{}".to_owned();
    }

    let settings: Settings = serde_json::from_str::<Settings>(&settings_str).map_err(|e| format!("Failed to parse the config file: {e}"))?;
    Ok(settings)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "ReplaySettings::default")]
    pub replay: ReplaySettings,

    #[serde(default = "RecentROMs::default")]
    pub recent_roms: RecentROMs,

    #[serde(default = "EmulationSettings::default")]
    pub emulation: EmulationSettings,

    #[serde(default = "GameBoySettings::default")]
    pub game_boy_settings: GameBoySettings,

    #[serde(default = "NintendoDSSettings::default")]
    pub nintendo_ds_settings: NintendoDSSettings,

    #[serde(default = "Controls::default")]
    pub controls: Controls,

    #[serde(default = "BTreeMap::default")]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub rom_config: BTreeMap<String, ROMConfig>,

    #[serde(default = "PokeAByteSettings::default")]
    pub pokeabyte: PokeAByteSettings,

    #[serde(default = "BTreeMap::default")]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub custom: BTreeMap<String, UTF8CString>
}

impl Settings {
    pub(crate) fn get_rom_config_or_default(&mut self, rom: &str) -> &mut ROMConfig {
        if !self.rom_config.contains_key(rom) {
            self.rom_config.insert(rom.to_owned(), ROMConfig::default());
        }
        self.rom_config.get_mut(rom).expect("we just added the rom??")
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct RecentROMs {
    pub max_recent_roms: NonZeroIsize,
    pub recent_roms: Vec<UTF8CString>
}

impl Default for RecentROMs {
    fn default() -> Self {
        Self {
            max_recent_roms: unsafe { NonZeroIsize::new_unchecked(20) },
            recent_roms: Vec::new()
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct ReplaySettings {
    #[serde(default = "ReplaySettings::MAX_RECORDING_BLOB_SIZE_MB")]
    pub max_recording_blob_size_mb: NonZeroU32,

    #[serde(default = "ReplaySettings::AUTO_DECOMPRESS_REPLAYS_UPFRONT")]
    pub auto_decompress_replays_upfront: bool,

    #[serde(default = "ReplaySettings::DEFAULT_MAX_ZSTD_COMPRESSION_LEVEL")]
    pub zstd_compression_level: i32,

    #[serde(default = "ReplaySettings::DEFAULT_FRAMES_PER_KEYFRAME")]
    pub frames_per_keyframe: NonZeroU64,

    #[serde(default = "ReplaySettings::AUTO_STOP_PLAYBACK_ON_INPUT")]
    pub auto_stop_playback_on_input: bool,

    #[serde(default = "ReplaySettings::AUTO_UNPAUSE_ON_INPUT")]
    pub auto_unpause_on_input: bool,

    #[serde(default = "ReplaySettings::AUTO_PAUSE_ON_RECORD")]
    pub auto_pause_on_record: bool,
}

impl Default for ReplaySettings {
    fn default() -> Self {
        Self {
            max_recording_blob_size_mb: Self::MAX_RECORDING_BLOB_SIZE_MB(),
            auto_decompress_replays_upfront: Self::AUTO_DECOMPRESS_REPLAYS_UPFRONT(),
            zstd_compression_level: Self::DEFAULT_MAX_ZSTD_COMPRESSION_LEVEL(),
            frames_per_keyframe: Self::DEFAULT_FRAMES_PER_KEYFRAME(),
            auto_stop_playback_on_input: Self::AUTO_STOP_PLAYBACK_ON_INPUT(),
            auto_unpause_on_input: Self::AUTO_UNPAUSE_ON_INPUT(),
            auto_pause_on_record: Self::AUTO_PAUSE_ON_RECORD(),
        }
    }
}

impl ReplaySettings {
    const MAX_RECORDING_BLOB_SIZE_MB: fn() -> NonZeroU32 = || unsafe { NonZeroU32::new_unchecked(
        (ReplayFileRecorderSettings::default().minimum_uncompressed_bytes_per_blob / 1024 / 1024) as u32
    ) };
    const AUTO_DECOMPRESS_REPLAYS_UPFRONT: fn() -> bool = || false;
    const DEFAULT_MAX_ZSTD_COMPRESSION_LEVEL: fn() -> i32 = || ReplayFileRecorderSettings::default().compression_level;
    const DEFAULT_FRAMES_PER_KEYFRAME: fn() -> NonZeroU64 = || unsafe { NonZeroU64::new_unchecked(120) };
    const AUTO_STOP_PLAYBACK_ON_INPUT: fn() -> bool = || false;
    const AUTO_UNPAUSE_ON_INPUT: fn() -> bool = || false;
    const AUTO_PAUSE_ON_RECORD: fn() -> bool = || false;
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ROMConfig {
    pub save_name: UTF8CString
}

impl Default for ROMConfig {
    fn default() -> Self {
        Self {
            save_name: "default".into()
        }
    }
}

#[derive(Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PokeAByteSettings {
    #[serde(default = "bool::default")]
    pub enabled: bool
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct EmulationSettings {
    #[serde(default = "EmulationSettings::DEFAULT_BASE_SPEED_MULTIPLIER")]
    pub base_speed_multiplier: f64,

    #[serde(default = "EmulationSettings::DEFAULT_TURBO_SPEED_MULTIPLIER")]
    pub turbo_speed_multiplier: f64,

    #[serde(default = "EmulationSettings::DEFAULT_MAX_SAVE_STATE_HISTORY")]
    pub max_save_state_history: NonZeroUsize
}

impl EmulationSettings {
    const DEFAULT_BASE_SPEED_MULTIPLIER: fn() -> f64 = || 1.0;
    const DEFAULT_TURBO_SPEED_MULTIPLIER: fn() -> f64 = || 2.0;
    const DEFAULT_MAX_SAVE_STATE_HISTORY: fn() -> NonZeroUsize = || unsafe { NonZeroUsize::new_unchecked(100) };
}

impl Default for EmulationSettings {
    fn default() -> Self {
        Self {
            base_speed_multiplier: EmulationSettings::DEFAULT_BASE_SPEED_MULTIPLIER(),
            turbo_speed_multiplier: EmulationSettings::DEFAULT_TURBO_SPEED_MULTIPLIER(),
            max_save_state_history: EmulationSettings::DEFAULT_MAX_SAVE_STATE_HISTORY()
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct GameBoySettings {
    #[serde(default = "GameBoyMode::default")]
    pub gbc_mode: GameBoyMode,

    #[serde(default = "bool::default")]
    pub sgb: bool,

    #[serde(default = "GameBoySettings::DEFAULT_VIDEO_SCALE")]
    pub video_scale: NonZeroU8
}

impl GameBoySettings {
    const DEFAULT_VIDEO_SCALE: fn() -> NonZeroU8 = || unsafe { NonZeroU8::new_unchecked(4) };
}

impl Default for GameBoySettings {
    fn default() -> Self {
        Self {
            gbc_mode: GameBoyMode::default(),
            sgb: false,
            video_scale: Self::DEFAULT_VIDEO_SCALE()
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct NintendoDSSettings {
    #[serde(default = "NintendoDSDate::default")]
    pub date: NintendoDSDate,

    #[serde(default = "bool::default")]
    pub jit: bool,

    #[serde(default = "NintendoDSSettings::DEFAULT_VIDEO_SCALE")]
    pub video_scale: NonZeroU8
}

impl NintendoDSSettings {
    const DEFAULT_VIDEO_SCALE: fn() -> NonZeroU8 = || unsafe { NonZeroU8::new_unchecked(2) };
}

impl Default for NintendoDSSettings {
    fn default() -> Self {
        Self {
            date: NintendoDSDate::default(),
            jit: false,
            video_scale: Self::DEFAULT_VIDEO_SCALE()
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[repr(C)]
pub struct NintendoDSDate {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8
}

impl NintendoDSDate {
    /// Clean up ranges.
    pub fn get_cleaned(self) -> Self {
        let mut cleaned = Self {
            year: self.year.clamp(2000, 2099),
            month: self.month.clamp(1, 12),
            day: self.day.clamp(1, 31),
            hour: self.hour.clamp(0, 23),
            minute: self.minute.clamp(0, 59),
            second: self.second.clamp(0, 59),
        };

        let max_days_per_month = |month: u8| match month {
            2 => 29,
            4|6|9|11 => 30,
            _ => 31
        };

        cleaned.day = cleaned.day.min(max_days_per_month(cleaned.month));

        // handle leap years
        //
        // (the % 100/400 is technically redundant since the range is 2000-2099, and 2000 was a leap year; idc)
        if cleaned.month == 2 && cleaned.year % 4 == 0 && (cleaned.year % 100 != 0 || cleaned.year % 400 == 0) {
            cleaned.day = cleaned.day.min(28);
        }

        cleaned
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, Default, TryFromPrimitive)]
#[repr(u32)]
pub enum GameBoyMode {
    /// Run all Game Boy games in Game Boy Color mode
    #[serde(rename = "GBC-always")]
    #[default]
    AlwaysGBC = 0,

    /// Run Game Boy games in Game Boy mode
    #[serde(rename = "GBC-auto")]
    GBInGBMode = 1,

    /// Run all Game Boy games in Game Boy mode, even incompatible Game Boy Color games
    #[serde(rename = "GBC-never")]
    AlwaysGB = 2
}

pub type ControlMap = BTreeMap<i32, ControlSetting>;

#[derive(Clone, Serialize, Deserialize)]
pub struct Controls {
    #[serde(default = "BTreeMap::default")]
    pub keyboard_controls: ControlMap,

    #[serde(default = "BTreeMap::default")]
    pub controller_controls: BTreeMap<String, ControllerSettings>
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            keyboard_controls: ControlMap::new(),
            controller_controls: BTreeMap::new()
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct ControllerSettings {
    #[serde(default = "BTreeMap::default")]
    pub buttons: ControlMap,

    #[serde(default = "BTreeMap::default")]
    pub axis: ControlMap,
}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ControlSetting {
    pub control: Control,
    #[serde(default = "ControlModifier::default")]
    #[serde(skip_serializing_if = "ControlModifier::is_default")]
    pub modifier: ControlModifier
}

// FIXME: Determine if we need this. If not, get rid of it!
impl ControlSetting {
    pub const fn as_u64(self) -> u64 {
        let low = self.control as u64;
        let high = self.modifier as u64;
        low | (high << 32)
    }
    pub fn from_u64(u: u64) -> Option<Self> {
        let low = u as u32;
        let high = (u >> 32) as u32;

        let control = Control::try_from(low).ok()?;
        let modifier = ControlModifier::try_from(high).ok()?;

        Some(Self { control, modifier })
    }
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Serialize, Deserialize, TryFromPrimitive)]
#[repr(u32)]
#[serde(rename_all = "snake_case")]
pub enum ControlModifier {
    #[default]
    Normal,
    Rapid,
    Toggle
}

impl ControlModifier {
    fn is_default(&self) -> bool {
        self == &ControlModifier::Normal
    }

    #[inline]
    pub const fn as_str(self) -> &'static str {
        let cstr = self.as_c_str();
        let Ok(str) = cstr.to_str() else {
            // SAFETY: Trust me bro.
            unsafe { unreachable_unchecked() }
        };
        str
    }

    pub const fn as_c_str(self) -> &'static CStr {
        match self {
            ControlModifier::Normal => c"Normal",
            ControlModifier::Rapid => c"Rapid Fire",
            ControlModifier::Toggle => c"Toggle"
        }
    }

}

#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize, TryFromPrimitive)]
#[repr(u32)]
#[serde(rename_all = "snake_case")]
pub enum Control {
    Up,
    Down,
    Left,
    Right,

    A,
    B,
    Start,
    Select,

    L,
    R,
    X,
    Y,

    Turbo,
    Reset,
    Pause
}
impl Control {
    pub const fn is_button(self) -> bool {
        match self {
            Control::A => true,
            Control::B => true,
            Control::Start => true,
            Control::Select => true,
            Control::Up => true,
            Control::Down => true,
            Control::Left => true,
            Control::Right => true,
            Control::L => true,
            Control::R => true,
            Control::X => true,
            Control::Y => true,
            Control::Turbo => false,
            Control::Reset => false,
            Control::Pause => false
        }
    }

    pub(crate) const fn set_for_input(&self, input: &mut Input, value: bool) {
        match self {
            Control::A => input.a = value,
            Control::B => input.b = value,
            Control::Start => input.start = value,
            Control::Select => input.select = value,
            Control::Up => input.d_up = value,
            Control::Down => input.d_down = value,
            Control::Left => input.d_left = value,
            Control::Right => input.d_right = value,
            Control::L => input.l = value,
            Control::R => input.r = value,
            Control::X => input.x = value,
            Control::Y => input.y = value,
            Control::Turbo => {}
            Control::Reset => {}
            Control::Pause => {}
        }
    }

    pub(crate) const fn invert_for_input(&self, input: &mut Input) {
        match self {
            Control::A => input.a = !input.a,
            Control::B => input.b = !input.b,
            Control::Start => input.start = !input.start,
            Control::Select => input.select = !input.select,
            Control::Up => input.d_up = !input.d_up,
            Control::Down => input.d_down = !input.d_down,
            Control::Left => input.d_left = !input.d_left,
            Control::Right => input.d_right = !input.d_right,
            Control::L => input.l = !input.l,
            Control::R => input.r = !input.r,
            Control::X => input.x = !input.x,
            Control::Y => input.y = !input.y,
            Control::Turbo => {}
            Control::Reset => {}
            Control::Pause => {}
        }
    }

    #[inline]
    pub const fn as_str(self) -> &'static str {
        let cstr = self.as_c_str();
        let Ok(str) = cstr.to_str() else {
            // SAFETY: Trust me bro.
            unsafe { unreachable_unchecked() }
        };
        str
    }

    pub const fn as_c_str(self) -> &'static CStr {
        match self {
            Control::A => c"A",
            Control::B => c"B",
            Control::Start => c"Start",
            Control::Select => c"Select",
            Control::Up => c"D-Up",
            Control::Down => c"D-Down",
            Control::Left => c"D-Left",
            Control::Right => c"D-Right",
            Control::L => c"L",
            Control::R => c"R",
            Control::X => c"X",
            Control::Y => c"Y",
            Control::Turbo => c"Turbo",
            Control::Reset => c"Reset console",
            Control::Pause => c"Pause"
        }
    }
}

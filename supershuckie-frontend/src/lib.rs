pub mod util;
pub mod settings;

use std::cell::OnceCell;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use crate::settings::*;
use crate::util::UTF8CString;
use std::ffi::CStr;
use std::fmt::Formatter;
use std::fs::File;
use std::hint::unreachable_unchecked;
use std::io::Write;
use std::num::{NonZeroU64, NonZeroU8};
use std::path::{absolute, Path, PathBuf};
use std::sync::Arc;
use std::io::BufWriter;
use std::time::Duration;
use num_enum::TryFromPrimitive;
use supershuckie_core::emulator::{EmulatorCore, GameBoyColor, Input, Model, PartialReplayRecordMetadata, ScreenData, NullEmulatorCore, NintendoDS, GameBoyAdvance};
use supershuckie_core::{std_timestamp_provider, ElapsedTimeStats, ReplayPlayerAttachError, Speed, SuperShuckieRapidFire, ThreadedSuperShuckieCore};
use supershuckie_frontend_webserver::{Stats, SuperShuckieServerCommand, SuperShuckieWebserver};
use supershuckie_replay_recorder::replay_file::{ReplayConsoleType, ReplayHeaderBlake3Hash, ReplayPatchFormat};
use supershuckie_replay_recorder::{blake3_hash, ByteVec, SignedInteger, TimestampMillis, UnsignedInteger};
use supershuckie_replay_recorder::replay_file::playback::ReplayFilePlayer;
use supershuckie_replay_recorder::replay_file::record::ReplayFileRecorderSettings;

const SETTINGS_FILE: &str = "settings.json";
const SAVE_STATE_EXTENSION: &str = "save_state";
const SAVE_DATA_EXTENSION: &str = "sav";
const REPLAY_EXTENSION: &str = "replay";

pub type ConnectedControllerIndex = u32;

#[derive(Copy, Clone, PartialEq, Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum SuperShuckieEmulatorType {
    GameBoy,
    GameBoySGB2,
    GameBoyColor,
    GameBoyAdvance,
    NintendoDS
}

impl SuperShuckieEmulatorType {
    /// Return true if this uses a shared config with another system.
    ///
    /// Does not return true if this system owns that config.
    pub const fn uses_shared_config(self) -> bool {
        match self {
            SuperShuckieEmulatorType::GameBoy => false,
            SuperShuckieEmulatorType::GameBoySGB2 => true,
            SuperShuckieEmulatorType::GameBoyColor => true,
            SuperShuckieEmulatorType::GameBoyAdvance => false,
            SuperShuckieEmulatorType::NintendoDS => false,
        }
    }

    /// Get the human-readable name.
    #[inline]
    pub const fn name(self) -> &'static str {
        match self.name_cstr().to_str() {
            Ok(n) => n,
            Err(_) => unsafe { unreachable_unchecked() }
        }
    }

    /// Get the human-readable name as a C string.
    pub const fn name_cstr(self) -> &'static CStr {
        match self {
            SuperShuckieEmulatorType::GameBoy => c"Game Boy",
            SuperShuckieEmulatorType::GameBoySGB2 => c"Super Game Boy 2",
            SuperShuckieEmulatorType::GameBoyColor => c"Game Boy Color",
            SuperShuckieEmulatorType::GameBoyAdvance => c"Game Boy Advance",
            SuperShuckieEmulatorType::NintendoDS => c"Nintendo DS"
        }
    }
}

impl core::fmt::Display for SuperShuckieEmulatorType {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}



pub enum UserInput {
    Keyboard { keycode: i32 },
    Button { controller: ConnectedControllerIndex, button: i32 },
    Axis { controller: ConnectedControllerIndex, axis: i32 }
}

pub struct SuperShuckieFrontend {
    core: ThreadedSuperShuckieCore,
    emulator_type: Option<SuperShuckieEmulatorType>,

    callbacks: Box<dyn SuperShuckieFrontendCallbacks>,

    user_dir: PathBuf,
    config_dir: PathBuf,
    pokeabyte_error: Option<UTF8CString>,

    loaded_rom_data: Option<Vec<u8>>,

    current_input: Input,
    current_rapid_fire_input: Option<SuperShuckieRapidFire>,
    current_toggled_input: Option<Input>,
    current_save_state_history: Vec<Vec<u8>>,
    current_save_state_history_position: usize,
    current_replay: Option<UTF8CString>,

    web_server: Option<SuperShuckieWebserver>,
    external_commands_error: Option<UTF8CString>,

    connected_controllers: BTreeMap<ConnectedControllerIndex, UTF8CString>,

    rom_name: Option<Arc<UTF8CString>>,
    save_file: Option<Arc<UTF8CString>>,
    recording_replay_file: Option<ReplayFileInfo>,

    bios_override: Option<Vec<u8>>,

    last_read_elapsed_time_stats: ElapsedTimeStats,
    last_read_replay_stats: Option<LastReadReplayCropData>,

    last_replay_and_frame: Option<(UTF8CString, u32)>,

    settings: Settings
}

impl SuperShuckieFrontend {
    pub fn new<DATA: AsRef<Path>, CONF: AsRef<Path>>(data: DATA, config_dir: CONF, callbacks: Box<dyn SuperShuckieFrontendCallbacks>) -> Self {
        let data_dir = data.as_ref().to_owned();

        // FIXME: Check this
        let settings = try_to_init_data_dir_and_get_settings(
            data_dir.as_ref(),
            config_dir.as_ref(),
        ).expect("failed to init user_dir");

        let mut s = Self {
            core: ThreadedSuperShuckieCore::new(Box::new(NullEmulatorCore)),
            emulator_type: None,
            user_dir: data_dir,
            rom_name: None,
            save_file: None,
            loaded_rom_data: None,
            current_rapid_fire_input: None,
            current_toggled_input: None,
            callbacks,
            settings,
            current_input: Input::default(),
            current_save_state_history: Vec::new(),
            last_read_elapsed_time_stats: ElapsedTimeStats::default(),
            current_save_state_history_position: 0,
            recording_replay_file: None,
            pokeabyte_error: None,
            config_dir: config_dir.as_ref().to_owned(),
            web_server: None,
            external_commands_error: None,
            connected_controllers: BTreeMap::new(),
            last_read_replay_stats: None,
            bios_override: None,
            last_replay_and_frame: None,
            current_replay: None
        };

        // This is not tied to the core, so we want to immediately enable this.
        if s.settings.external_commands.enabled {
            let _ = s.set_external_commands_enabled(true);
        }

        s.unload_rom();
        s
    }

    /// Create a save state.
    ///
    /// If `name` is set, that name will be used.
    ///
    /// Returns the name of the save state if created.
    pub fn create_save_state(&mut self, name: Option<&str>) -> Result<UTF8CString, UTF8CString> {
        if !self.is_game_running() {
            return Err("Game not running".into())
        }

        let current_rom_name = self.get_current_rom_name().expect("no rom name when game is running in create_save_state");
        let save_states_dir = self.get_save_states_dir_for_rom(current_rom_name);

        let (mut file, filename, _) = self.load_file_or_make_generic(&save_states_dir, name, None, SAVE_STATE_EXTENSION)?;

        let state = self.create_save_state_now();
        file.write_all(&state)
            .map_err(|e| format!("Can't write to {filename}: {e}").into())
            .map(|_| filename.into())
    }

    /// Connect a controller.
    pub fn connect_controller(&mut self, controller_name: &str) -> ConnectedControllerIndex {
        for i in 0..=ConnectedControllerIndex::MAX {
            if self.connected_controllers.contains_key(&i) {
                continue
            }
            self.connected_controllers.insert(i, controller_name.into());
            return i;
        }

        panic!("Out of controller indices");
    }

    /// Get a list of all connected controllers.
    pub fn get_connected_controllers(&self) -> Vec<UTF8CString> {
        self.connected_controllers.iter().map(|(_,v)| v.to_owned()).collect()
    }

    /// Disconnect a controller.
    pub fn disconnect_controller(&mut self, controller: ConnectedControllerIndex) {
        self.connected_controllers.remove(&controller);
    }

    /// Get the name of the connected controller.
    pub fn name_of_controller(&self, controller: ConnectedControllerIndex) -> Option<&str> {
        self.connected_controllers.get(&controller).map(|i| i.as_str())
    }

    /// Get the name of the connected controller as a C string.
    pub fn name_of_controller_c_str(&self, controller: ConnectedControllerIndex) -> Option<&CStr> {
        self.connected_controllers.get(&controller).map(|i| i.as_c_str())
    }

    /// Mark the start of a replay.
    pub fn mark_replay_start(&mut self, timer_offset: TimestampMillis) -> Result<(), ()> {
        if let Ok(n) = self.core.mark_start(timer_offset) {
            let stats = self.get_or_create_last_read_replay_stats();
            stats.timer_offset = Some(timer_offset);
            stats.start = Some(n);
            Ok(())
        }
        else {
            Err(())
        }
    }

    /// Mark the end of a replay.
    pub fn mark_replay_end(&mut self) -> Result<(), ()> {
        if let Ok(n) = self.core.mark_end() {
            self.get_or_create_last_read_replay_stats().end = Some(n);
            Ok(())
        }
        else {
            Err(())
        }
    }

    fn get_or_create_last_read_replay_stats(&mut self) -> &mut LastReadReplayCropData {
        if self.last_read_replay_stats.is_none() {
            self.last_read_replay_stats = Some(LastReadReplayCropData::default());
        }
        self.last_read_replay_stats.as_mut().expect("should be created")
    }

    /// Change the given replay counter, adding delta.
    #[inline]
    pub fn change_replay_counter(&mut self, counter: String, delta: SignedInteger) {
        self.core.change_replay_counter(counter, delta);
    }

    /// Get all replay counters.
    #[inline]
    pub fn get_replay_counters(&self) -> BTreeMap<String, SignedInteger> {
        self.core.get_replay_counters()
    }

    fn load_file_or_make_generic(&mut self, dir: &Path, name: Option<&str>, generic_prefix: Option<&str>, extension: &str) -> Result<(File, String, PathBuf), UTF8CString> {
        match name {
            Some(name) => {
                let filename = format!("{name}.{extension}");
                let path = dir.join(&filename);
                Ok((File::create(&path).map_err(|e| format!("Can't open {name} for writing: {e}"))?, filename, path))
            },
            None => {
                let prefix = generic_prefix.unwrap_or(self.get_current_save_name().expect("no save name when game is running in load_file_or_make_generic"));
                let mut i = 0u64;
                loop {
                    let filename = format!("{prefix}-{i}.{extension}");
                    let path = dir.join(&filename);
                    let Ok(file) = File::create_new(&path) else {
                        i = i.checked_add(1).ok_or_else(|| UTF8CString::from_str("Maximum number of generics reached."))?;
                        continue
                    };
                    return Ok((file, filename, path))
                }
            }
        }
    }

    /// Loads a save state with the given name if it exists.
    ///
    /// If it does, and it is successfully loaded, `Ok(true)` is returned.
    ///
    /// If it does not exist, `Ok(false)` is returned.
    pub fn load_save_state_if_exists(&mut self, name: &str) -> Result<bool, UTF8CString> {
        if !self.is_game_running() {
            return Err("Game not running".into())
        }

        let current_rom_name = self.get_current_rom_name().expect("no rom name when game is running in load_save_state_if_exists");
        let save_states_dir = self.get_save_states_dir_for_rom(current_rom_name);
        let save_state_file = save_states_dir.join(format!("{name}.{SAVE_STATE_EXTENSION}"));

        if !save_state_file.is_file() {
            return Ok(false)
        }

        self.push_save_state_history();

        let save_state = std::fs::read(save_state_file).map_err(|e| format!("Failed to load save state {name}: {e}"))?;
        self.core.load_save_state(save_state);
        Ok(true)
    }

    /// Loads a replay with the given name if it exists.
    ///
    /// If it does, and it is successfully loaded, `Ok(true)` is returned.
    ///
    /// If it does not exist, `Ok(false)` is returned.
    pub fn load_replay_if_exists(&mut self, name: &str, override_errors: bool) -> Result<bool, UTF8CString> {
        self.assert_replays_available()?;

        let current_rom_name = self.get_current_rom_name().expect("no rom name when game is running in load_replay_if_exists");
        let replay_dir = self.get_replays_dir_for_rom(current_rom_name);
        let replay_file = replay_dir.join(format!("{name}.{REPLAY_EXTENSION}"));

        if !replay_file.is_file() {
            return Ok(false)
        }

        let file = match std::fs::read(replay_file) {
            Ok(n) => n,
            Err(e) => {
                return Err(format!("Failed to read replay {name}:\n\n{e}").into())
            }
        };

        let mut player = match ReplayFilePlayer::new(file, override_errors) {
            Ok(n) => n,
            Err(e) => {
                return Err(format!("Failed to parse replay {name}:\n\n{e:?}").into())
            }
        };

        self.set_paused(true);

        if self.settings.replay.auto_decompress_replays_upfront {
            player.decompress_all_blobs();
        }

        let current_emulator_type = self.emulator_type.expect("???? no emulator type when reloading a replay?");
        let metadata = player.get_replay_metadata();
        let expected_type = match metadata.console_type {
            ReplayConsoleType::GameBoy => SuperShuckieEmulatorType::GameBoy,
            ReplayConsoleType::SuperGameBoy2 => SuperShuckieEmulatorType::GameBoySGB2,
            ReplayConsoleType::GameBoyColor => SuperShuckieEmulatorType::GameBoyColor,
            _ => current_emulator_type
        };

        self.last_read_replay_stats = Some(LastReadReplayCropData {
            start: metadata.crop_start,
            end: metadata.crop_end,
            timer_offset: metadata.timer_offset
        });

        // TODO: let the user supply their own bios override instead
        self.load_builtin_bios_override(metadata.bios_checksum);

        if current_emulator_type != expected_type || self.bios_override.is_some() {
            self.instantiate_and_load_core(expected_type);
        }

        if let Err(e) = self.core.attach_replay_player(player, override_errors) {
            return match e {
                ReplayPlayerAttachError::Incompatible { description } => {
                    Err(format!("This replay file is incompatible:\n\n{description}").into())
                }
                ReplayPlayerAttachError::MismatchedMetadata { issues } => {
                    let mut err = String::new();

                    err += "This replay file has mismatched data which may prevent playback:";

                    for issue in issues {
                        err += "\n\n";
                        err += &issue.to_string();
                    }

                    Err(err.into())
                }
            }
        }

        self.save_file = Some(Arc::new("replay".into()));
        self.current_replay = Some(name.into());
        self.last_replay_and_frame = None;

        Ok(true)
    }

    /// Stop playing back any currently playing replay.
    #[inline]
    pub fn stop_replay_playback(&mut self) {
        let Some(r) = self.current_replay.take() else {
            return
        };

        self.last_replay_and_frame = Some((r, self.last_read_elapsed_time_stats.frames));
        self.last_read_replay_stats = None;

        self.core.detach_replay_player();
        self.reset_speed();
        self.current_input = Input::default();
    }

    /// Get the replay playback stats if currently playing back.
    pub fn get_replay_playback_stats(&self) -> Option<SuperShuckieReplayTimes> {
        if !self.core.is_playing_back() {
            return None;
        }
        
        let frames = self.core.get_playback_total_frames();
        let ms = self.core.get_playback_total_milliseconds();
        Some(SuperShuckieReplayTimes { total_milliseconds: ms, total_frames: frames })
    }

    fn load_builtin_bios_override(&mut self, hash: ReplayHeaderBlake3Hash) {
        self.bios_override = None;
        let gba_bios = include_bytes!("../../bootrom/agb/gba_bios.bin");
        if hash == blake3_hash(gba_bios) {
            self.bios_override = Some(gba_bios.to_vec());
        }
    }

    fn push_save_state_history(&mut self) {
        self.current_save_state_history.truncate(self.current_save_state_history_position);
        self.current_save_state_history.push(self.create_save_state_now());

        while self.current_save_state_history.len() > self.settings.emulation.max_save_state_history.get() {
            self.current_save_state_history.remove(0);
        }

        self.current_save_state_history_position = self.current_save_state_history.len();

    }

    fn create_save_state_now(&self) -> Vec<u8> {
        self.core.create_save_state().expect("Failed to create a save state for an unknown reason (this is a bug!).") // TODO: handle this failing?
    }

    /// Undo loading a save state, loading the state before loading the save state.
    pub fn undo_load_save_state(&mut self) -> bool {
        if self.current_save_state_history_position == 0 {
            return false // no more to go
        }

        let backup = self.create_save_state_now();
        self.current_save_state_history_position -= 1;

        let history = &mut self.current_save_state_history[self.current_save_state_history_position];
        let state_to_load = std::mem::replace(history, backup);

        self.core.load_save_state(state_to_load);
        true
    }

    /// Redo loading a save state, loading the save state before undoing loading the save state.
    pub fn redo_load_save_state(&mut self) -> bool {
        if self.current_save_state_history_position == self.current_save_state_history.len() {
            return false // no more to go
        }

        let backup = self.create_save_state_now();

        let history = &mut self.current_save_state_history[self.current_save_state_history_position];
        self.current_save_state_history_position += 1;

        let state_to_load = std::mem::replace(history, backup);

        self.core.load_save_state(state_to_load);
        true
    }

    #[inline]
    pub fn set_touch(&mut self, at: Option<(u8, u8)>) {
        self.current_input.touch = at;
        self.core.enqueue_input(self.current_input);
    }

    pub fn on_user_input(&mut self, input: UserInput, value: f64) {
        let Some(mode) = self.emulator_type else {
            return
        };

        let controls = self.get_control_settings(mode);

        let Some(control) = (match input {
            UserInput::Keyboard { keycode } => controls.keyboard_controls.get(&keycode).copied(),
            UserInput::Button { button, controller } => {
                self.connected_controllers.get(&controller)
                    .and_then(|i| controls.controller_controls.get(i.as_str()))
                    .and_then(|i| i.buttons.get(&button))
                    .copied()
            }
            UserInput::Axis { axis, controller } => {
                self.connected_controllers.get(&controller)
                    .and_then(|i| controls.controller_controls.get(i.as_str()))
                    .and_then(|i| i.axis.get(&axis))
                    .copied()
            }
        })
        else {
            return
        };

        let pressed = value > 0.5;

        if control.control.is_button() {
            if pressed && self.settings.replay.auto_stop_playback_on_input && self.get_replay_playback_stats().is_some() {
                self.stop_replay_playback();
            }

            if pressed && self.settings.replay.auto_unpause_on_input && self.is_paused() {
                self.set_paused(false);
            }

            match control.modifier {
                ControlModifier::Normal => {
                    control.control.set_for_input(&mut self.current_input, pressed);
                    self.core.enqueue_input(self.current_input);
                },
                ControlModifier::Rapid => {
                    if self.current_rapid_fire_input.is_none() {
                        if !pressed {
                            return
                        }

                        let mut new_rapid_fire = SuperShuckieRapidFire::default();
                        new_rapid_fire.hold_length = unsafe { NonZeroU64::new_unchecked(3) };
                        new_rapid_fire.interval = unsafe { NonZeroU64::new_unchecked(3) };
                        self.current_rapid_fire_input = Some(new_rapid_fire);
                    }

                    let Some(input) = self.current_rapid_fire_input.as_mut() else { unreachable!("we just enabled rapid fire input...!") };
                    control.control.set_for_input(&mut input.input, pressed);
                    if !pressed && input.input.is_empty() {
                        self.current_rapid_fire_input = None;
                    }
                    self.core.set_rapid_fire_input(self.current_rapid_fire_input);
                },
                ControlModifier::Toggle => {
                    if !pressed {
                        return
                    }
                    
                    if self.current_toggled_input.is_none() {
                        self.current_toggled_input = Some(Input::new());
                    }

                    let Some(input) = self.current_toggled_input.as_mut() else { unreachable!("we just enabled toggled input...!") };
                    control.control.invert_for_input(input);
                    if !pressed && input.is_empty() {
                        self.current_toggled_input = None;
                    }
                    self.core.set_toggled_input(self.current_toggled_input);
                }
            }
        }
        else if self.is_game_running() {
            match control.control {
                Control::Turbo => self.apply_turbo(value),
                Control::Reset => if pressed {
                    self.core.hard_reset();
                }
                Control::Pause => if pressed && self.is_game_running() {
                    self.set_paused(!self.is_paused());
                }

                Control::A => unreachable!(),
                Control::B => unreachable!(),
                Control::Start => unreachable!(),
                Control::Select => unreachable!(),
                Control::Up => unreachable!(),
                Control::Down => unreachable!(),
                Control::Left => unreachable!(),
                Control::Right => unreachable!(),
                Control::L => unreachable!(),
                Control::R => unreachable!(),
                Control::X => unreachable!(),
                Control::Y => unreachable!(),
            }
        }
    }

    pub fn load_rom<P: AsRef<Path>>(&mut self, path: P) -> Result<(), UTF8CString> {
        let path = path.as_ref();
        let Ok(path) = absolute(path) else {
            return Err(format!("Can't resolve path {} (failed)", path.display()).into())
        };
        let Some(path_utf8) = path.to_str() else {
            return Err(format!("Can't resolve path {} (not UTF-8)", path.display()).into())
        };

        let Some(filename) = path.file_name().and_then(|i| i.to_str()) else {
            return Err(format!(
                "{} does not appear to be a valid ROM file (missing filename)",
                path.display()
            ).into())
        };

        let Some(extension) = path.extension().and_then(|i| i.to_str()) else {
            return Err(format!("{filename} does not appear to be a valid ROM file (missing extension)").into())
        };

        let data = std::fs::read(&path).map_err(|e| {
            format!("Failed to read ROM at {filename}: {e}")
        })?;

        let emulator_to_use = match extension.to_lowercase().as_str() {
            "gb" | "gbc" => self.choose_for_game_boy(data.as_slice()),
            "gba" => SuperShuckieEmulatorType::GameBoyAdvance,
            "nds" => SuperShuckieEmulatorType::NintendoDS,
            unknown => return Err(format!("Unknown or unsupported ROM file type .{unknown}").into())
        };

        self.create_userdata_for_rom(filename)?;
        self.close_rom();
        self.loaded_rom_data = Some(data);
        self.rom_name = Some(Arc::new(UTF8CString::from_str(filename)));
        self.emulator_type = Some(emulator_to_use);
        self.save_file = Some(Arc::new(self.get_current_save_file_name_for_rom(filename)));
        self.reload_core();

        let path_cstr = UTF8CString::from_str(path_utf8);
        self.settings.recent_roms.recent_roms.retain(|i| i != &path_cstr);
        self.settings.recent_roms.recent_roms.insert(0, path_cstr);
        self.last_replay_and_frame = None;

        Ok(())
    }

    /// Get all recent ROMs.
    #[inline]
    pub fn get_recent_roms(&self) -> Vec<UTF8CString> {
        self.settings.recent_roms.recent_roms.clone()
    }

    /// Clear all recent ROMs.
    #[inline]
    pub fn clear_recent_roms(&mut self) {
        self.settings.recent_roms.recent_roms.clear();
    }

    /// Get the control settings.
    #[inline]
    pub fn get_control_settings(&self, emulator_type: SuperShuckieEmulatorType) -> &Controls {
        match emulator_type {
            SuperShuckieEmulatorType::GameBoySGB2 | SuperShuckieEmulatorType::GameBoyColor | SuperShuckieEmulatorType::GameBoy => &self.settings.game_boy_settings.controls,
            SuperShuckieEmulatorType::GameBoyAdvance => &self.settings.game_boy_advance_settings.controls,
            SuperShuckieEmulatorType::NintendoDS => &self.settings.nintendo_ds_settings.controls
        }
    }

    /// Overwrite the control settings.
    #[inline]
    pub fn set_control_settings(&mut self, controls: Controls, emulator_type: SuperShuckieEmulatorType) {
        *match emulator_type {
            SuperShuckieEmulatorType::GameBoySGB2 | SuperShuckieEmulatorType::GameBoyColor | SuperShuckieEmulatorType::GameBoy => &mut self.settings.game_boy_settings.controls,
            SuperShuckieEmulatorType::GameBoyAdvance => &mut self.settings.game_boy_advance_settings.controls,
            SuperShuckieEmulatorType::NintendoDS => &mut self.settings.nintendo_ds_settings.controls
        } = controls;
    }

    /// Hard reset the console.
    #[inline]
    pub fn hard_reset_console(&mut self) {
        self.core.hard_reset()
    }

    fn create_userdata_for_rom(&mut self, rom: &str) -> Result<(), UTF8CString> {
        fn create_if_not_dir(what: &Path) -> Result<(), UTF8CString> {
            if !what.is_dir() && let Err(e) = std::fs::create_dir(what) {
                return Err(format!("Failed to create userdata dir for {}: {e}", what.display()).into());
            }
            Ok(())
        }

        create_if_not_dir(&self.get_userdir_for_rom(rom))?;
        create_if_not_dir(&self.get_save_states_dir_for_rom(rom))?;
        create_if_not_dir(&self.get_save_data_dir_for_rom(rom))?;
        create_if_not_dir(&self.get_replays_dir_for_rom(rom))?;

        Ok(())
    }

    fn get_save_states_dir_for_rom(&self, rom: &str) -> PathBuf {
        self.get_userdir_for_rom(rom).join("save states")
    }

    fn get_save_data_dir_for_rom(&self, rom: &str) -> PathBuf {
        self.get_userdir_for_rom(rom).join("save data")
    }

    fn get_replays_dir_for_rom(&self, rom: &str) -> PathBuf {
        self.get_userdir_for_rom(rom).join("replays")
    }

    fn get_userdir_for_rom(&self, filename: &str) -> PathBuf {
        self.user_dir.join(format!("{filename}-data"))
    }

    #[inline]
    pub fn get_user_dir(&self) -> UTF8CString {
        self.user_dir.to_str().expect("path is not UTF-8").into()
    }

    #[inline]
    pub fn get_dir_for_current_rom(&self) -> Option<UTF8CString> {
        self.get_current_rom_name()
            .map(|rom| self.get_userdir_for_rom(rom).to_str().expect("rom path is not UTF-8").into())
    }

    #[inline]
    pub fn reload_core(&mut self) {
        let emulator_type = self.emulator_type.expect("reload_rom_in_place with no emulator type");
        self.bios_override = None;
        self.instantiate_and_load_core(emulator_type);
    }

    fn instantiate_and_load_core(&mut self, emulator_type: SuperShuckieEmulatorType) {
        let rom_name = self.get_current_rom_name().expect("reload_rom_in_place with no loaded ROM");
        let save_file = self.get_current_save_name().expect("reload_rom_in_place with no save file");
        let save_file_data = self.get_save_file_data(rom_name, save_file);
        let rom_data = self.loaded_rom_data.as_ref().expect("reload_rom_in_place with no loaded rom");
        let core = self.make_new_core(rom_data, save_file_data, emulator_type);
        self.switch_core(ThreadedSuperShuckieCore::new(core));
    }

    fn switch_core(&mut self, core: ThreadedSuperShuckieCore) {
        self.before_unload_or_reload_rom();
        let was_transferred = self.settings.pokeabyte.enabled && self.core.transfer_pokeabyte_integration(&core);
        self.assign_core(core);
        self.after_switch_core();

        self.force_refresh_screens();
        self.current_input = Input::default();
        self.core.set_speed(Speed::from_multiplier_float(self.settings.emulation.base_speed_multiplier));
        if !was_transferred && self.settings.pokeabyte.enabled {
            let _ = self.set_pokeabyte_enabled(true);
        }
    }

    fn assign_core(&mut self, new_core: ThreadedSuperShuckieCore) {
        let was_paused = self.core.is_paused();
        if was_paused {
            new_core.pause();
        }
        self.core = new_core;
    }

    fn reset_save_state_history(&mut self) {
        self.current_save_state_history = Vec::new();
        self.current_save_state_history_position = 0;
    }

    fn make_new_core(&self, rom_data: &[u8], save_file: Option<Vec<u8>>, emulator_type: SuperShuckieEmulatorType) -> Box<dyn EmulatorCore> {
        let bios = self.get_bios_for_core(emulator_type);

        let sram = save_file.as_ref().map(|i| i.as_slice());

        let core: Box<dyn EmulatorCore> = match emulator_type {
            SuperShuckieEmulatorType::GameBoy => Box::new(GameBoyColor::new_from_rom(rom_data, bios.as_slice(), sram, Model::DmgB)),
            SuperShuckieEmulatorType::GameBoySGB2 => Box::new(GameBoyColor::new_from_rom(rom_data, bios.as_slice(), sram, Model::Sgb2)),
            SuperShuckieEmulatorType::GameBoyColor => Box::new(GameBoyColor::new_from_rom(rom_data, bios.as_slice(), sram, Model::Cgb0)),
            SuperShuckieEmulatorType::GameBoyAdvance => Box::new(GameBoyAdvance::new_from_rom(rom_data, sram, bios.as_slice(), std_timestamp_provider())),
            SuperShuckieEmulatorType::NintendoDS => {
                let mut core = Box::new(NintendoDS::new_from_rom(
                    rom_data,
                    sram,
                    std_timestamp_provider(),
                    self.settings.nintendo_ds_settings.jit
                ));

                let date = self.get_nds_date().get_cleaned();
                core.set_date(
                    date.year,
                    date.month,
                    date.day,
                    date.hour,
                    date.minute,
                    date.second
                );

                core
            }
        };

        core
    }

    fn get_current_save_file_name_for_rom(&mut self, rom: &str) -> UTF8CString {
        self.settings.get_rom_config_or_default(rom).save_name.clone()
    }

    fn get_save_file_data(&self, rom: &str, save_file: &str) -> Option<Vec<u8>> {
        std::fs::read(self.get_save_path(rom, save_file)).ok()
    }

    fn delete_save_file_data(&mut self, rom: &str, save_file: &str) {
        let _ = std::fs::remove_file(self.get_save_path(rom, save_file)).ok();
    }

    fn get_save_path(&self, rom: &str, save_file: &str) -> PathBuf {
        self.get_save_data_dir_for_rom(rom)
            .join(format!("{save_file}.{SAVE_DATA_EXTENSION}"))
    }

    fn get_bios_for_core(&self, emulator_kind: SuperShuckieEmulatorType) -> Vec<u8> {
        if let Some(s) = self.bios_override.clone() {
            return s;
        }

        // Defaults
        match emulator_kind {
            SuperShuckieEmulatorType::GameBoy | SuperShuckieEmulatorType::GameBoySGB2 => include_bytes!("../../bootrom/dmg/dmg.bin").to_vec(),
            SuperShuckieEmulatorType::GameBoyColor => include_bytes!("../../bootrom/cgb/cgb_boot/cgb_boot_fast.bin").to_vec(),
            SuperShuckieEmulatorType::GameBoyAdvance => Vec::new(),
            SuperShuckieEmulatorType::NintendoDS => Vec::new()
        }
    }

    /// Close the ROM, saving.
    pub fn close_rom(&mut self) {
        self.save_sram_unchecked();
        self.unload_rom();
    }

    /// Unload the ROM without saving.
    pub fn unload_rom(&mut self) {
        self.before_unload_or_reload_rom();
        self.assign_core(ThreadedSuperShuckieCore::new(Box::new(NullEmulatorCore)));
        self.save_file = None;
        self.rom_name = None;
        self.emulator_type = None;
        self.current_input = Input::default();
        self.after_switch_core();
    }

    /// Set whether or not the game is paused.
    pub fn set_paused(&mut self, paused: bool) {
        if paused {
            self.core.pause();
        }
        else {
            self.core.start();
        }
    }

    /// Set whether or not the game is paused temporarily.
    pub fn set_playback_frozen(&mut self, paused: bool) {
        self.core.set_playback_frozen(paused);
    }

    /// Get whether or not the game is manually paused
    pub fn is_paused(&self) -> bool {
        self.core.is_paused()
    }

    /// Save the SRAM.
    pub fn save_sram(&mut self) -> Result<(), UTF8CString> {
        if !self.is_game_running() {
            return Err("Game not running".into())
        }

        let current_rom = self.get_current_rom_name().expect("save_sram with no current ROM");
        let current_save = self.get_current_save_name().expect("save_sram with no current save");

        let sram = self.core.get_sram().expect("save_sram failed to get sram (BUG!)");
        let save_file = self.get_save_path(current_rom, current_save);

        std::fs::write(&save_file, sram).map_err(|e| format!("Failed to write SRAM to disk: {e}").into())
    }

    fn save_sram_unchecked(&mut self) {
        let _ = self.save_sram();
    }

    /// Return `true` if a ROM is running.
    #[inline]
    pub fn is_game_running(&self) -> bool {
        self.emulator_type.is_some()
    }

    /// Calls the `refresh_screens` callback regardless of if there's a new frame.
    #[inline]
    pub fn force_refresh_screens(&mut self) {
        self.refresh_screen(true);
    }

    /// Set the video scale for the current system.
    ///
    /// This value is saved per-system.
    pub fn set_video_scale(&mut self, scale: NonZeroU8) {
        let old_scale = match self.emulator_type {
            None => return,
            Some(n) => match n {
                SuperShuckieEmulatorType::GameBoy
                | SuperShuckieEmulatorType::GameBoySGB2
                | SuperShuckieEmulatorType::GameBoyColor => &mut self.settings.game_boy_settings.video_scale,
                SuperShuckieEmulatorType::GameBoyAdvance => &mut self.settings.game_boy_advance_settings.video_scale,
                SuperShuckieEmulatorType::NintendoDS => &mut self.settings.nintendo_ds_settings.video_scale
            }
        };

        if scale == *old_scale {
            return
        }

        *old_scale = scale;
        self.update_video_mode();
    }

    /// Get the game speed settings.
    pub fn get_speed_settings(&self, base: &mut f64, turbo: &mut f64) {
        *base = self.settings.emulation.base_speed_multiplier;
        *turbo = self.settings.emulation.turbo_speed_multiplier;
    }

    /// Set the game speed.
    pub fn set_speed_settings(&mut self, mut base: f64, mut turbo: f64) {
        base = Speed::from_multiplier_float(base).into_multiplier_float();
        turbo = Speed::from_multiplier_float(turbo).into_multiplier_float();

        self.settings.emulation.base_speed_multiplier = base;
        self.settings.emulation.turbo_speed_multiplier = turbo;

        self.reset_speed();
    }

    /// Set a custom setting.
    pub fn set_custom_setting(&mut self, setting: &str, value: Option<UTF8CString>) {
        match value {
            Some(n) => { self.settings.custom.insert(setting.to_owned(), n); },
            None => { self.settings.custom.remove(setting); }
        }
    }

    /// Get the Nintendo DS date.
    #[inline]
    pub fn get_nds_date(&self) -> &NintendoDSDate {
        &self.settings.nintendo_ds_settings.date
    }

    /// Set the Nintendo DS date.
    #[inline]
    pub fn set_nds_date(&mut self, date: NintendoDSDate) {
        self.settings.nintendo_ds_settings.date = date;
    }

    /// Get the Nintendo DS date.
    #[inline]
    pub fn get_jit_enabled(&self) -> bool {
        self.settings.nintendo_ds_settings.jit
    }

    /// Set the Nintendo DS date.
    #[inline]
    pub fn set_jit_enabled(&mut self, enabled: bool) {
        if self.settings.nintendo_ds_settings.jit == enabled {
            return
        }
        self.settings.nintendo_ds_settings.jit = enabled;

        if self.emulator_type == Some(SuperShuckieEmulatorType::NintendoDS) {
            self.stop_recording_replay();
            self.stop_replay_playback();

            self.core.pause();
            let state = self.core.create_save_state().expect("failed to make save state?");
            self.reload_core();
            self.core.load_save_state(state);
        }

    }

    /// Get a custom setting.
    pub fn get_custom_setting(&self, setting: &str) -> Option<&UTF8CString> {
        self.settings.custom.get(setting)
    }

    /// Set the current save file, optionally initializing (clearing) the old one.
    ///
    /// The game will be reloaded.
    pub fn load_or_create_save_file(&mut self, save_file: &str, initialize: bool) {
        if !self.is_game_running() {
            return;
        }

        self.set_current_save_file(save_file);

        if initialize {
            let rom_name = self.get_current_rom_name_arc().expect("save file when not running");
            self.delete_save_file_data(rom_name.as_str(), save_file);
        }

        self.reload_core();
    }

    /// Set the current save file.
    ///
    /// The game will NOT be reloaded.
    pub fn set_current_save_file(&mut self, save_file: &str) {
        if !self.is_game_running() {
            return;
        }

        self.save_sram_unchecked();

        let rom_name = self.get_current_rom_name_arc().expect("save file when not running");
        self.settings.get_rom_config_or_default(rom_name.as_str()).save_name = save_file.into();
        self.save_file = Some(Arc::new(save_file.into()));
    }

    /// Handle any logic that needs to be done regularly.
    ///
    /// Returns any errors that may occur
    pub fn tick(&mut self) -> Result<(), UTF8CString> {
        let mut errors = String::new();

        self.refresh_screen(false);

        let replay_errors = self.core.get_replay_recording_errors();
        if !replay_errors.is_empty() {
            let len_after_three = replay_errors.len().saturating_sub(3);

            for i in replay_errors.iter().take(3) {
                errors += &format!("- REPLAY ERROR: {i}\n");
            }

            if len_after_three > 0 {
                errors += &format!("...and {len_after_three} more replay error(s)\n");
            }
            self.core.stop_recording_replay();

            if let Some(r) = self.recording_replay_file.take() {
                errors += &format!("\n\nYour recording was stopped. The temp file ({}) was not deleted.", r.temp_replay_path.file_name().expect("no temp file filename???").display());
            }
        }

        if let Some(mut s) = self.web_server.take() {
            let mut stats: OnceCell<Arc<Stats>> = OnceCell::new();
            let replays: OnceCell<Arc<Vec<String>>> = OnceCell::new();

            let reset_stats = |stats: &mut OnceCell<Arc<Stats>>| {
                *stats = OnceCell::new();
            };

            let make_stats = |what: &SuperShuckieFrontend, stats: &OnceCell<Arc<Stats>>| -> Arc<Stats> {
                stats.get_or_init(move || {
                    let counters = what.get_replay_counters();

                    // use cached to avoid DoSing the emulator lol
                    let stats = what.last_read_elapsed_time_stats;

                    let replay_state = what.get_replay_state();
                    let is_playing_back = replay_state == SuperShuckieReplayState::Playback;
                    let is_recording = replay_state == SuperShuckieReplayState::Recording;
                    let is_playback_finished = what.core.is_replay_playback_finished();

                    let replay_stats = what.last_read_replay_stats.as_ref();

                    let timer_start = replay_stats.and_then(|i| i.start).map(|n| n.1.0 as u32);
                    let timer_end = replay_stats.and_then(|i| i.end).map(|n| n.1.0 as u32);
                    let timer_offset = replay_stats.and_then(|i| i.timer_offset).map(|n| n.0 as u32);

                    let mut timer_current = if let Some(start) = timer_start {
                        let value = stats.milliseconds.saturating_sub(start);

                        if let Some(end) = timer_end {
                            Some(value.min(end.saturating_sub(start)))
                        }
                        else {
                            Some(value)
                        }
                    }
                    else {
                        None
                    };

                    if let Some(c) = timer_current.as_mut() && let Some(offset) = timer_offset {
                        *c = c.wrapping_add(offset)
                    };

                    Arc::new(Stats {
                        time_start: timer_start,
                        time_end: timer_end,
                        time_current: timer_current,
                        time_offset: timer_offset,
                        total_elapsed_time: stats.milliseconds,
                        total_elapsed_frames: stats.frames,
                        is_playing_back,
                        is_recording,
                        is_playback_finished,
                        current_speed: stats.speed.into_multiplier_float(),
                        counters,
                        is_paused: what.is_paused(),
                    })
                }).clone()
            };

            while let Some(s) = s.next_server_command() {
                match s {
                    SuperShuckieServerCommand::Stats(t) => {
                        let _ = t.send(make_stats(self, &stats).clone());
                    }
                    SuperShuckieServerCommand::MarkStart(t, timer_offset) => {
                        reset_stats(&mut stats);
                        let _ = t.send(self.mark_replay_start(TimestampMillis(timer_offset as UnsignedInteger)).is_ok());
                    }
                    SuperShuckieServerCommand::MarkEnd(t) => {
                        reset_stats(&mut stats);
                        let _ = t.send(self.mark_replay_end().is_ok());
                    }
                    SuperShuckieServerCommand::IncrementCounter(t, name, counter) => {
                        reset_stats(&mut stats);
                        let _ = t.send(self.get_replay_state() == SuperShuckieReplayState::Recording);
                        self.change_replay_counter(name, counter);
                    }
                    SuperShuckieServerCommand::SetPaused(t, paused) => {
                        self.set_paused(paused);
                        reset_stats(&mut stats);
                        let _ = t.send(true);
                    }
                    SuperShuckieServerCommand::LoadReplay(t, replay) => {
                        let _ = match self.load_replay_if_exists(&replay, true) {
                            Ok(true) => t.send(true),
                            _ => t.send(false)
                        };
                    }
                    SuperShuckieServerCommand::GoToFrame(t, frame) => {
                        if self.get_replay_state() != SuperShuckieReplayState::Playback {
                            let _ = t.send(false);
                            continue;
                        }
                        self.go_to_replay_frame(frame);
                        let _ = t.send(true);
                    }
                    SuperShuckieServerCommand::EnumerateReplays(t) => {
                        let replays = replays.get_or_init(|| {
                            if let Some(n) = self.get_current_rom_name() {
                                Arc::new(self.get_all_replays_for_rom(n).iter().map(UTF8CString::to_string).collect())
                            }
                            else {
                                Arc::new(Vec::new())
                            }
                        }).clone();
                        let _ = t.send(replays);
                    }
                    SuperShuckieServerCommand::SetPlaybackSpeed(t, speed) => {
                        if self.is_game_running() {
                            self.core.set_speed(Speed::from_multiplier_float(speed));
                            let _ = t.send(true);
                        }
                        else {
                            let _ = t.send(false);
                        }
                    }
                }
            }

            self.web_server = Some(s);
        }

        if errors.is_empty() {
            Ok(())
        }
        else {
            Err(errors.trim().into())
        }
    }

    fn refresh_screen(&mut self, force: bool) {
        let current_stats = self.core.get_elapsed_time();
        if !force && current_stats.frames == self.last_read_elapsed_time_stats.frames {
            return
        }

        self.last_read_elapsed_time_stats = current_stats;
        self.core.read_screens(|screens| {
            self.callbacks.refresh_screens(screens);
        })
    }

    fn get_current_rom_name_arc(&self) -> Option<Arc<UTF8CString>> {
        self.rom_name.clone()
    }

    pub fn get_current_rom_name(&self) -> Option<&str> {
        self.rom_name.as_ref().map(|i| i.as_str())
    }

    pub fn get_current_rom_name_c_str(&self) -> Option<&CStr> {
        self.rom_name.as_ref().map(|i| i.as_c_str())
    }

    pub fn get_current_save_name(&self) -> Option<&str> {
        self.save_file.as_ref().map(|i| i.as_str())
    }

    pub fn get_current_save_name_c_str(&self) -> Option<&CStr> {
        self.save_file.as_ref().map(|i| i.as_c_str())
    }

    #[inline]
    pub fn set_auto_stop_playback_on_input_setting(&mut self, new_setting: bool) {
        self.settings.replay.auto_stop_playback_on_input = new_setting
    }

    #[inline]
    pub fn get_auto_stop_playback_on_input_setting(&self) -> bool {
        self.settings.replay.auto_stop_playback_on_input
    }

    #[inline]
    pub fn set_auto_unpause_on_input_setting(&mut self, new_setting: bool) {
        self.settings.replay.auto_unpause_on_input = new_setting
    }

    #[inline]
    pub fn get_auto_unpause_on_input_setting(&self) -> bool {
        self.settings.replay.auto_unpause_on_input
    }

    #[inline]
    pub fn set_auto_pause_on_record_setting(&mut self, new_setting: bool) {
        self.settings.replay.auto_pause_on_record = new_setting
    }

    #[inline]
    pub fn get_auto_pause_on_record_setting(&self) -> bool {
        self.settings.replay.auto_pause_on_record
    }

    #[inline]
    pub fn set_auto_decompress_replays_upfront_setting(&mut self, new_setting: bool) {
        self.settings.replay.auto_decompress_replays_upfront = new_setting;
    }

    #[inline]
    pub fn get_auto_decompress_replays_upfront_setting(&self) -> bool {
        self.settings.replay.auto_decompress_replays_upfront
    }

    /// Get the number of milliseconds elapsed.
    #[inline]
    pub fn get_elapsed_milliseconds(&self) -> u32 {
        self.last_read_elapsed_time_stats.milliseconds
    }

    /// Get the number of milliseconds elapsed.
    #[inline]
    pub fn get_elapsed_frames(&self) -> u32 {
        self.last_read_elapsed_time_stats.frames
    }

    /// Skip to the desired frame.
    #[inline]
    pub fn go_to_replay_frame(&mut self, frame: u32) {
        self.core.go_to_replay_frame(frame);
    }

    #[inline]
    pub fn advance_playback_frames(&mut self, delta: i32) {
        self.core.advance_playback_frames(delta)
    }

    /// Save the settings to disk.
    #[inline]
    pub fn write_config(&self) {
        // TODO: handle errors here?
        let _ = std::fs::write(
            &self.config_dir.join(SETTINGS_FILE),
            serde_json::to_string_pretty(&self.settings).expect("failed to serialize")
        );
    }

    fn before_unload_or_reload_rom(&mut self) {
        self.reset_save_state_history();
        self.stop_recording_replay();
        self.stop_replay_playback();
        self.pokeabyte_error = None;
    }

    /// Start recording a replay.
    ///
    /// If `name` is set, that name will be used.
    ///
    /// Returns the name of the replay if started.
    pub fn start_recording_replay(&mut self, name: Option<&str>) -> Result<UTF8CString, UTF8CString> {
        self.assert_replays_available()?;

        let current_rom_name = self.get_current_rom_name_arc().expect("no rom name when game is running in start_recording_replay");
        let save_states_dir = self.get_replays_dir_for_rom(current_rom_name.as_str());

        let (final_file, final_replay, final_replay_path) = self.load_file_or_make_generic(&save_states_dir, name, None, REPLAY_EXTENSION)?;
        let (temp_file, _, temp_replay) = self.load_file_or_make_generic(&save_states_dir, name, Some("temp"), REPLAY_EXTENSION)?;

        if self.settings.replay.auto_pause_on_record {
            self.set_paused(true);
        }

        self.last_read_replay_stats = None;
        self.last_replay_and_frame = None;

        self.core.start_recording_replay(PartialReplayRecordMetadata {
            rom_name: current_rom_name.to_string(),
            rom_filename: current_rom_name.to_string(),

            settings: ReplayFileRecorderSettings {
                minimum_uncompressed_bytes_per_blob: (self.settings.replay.max_recording_blob_size_mb.get() as usize)
                    .saturating_mul(1024)
                    .saturating_mul(1024),
                compression_level: self.settings.replay.zstd_compression_level
            },

            // TODO: patches
            patch_format: ReplayPatchFormat::Unpatched,
            patch_target_checksum: ReplayHeaderBlake3Hash::default(),
            patch_data: ByteVec::default(),

            frames_per_keyframe: self.settings.replay.frames_per_keyframe,

            // have a buffer so we don't destroy your SSD
            final_file: BufWriter::with_capacity(8 * 1024 * 1024, final_file),
            temp_file: BufWriter::with_capacity(8 * 1024 * 1024, temp_file),
        });

        self.recording_replay_file = Some(ReplayFileInfo {
            final_replay_name: final_replay.clone().into(),
            temp_replay_path: temp_replay,
            final_replay_path
        });

        Ok(final_replay.into())
    }

    fn assert_replays_available(&self) -> Result<(), UTF8CString> {
        let Some(emulator_type) = self.emulator_type else {
            return Err("No ROM loaded".into());
        };

        if !self.is_game_running() {
            return Err("No game running".into());
        }

        if self.settings.nintendo_ds_settings.jit && emulator_type == SuperShuckieEmulatorType::NintendoDS {
            return Err("Replays are disabled for Nintendo DS (JIT enabled)".into());
        }

        Ok(())
    }

    /// Stop recording replay.
    pub fn stop_recording_replay(&mut self) {
        let Some(replay_file) = self.recording_replay_file.take() else {
            return
        };

        // FIXME: We should make sure that it actually finalized here before deleting the temp file.
        let zero_frames = self.core.get_elapsed_time().frames == 0;

        self.last_read_replay_stats = None;

        self.core.stop_recording_replay();
        let _ = std::fs::remove_file(&replay_file.temp_replay_path);

        if zero_frames {
            let _ = std::fs::remove_file(&replay_file.final_replay_path);
        }
    }

    #[inline]
    pub fn continue_last_replay(&mut self) -> Result<bool, UTF8CString> {
        let Some((name, frame)) = self.last_replay_and_frame.take() else {
            return Ok(false)
        };

        self.load_replay_if_exists(name.as_str(), true)?;
        let current_frame = self.core.get_elapsed_time().frames;

        if frame == current_frame {
            return Ok(true)
        }

        self.core.pause();
        self.core.go_to_replay_frame(frame);

        let mut infinite_loop_prevention = 0;
        while self.core.get_elapsed_time().frames == current_frame && infinite_loop_prevention < 50 {
            std::thread::sleep(Duration::from_millis(100));
            self.core.rendezvous();
            infinite_loop_prevention += 1;
        }

        self.refresh_screen(true);
        Ok(true)
    }

    #[inline]
    pub fn can_continue_last_replay(&self) -> bool {
        self.last_replay_and_frame.is_some()
    }

    /// Get all saves for the given ROM.
    #[inline]
    pub fn get_all_saves_for_rom(&self, rom: &str) -> Vec<UTF8CString> {
        list_files_in_dir_with_extension(&self.get_save_data_dir_for_rom(rom), SAVE_DATA_EXTENSION)
    }

    /// Get all save states for the given ROM.
    #[inline]
    pub fn get_all_save_states_for_rom(&self, rom: &str) -> Vec<UTF8CString> {
        list_files_in_dir_with_extension(&self.get_save_states_dir_for_rom(rom), SAVE_STATE_EXTENSION)
    }

    /// Get all replays for the given ROM.
    #[inline]
    pub fn get_all_replays_for_rom(&self, rom: &str) -> Vec<UTF8CString> {
        list_files_in_dir_with_extension(&self.get_replays_dir_for_rom(rom), REPLAY_EXTENSION)
    }

    /// Set whether or not speed changes in replays are ignored
    #[inline]
    pub fn set_ignore_speed_changes_in_replays(&mut self, ignored: bool) {
        self.settings.replay.ignore_speed_changes_in_replays = ignored;
        self.core.set_ignore_speed_changes_in_replay(ignored);
        self.reset_speed();
    }

    /// Get whether or not speed changes in replays are ignored.
    #[inline]
    pub fn get_ignore_speed_changes_in_replays(&self) -> bool {
        self.settings.replay.ignore_speed_changes_in_replays
    }

    /// Set whether or not keyframes are automatically resynced on playback.
    #[inline]
    pub fn set_auto_resync_keyframes_in_replay(&mut self, resync: bool) {
        self.settings.replay.auto_resync_keyframes_in_replays = resync;
        self.core.set_auto_resync_keyframes_in_replay(resync);
    }

    /// Get whether or not keyframes are automatically resynced on playback.
    #[inline]
    pub fn get_auto_resync_keyframes_in_replay(&self) -> bool {
        self.settings.replay.auto_resync_keyframes_in_replays
    }

    fn after_switch_core(&mut self) {
        if self.settings.replay.ignore_speed_changes_in_replays {
            self.core.set_ignore_speed_changes_in_replay(true);
        }
        if self.settings.replay.auto_resync_keyframes_in_replays {
            self.core.set_auto_resync_keyframes_in_replay(true);
        }
        self.update_video_mode();
    }

    fn update_video_mode(&mut self) {
        let video_scale = match self.emulator_type {
            None => unsafe { NonZeroU8::new_unchecked(4) },
            Some(n) => match n {
                SuperShuckieEmulatorType::GameBoy
                | SuperShuckieEmulatorType::GameBoySGB2
                | SuperShuckieEmulatorType::GameBoyColor => self.settings.game_boy_settings.video_scale,
                SuperShuckieEmulatorType::GameBoyAdvance => self.settings.game_boy_advance_settings.video_scale,
                SuperShuckieEmulatorType::NintendoDS => self.settings.nintendo_ds_settings.video_scale
            }
        };

        self.core.read_screens(|screens| {
            self.callbacks.change_video_mode(screens, video_scale);
        });
    }

    #[inline]
    fn reset_speed(&mut self) {
        self.apply_turbo(0.0);
    }

    fn apply_turbo(&mut self, turbo: f64) {
        if !self.settings.replay.ignore_speed_changes_in_replays && self.get_replay_playback_stats().is_some() {
            return
        }

        let base_speed = self.settings.emulation.base_speed_multiplier;
        let max_speed = self.settings.emulation.turbo_speed_multiplier * base_speed;
        let total_speed = base_speed + (max_speed - base_speed) * turbo;
        self.core.set_speed(Speed::from_multiplier_float(total_speed));
    }

    #[inline]
    /// Get the replay file info, or `None` if not recording.
    pub fn get_replay_file_info(&self) -> Option<&ReplayFileInfo> {
        self.recording_replay_file.as_ref()
    }

    /// Returns true if PokeAByte is enabled, false if not, or an error if there was an error starting it.
    pub fn is_pokeabyte_enabled(&self) -> Result<bool, &UTF8CString> {
        match self.pokeabyte_error.as_ref() {
            Some(e) => Err(e),
            None => Ok(self.settings.pokeabyte.enabled)
        }
    }

    /// Set whether or not the Poke-A-Byte integration server is enabled.
    pub fn set_pokeabyte_enabled(&mut self, enabled: bool) -> Result<(), &UTF8CString> {
        self.settings.pokeabyte.enabled = enabled;
        self.pokeabyte_error = None;
        match self.core.set_pokeabyte_enabled(enabled) {
            Ok(_) => Ok(()),
            Err(e) => {
                self.pokeabyte_error = Some(e.into());
                Err(self.pokeabyte_error.as_ref().expect("pokeabyte_error was just set earlier..."))
            }
        }
    }

    /// Returns true if external commands are enabled, false if not, or an error if there was an error starting it.
    pub fn get_external_commands_enabled(&self) -> Result<bool, &UTF8CString> {
        match self.external_commands_error.as_ref() {
            Some(e) => Err(e),
            None => Ok(self.settings.external_commands.enabled)
        }
    }

    /// Set whether or not external commands are enabled, returning an error if there was an error starting it.
    pub fn set_external_commands_enabled(&mut self, enabled: bool) -> Result<(), &UTF8CString> {
        self.settings.external_commands.enabled = enabled;
        self.external_commands_error = None;
        if !enabled {
            self.web_server = None;
            return Ok(())
        }
        if self.web_server.is_some() {
            return Ok(())
        }
        match SuperShuckieWebserver::new("127.0.0.1:30158") {
            Ok(n) => {
                self.web_server = Some(n);
                Ok(())
            },
            Err(e) => {
                self.external_commands_error = Some(e.into());
                Err(self.external_commands_error.as_ref().expect("??? we just set it"))
            }
        }
    }

    #[inline]
    pub fn get_replay_state(&self) -> SuperShuckieReplayState {
        if self.get_replay_playback_stats().is_some() {
            SuperShuckieReplayState::Playback
        }
        else if self.get_replay_file_info().is_some() {
            SuperShuckieReplayState::Recording
        }
        else {
            SuperShuckieReplayState::NoReplay
        }
    }

    #[inline]
    pub fn get_gbc_mode(&self) -> GameBoyMode {
        self.settings.game_boy_settings.gbc_mode
    }

    #[inline]
    pub fn set_gbc_mode(&mut self, mode: GameBoyMode) {
        self.settings.game_boy_settings.gbc_mode = mode;
        self.reload_game_boy_if_needed();
    }

    #[inline]
    pub fn is_sgb_enabled(&self) -> bool {
        self.settings.game_boy_settings.sgb
    }

    #[inline]
    pub fn set_sgb_enabled(&mut self, enabled: bool) {
        self.settings.game_boy_settings.sgb = enabled;
        self.reload_game_boy_if_needed();
    }

    #[inline]
    pub fn get_emulator_type(&self) -> Option<SuperShuckieEmulatorType> {
        self.emulator_type
    }

    fn reload_game_boy_if_needed(&mut self) {
        let current = match self.emulator_type {
            Some(n) if matches!(n, SuperShuckieEmulatorType::GameBoy | SuperShuckieEmulatorType::GameBoyColor | SuperShuckieEmulatorType::GameBoySGB2) => n,
            _ => return
        };

        let Some(rom) = self.loaded_rom_data.as_ref() else {
            panic!("emulator_type is non-None but we have no loaded rom data???")
        };

        let expected = self.choose_for_game_boy(rom.as_slice());

        if expected != current {
            self.emulator_type = Some(expected);
            self.reload_core();
        }
    }

    fn choose_for_game_boy(&self, data: &[u8]) -> SuperShuckieEmulatorType {
        let game_boy = match self.settings.game_boy_settings.sgb {
            true => SuperShuckieEmulatorType::GameBoySGB2,
            false => SuperShuckieEmulatorType::GameBoy
        };

        match self.settings.game_boy_settings.gbc_mode {
            GameBoyMode::AlwaysGBC => SuperShuckieEmulatorType::GameBoyColor,
            GameBoyMode::AlwaysGB => game_boy,
            GameBoyMode::GBInGBMode => {
                if data.get(0x143).copied() == Some(0x00) {
                    game_boy
                }
                else {
                    SuperShuckieEmulatorType::GameBoyColor
                }
            },
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
#[repr(C)]
pub enum SuperShuckieReplayState {
    NoReplay,
    Recording,
    Playback
}

fn list_files_in_dir_with_extension(dir: &Path, extension: &str) -> Vec<UTF8CString> {
    let Ok(n) = std::fs::read_dir(dir) else {
        return Vec::new()
    };

    let mut options = Vec::new();
    for item in n {
        let Ok(item) = item else { continue };
        let path = item.path();
        if path.extension() != Some(extension.as_ref()) {
            continue
        }
        if !path.is_file() {
            continue
        }
        let Some(stem) = path.file_stem() else {
            continue
        };
        let Some(stem_utf8) = stem.to_str() else {
            continue
        };
        options.push(stem_utf8.into());
    }

    // Ensure the number at the end is compared numerically (if the rest is the same)
    options.sort_by(|a: &UTF8CString, b: &UTF8CString| {
        let a_str = a.as_str();
        let b_str = b.as_str();

        let a_split: Vec<&str> = a_str.rsplitn(2, '-').collect();
        let b_split: Vec<&str> = b_str.rsplitn(2, '-').collect();

        if a_split.len() != 2 || b_split.len() != 2 {
            return a_str.cmp(b_str);
        }

        // 1 is the prefix, 0 is the suffix (because of rsplitn)
        let prefix_cmp = a_split[1].cmp(&b_split[1]);
        if prefix_cmp != Ordering::Equal {
            return prefix_cmp;
        }

        let Ok(a_int) = a_split[0].parse::<i64>() else {
            return a_str.cmp(b_str);
        };
        let Ok(b_int) = b_split[0].parse::<i64>() else {
            return a_str.cmp(b_str);
        };
        a_int.cmp(&b_int)
    });

    options
}

#[derive(Copy, Clone, Debug)]
pub struct SuperShuckieReplayTimes {
    pub total_frames: u32,
    pub total_milliseconds: u32
}


/// Info of the replay file.
pub struct ReplayFileInfo {
    /// Name of the replay file being made
    pub final_replay_name: UTF8CString,

    /// Path of the replay file being made
    pub final_replay_path: PathBuf,

    /// Path to the temp file being recorded
    pub temp_replay_path: PathBuf
}

#[derive(Clone, Debug, Default)]
struct LastReadReplayCropData {
    start: Option<(UnsignedInteger, TimestampMillis)>,
    end: Option<(UnsignedInteger, TimestampMillis)>,
    timer_offset: Option<TimestampMillis>,
}

pub trait SuperShuckieFrontendCallbacks {
    fn refresh_screens(&mut self, screens: &[ScreenData]);
    fn change_video_mode(&mut self, screens: &[ScreenData], screen_scaling: NonZeroU8);
}

fn _ensure_callbacks_are_object_safe(_: Box<dyn SuperShuckieFrontendCallbacks>) {}

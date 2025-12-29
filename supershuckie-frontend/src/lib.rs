pub mod util;
pub mod settings;

use std::collections::BTreeMap;
use crate::settings::*;
use crate::util::UTF8CString;
use std::ffi::CStr;
use std::fs::File;
use std::io::Write;
use std::num::{NonZeroU64, NonZeroU8};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use supershuckie_core::emulator::{EmulatorCore, GameBoyColor, Input, Model, NullEmulatorCore, PartialReplayRecordMetadata, ScreenData};
use supershuckie_core::{ReplayPlayerAttachError, Speed, SuperShuckieRapidFire, ThreadedSuperShuckieCore};
use supershuckie_replay_recorder::replay_file::{ReplayConsoleType, ReplayHeaderBlake3Hash, ReplayPatchFormat};
use supershuckie_replay_recorder::ByteVec;
use supershuckie_replay_recorder::replay_file::playback::ReplayFilePlayer;
use supershuckie_replay_recorder::replay_file::record::ReplayFileRecorderSettings;

const SETTINGS_FILE: &str = "settings.json";
const SAVE_STATE_EXTENSION: &str = "save_state";
const SAVE_DATA_EXTENSION: &str = "sav";
const REPLAY_EXTENSION: &str = "replay";

pub type ConnectedControllerIndex = u32;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SuperShuckieEmulatorType {
    GameBoy,
    GameBoySGB2,
    GameBoyColor
}

pub enum UserInput {
    Keyboard { keycode: i32 },
    Button { controller: ConnectedControllerIndex, button: i32 },
    Axis { controller: ConnectedControllerIndex, axis: i32 }
}

pub struct SuperShuckieFrontend {
    core: ThreadedSuperShuckieCore,
    core_metadata: CoreMetadata,

    callbacks: Box<dyn SuperShuckieFrontendCallbacks>,

    user_dir: PathBuf,
    frame_count: u32,
    pokeabyte_error: Option<UTF8CString>,

    loaded_rom_data: Option<Vec<u8>>,

    current_input: Input,
    current_rapid_fire_input: Option<SuperShuckieRapidFire>,
    current_toggled_input: Option<Input>,
    current_save_state_history: Vec<Vec<u8>>,
    current_save_state_history_position: usize,

    connected_controllers: BTreeMap<ConnectedControllerIndex, UTF8CString>,

    rom_name: Option<Arc<UTF8CString>>,
    save_file: Option<Arc<UTF8CString>>,
    recording_replay_file: Option<ReplayFileInfo>,

    paused: bool,

    settings: Settings
}

impl SuperShuckieFrontend {
    pub fn new<P: AsRef<Path>>(user_dir: P, callbacks: Box<dyn SuperShuckieFrontendCallbacks>) -> Self {
        let user_dir = user_dir.as_ref().to_owned();

        // FIXME: Check this
        let settings = try_to_init_user_dir_and_get_settings(user_dir.as_ref()).expect("failed to init user_dir");

        let mut s = Self {
            core: ThreadedSuperShuckieCore::new(Box::new(NullEmulatorCore)),
            core_metadata: CoreMetadata { emulator_type: None },
            user_dir,
            rom_name: None,
            save_file: None,
            loaded_rom_data: None,
            frame_count: 0,
            current_rapid_fire_input: None,
            current_toggled_input: None,
            callbacks,
            settings,
            current_input: Input::default(),
            current_save_state_history: Vec::new(),
            current_save_state_history_position: 0,
            recording_replay_file: None,
            pokeabyte_error: None,
            paused: false,
            connected_controllers: BTreeMap::new()
        };

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
        if !self.is_game_running() {
            return Err("Game not running".into())
        }

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

        if self.settings.replay.auto_decompress_replays_upfront {
            player.decompress_all_blobs();
        }

        let current_emulator_type = self.core_metadata.emulator_type.expect("???? no emulator type when reloading a replay?");
        let metadata = player.get_replay_metadata();
        let expected_type = match metadata.console_type {
            ReplayConsoleType::GameBoy => SuperShuckieEmulatorType::GameBoy,
            ReplayConsoleType::SuperGameBoy2 => SuperShuckieEmulatorType::GameBoySGB2,
            ReplayConsoleType::GameBoyColor => SuperShuckieEmulatorType::GameBoyColor,
            _ => current_emulator_type
        };

        if current_emulator_type != expected_type {
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

        Ok(true)
    }

    /// Stop playing back any currently playing replay.
    #[inline]
    pub fn stop_replay_playback(&mut self) {
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

    pub fn on_user_input(&mut self, input: UserInput, value: f64) {
        let Some(control) = (match input {
            UserInput::Keyboard { keycode } => self.settings.controls.keyboard_controls.get(&keycode).copied(),
            UserInput::Button { button, controller } => {
                self.connected_controllers.get(&controller)
                    .and_then(|i| self.settings.controls.controller_controls.get(i.as_str()))
                    .and_then(|i| i.buttons.get(&button))
                    .copied()
            }
            UserInput::Axis { axis, controller } => {
                self.connected_controllers.get(&controller)
                    .and_then(|i| self.settings.controls.controller_controls.get(i.as_str()))
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
                    self.set_paused(!self.paused);
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

        let Some(filename) = path.file_name().and_then(|i| i.to_str()) else {
            return Err(format!(
                "{} does not appear to be a valid ROM file (missing filename)",
                path.display()
            ).into())
        };

        let Some(extension) = path.extension().and_then(|i| i.to_str()) else {
            return Err(format!("{filename} does not appear to be a valid ROM file (missing extension)").into())
        };

        let data = std::fs::read(path).map_err(|e| {
            format!("Failed to read ROM at {filename}: {e}")
        })?;

        let emulator_to_use = match extension.to_lowercase().as_str() {
            "gb" | "gbc" => self.choose_for_game_boy(data.as_slice()),
            unknown => return Err(format!("Unknown or unsupported ROM file type .{unknown}").into())
        };

        self.create_userdata_for_rom(filename)?;
        self.close_rom();
        self.loaded_rom_data = Some(data);
        self.rom_name = Some(Arc::new(UTF8CString::from_str(filename)));
        self.core_metadata.emulator_type = Some(emulator_to_use);
        self.save_file = Some(Arc::new(self.get_current_save_file_name_for_rom(filename)));
        self.reload_rom_in_place();
        Ok(())
    }

    /// Get the control settings.
    pub fn get_control_settings(&self) -> &Controls {
        &self.settings.controls
    }

    /// Overwrite the control settings.
    pub fn set_control_settings(&mut self, controls: Controls) {
        self.settings.controls = controls
    }

    /// Hard reset the console.
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

    fn reload_rom_in_place(&mut self) {
        let emulator_type = self.core_metadata.emulator_type.expect("reload_rom_in_place with no emulator type");
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
        self.core = core;
        self.after_switch_core();
        self.after_load_rom();
    }

    fn reset_save_state_history(&mut self) {
        self.current_save_state_history = Vec::new();
        self.current_save_state_history_position = 0;
    }

    fn make_new_core(&self, rom_data: &[u8], save_file: Option<Vec<u8>>, emulator_type: SuperShuckieEmulatorType) -> Box<dyn EmulatorCore> {
        let bios = self.get_bios_for_core(emulator_type);

        let mut core: Box<dyn EmulatorCore> = match emulator_type {
            SuperShuckieEmulatorType::GameBoy => Box::new(GameBoyColor::new_from_rom(rom_data, bios.as_slice(), Model::DmgB)),
            SuperShuckieEmulatorType::GameBoySGB2 => Box::new(GameBoyColor::new_from_rom(rom_data, bios.as_slice(), Model::Sgb2)),
            SuperShuckieEmulatorType::GameBoyColor => Box::new(GameBoyColor::new_from_rom(rom_data, bios.as_slice(), Model::Cgb0))
        };

        if let Some(sram) = save_file {
            let _ = core.load_sram(sram.as_slice()); // TODO: handle this?
        }

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
        // TODO: Let this be configurable.
        match emulator_kind {
            SuperShuckieEmulatorType::GameBoy | SuperShuckieEmulatorType::GameBoySGB2 => include_bytes!("../../bootrom/dmg/dmg.bin").to_vec(),
            SuperShuckieEmulatorType::GameBoyColor => include_bytes!("../../bootrom/cgb/cgb_boot/cgb_boot_fast.bin").to_vec()
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
        self.core = ThreadedSuperShuckieCore::new(Box::new(NullEmulatorCore));
        self.save_file = None;
        self.rom_name = None;
        self.core_metadata.emulator_type = None;
        self.current_input = Input::default();
        self.after_switch_core();
    }

    /// Set whether or not the game is paused.
    pub fn set_paused(&mut self, paused: bool) {
        // we still want to do this for config reasons
        self.paused = paused;

        if self.is_game_running() {
            if paused {
                self.core.pause();
            }
            else {
                self.core.start();
            }
        }
    }

    /// Set whether or not the game is paused temporarily.
    pub fn set_playback_frozen(&mut self, paused: bool) {
        self.core.set_playback_frozen(paused);
    }

    /// Get whether or not the game is manually paused
    pub fn is_paused(&self) -> bool {
        self.paused
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
        self.core_metadata.emulator_type.is_some()
    }

    /// Calls the `refresh_screens` callback regardless of if there's a new frame.
    #[inline]
    pub fn force_refresh_screens(&mut self) {
        self.refresh_screen(true);
    }

    /// Set the video scale.
    pub fn set_video_scale(&mut self, scale: NonZeroU8) {
        let old_scale = &mut self.settings.emulation.video_scale;
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

        self.reload_rom_in_place();
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
    pub fn tick(&mut self) {
        self.refresh_screen(false);
    }

    fn refresh_screen(&mut self, force: bool) {
        let current_frame_count = self.core.get_elapsed_frames();
        if force || current_frame_count == self.frame_count {
            return
        }

        self.frame_count = current_frame_count;
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
        self.core.get_elapsed_milliseconds()
    }

    /// Get the number of milliseconds elapsed.
    #[inline]
    pub fn get_elapsed_frames(&self) -> u32 {
        self.core.get_elapsed_frames()
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
    pub fn write_settings(&self) {
        // TODO: handle errors here?
        let _ = std::fs::write(self.user_dir.join(SETTINGS_FILE), serde_json::to_string_pretty(&self.settings).expect("failed to serialize"));
    }

    fn before_unload_or_reload_rom(&mut self) {
        self.reset_save_state_history();
        self.stop_recording_replay();
        self.pokeabyte_error = None;
    }

    /// Start recording a replay.
    ///
    /// If `name` is set, that name will be used.
    ///
    /// Returns the name of the replay if started.
    pub fn start_recording_replay(&mut self, name: Option<&str>) -> Result<UTF8CString, UTF8CString> {
        if !self.is_game_running() {
            return Err("Game not running".into())
        }

        let current_rom_name = self.get_current_rom_name_arc().expect("no rom name when game is running in start_recording_replay");
        let save_states_dir = self.get_replays_dir_for_rom(current_rom_name.as_str());

        let (final_file, final_replay, final_replay_path) = self.load_file_or_make_generic(&save_states_dir, name, None, REPLAY_EXTENSION)?;
        let (temp_file, _, temp_replay) = self.load_file_or_make_generic(&save_states_dir, name, Some("temp"), REPLAY_EXTENSION)?;

        if self.settings.replay.auto_pause_on_record {
            self.set_paused(true);
        }

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

            final_file,
            temp_file,
        });

        self.recording_replay_file = Some(ReplayFileInfo {
            final_replay_name: final_replay.clone().into(),
            temp_replay_path: temp_replay,
            final_replay_path
        });

        Ok(final_replay.into())
    }

    /// Stop recording replay.
    pub fn stop_recording_replay(&mut self) {
        let Some(replay_file) = self.recording_replay_file.take() else {
            return
        };

        // FIXME: We should make sure that it actually finalized here before deleting the temp file.
        let zero_frames = self.core.get_elapsed_frames() == 0;

        self.core.stop_recording_replay();
        let _ = std::fs::remove_file(&replay_file.temp_replay_path);

        if zero_frames {
            let _ = std::fs::remove_file(&replay_file.final_replay_path);
        }
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

    fn after_switch_core(&mut self) {
        self.update_video_mode();
    }

    fn update_video_mode(&mut self) {
        self.core.read_screens(|screens| {
            self.callbacks.change_video_mode(screens, self.settings.emulation.video_scale);
        });
    }

    fn after_load_rom(&mut self) {
        self.force_refresh_screens();
        self.current_input = Input::default();
        self.core.set_speed(Speed::from_multiplier_float(self.settings.emulation.base_speed_multiplier));
        if self.settings.pokeabyte.enabled {
            let _ = self.set_pokeabyte_enabled(true);
        }
        if !self.paused {
            self.core.start();
        }
    }

    #[inline]
    fn reset_speed(&mut self) {
        self.apply_turbo(0.0);
    }

    fn apply_turbo(&mut self, turbo: f64) {
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

    fn reload_game_boy_if_needed(&mut self) {
        let current = match self.core_metadata.emulator_type {
            Some(n) if matches!(n, SuperShuckieEmulatorType::GameBoy | SuperShuckieEmulatorType::GameBoyColor | SuperShuckieEmulatorType::GameBoySGB2) => n,
            _ => return
        };

        let Some(rom) = self.loaded_rom_data.as_ref() else {
            panic!("emulator_type is non-None but we have no loaded rom data???")
        };

        let expected = self.choose_for_game_boy(rom.as_slice());

        if expected != current {
            self.core_metadata.emulator_type = Some(expected);
            self.reload_rom_in_place();
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

    options
}

#[derive(Copy, Clone, Debug)]
pub struct SuperShuckieReplayTimes {
    pub total_frames: u32,
    pub total_milliseconds: u32
}

pub struct CoreMetadata {
    pub emulator_type: Option<SuperShuckieEmulatorType>
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

pub trait SuperShuckieFrontendCallbacks {
    fn refresh_screens(&mut self, screens: &[ScreenData]);
    fn change_video_mode(&mut self, screens: &[ScreenData], screen_scaling: NonZeroU8);
}

fn _ensure_callbacks_are_object_safe(_: Box<dyn SuperShuckieFrontendCallbacks>) {}

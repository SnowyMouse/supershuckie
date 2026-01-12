use std::ffi::{c_char, c_int, c_void, CStr};
use std::mem::MaybeUninit;
use std::num::NonZeroU8;
use std::ptr::null;
use std::slice::from_raw_parts_mut;
use supershuckie_core::emulator::{ScreenData, ScreenDataEncoding};
use supershuckie_frontend::{ConnectedControllerIndex, SuperShuckieEmulatorType, SuperShuckieFrontend, SuperShuckieFrontendCallbacks, SuperShuckieReplayState, UserInput};
use supershuckie_frontend::settings::{GameBoyMode, NintendoDSDate};
use supershuckie_frontend::util::UTF8CString;
use crate::control_settings::SuperShuckieControlSettings;
use crate::string_array::SuperShuckieStringArray;

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct SuperShuckieScreenDataC {
    pub width: u32,
    pub height: u32,
    pub screen_data_encoding: ScreenDataEncoding
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct SuperShuckieFrontendCallbacksC {
    pub userdata: *mut c_void,

    pub refresh_screens: Option<unsafe extern "C" fn(userdata: *mut c_void, screen_count: usize, screen_data: *const *const u32)>,
    pub change_video_mode: Option<unsafe extern "C" fn(userdata: *mut c_void, screen_count: usize, screen_data: *const SuperShuckieScreenDataC, screen_scale: NonZeroU8)>,
}

impl SuperShuckieFrontendCallbacks for SuperShuckieFrontendCallbacksC {
    fn refresh_screens(&mut self, screens: &[ScreenData]) {
        let Some(s) = self.refresh_screens else { return };

        let mut screens_buf = [null(); 4];
        for (index, screen) in screens.iter().enumerate() {
            screens_buf[index] = screen.pixels.as_ptr();
        }

        unsafe { s(self.userdata, screens.len(), screens_buf.as_ptr()) };
    }

    fn change_video_mode(&mut self, screens: &[ScreenData], scaling: NonZeroU8) {
        let Some(s) = self.change_video_mode else { return };

        let mut screens_buf = [MaybeUninit::<SuperShuckieScreenDataC>::uninit(); 4];
        for (index, screen) in screens.iter().enumerate() {
            screens_buf[index].write(SuperShuckieScreenDataC {
                width: screen.width as u32,
                height: screen.height as u32,
                screen_data_encoding: screen.encoding
            });
        }

        unsafe { s(self.userdata, screens.len(), screens_buf.as_ptr() as *const SuperShuckieScreenDataC, scaling) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_new(
    data_dir: *const c_char,
    config_dir: *const c_char,
    callbacks: &SuperShuckieFrontendCallbacksC
) -> *mut SuperShuckieFrontend {
    let data_dir = unsafe { CStr::from_ptr(data_dir) }
        .to_str()
        .expect("data_dir is not UTF-8");
    let config_file = unsafe { CStr::from_ptr(config_dir) }
        .to_str()
        .expect("config_file is not UTF-8");

    Box::into_raw(Box::new(SuperShuckieFrontend::new(data_dir, config_file, Box::new(*callbacks))))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_key_press(
    frontend: &mut SuperShuckieFrontend,
    keycode: i32,
    pressed: bool
) {
    frontend.on_user_input(UserInput::Keyboard { keycode }, pressed.then_some(1.0).unwrap_or(0.0));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_button_press(
    frontend: &mut SuperShuckieFrontend,
    controller: ConnectedControllerIndex,
    button: i32,
    pressed: bool
) {
    frontend.on_user_input(UserInput::Button { controller, button }, pressed.then_some(1.0).unwrap_or(0.0));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_axis(
    frontend: &mut SuperShuckieFrontend,
    controller: ConnectedControllerIndex,
    axis: i32,
    value: f64
) {
    frontend.on_user_input(UserInput::Axis { controller, axis }, value);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_set_paused(
    frontend: &mut SuperShuckieFrontend,
    paused: bool
) {
    frontend.set_paused(paused);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_tick(
    frontend: &mut SuperShuckieFrontend,
    error: *mut u8,
    error_len: usize
) -> bool {
    if let Err(e) = frontend.tick() {
        write_str_to_data(e.as_str(), unsafe { from_raw_parts_mut(error, error_len) });
        false
    }
    else {
        true
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_load_rom(
    frontend: &mut SuperShuckieFrontend,
    path: *const c_char,
    error: *mut u8,
    error_len: usize
) -> bool {
    let path = unsafe { CStr::from_ptr(path) };
    if error_len > 0 && let Err(e) = frontend.load_rom(path.to_str().expect("supershuckie_frontend_load_rom with non-UTF-8 path")) {
        write_str_to_data(e.as_str(), unsafe { from_raw_parts_mut(error, error_len) });
        false
    }
    else {
        true
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_close_rom(
    frontend: &mut SuperShuckieFrontend
) {
    let _ = frontend.close_rom();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_unload_rom(
    frontend: &mut SuperShuckieFrontend
) {
    frontend.unload_rom();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_load_or_create_save_file(
    frontend: &mut SuperShuckieFrontend,
    save_file: *const c_char,
    initialize: bool
) {
    let save_file = unsafe { CStr::from_ptr(save_file) }.to_str().expect("save file not utf-8");
    frontend.load_or_create_save_file(save_file, initialize);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_set_current_save_file(
    frontend: &mut SuperShuckieFrontend,
    save_file: *const c_char
) {
    let save_file = unsafe { CStr::from_ptr(save_file) }.to_str().expect("save file not utf-8");
    frontend.set_current_save_file(save_file);
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_hard_reset_console(
    frontend: &mut SuperShuckieFrontend
) {
    frontend.hard_reset_console();
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_is_game_running(
    frontend: &SuperShuckieFrontend
) -> bool {
    frontend.is_game_running()
}

fn write_str_to_data(string: &str, buffer: &mut [u8]) {
    if buffer.is_empty() {
        return
    }
    buffer.fill(0);

    let buffer_length = buffer.len();
    let mut buffer_usable = &mut buffer[0..buffer_length - 1]; // need the last byte to be null terminated
    if buffer_usable.is_empty() {
        return
    }

    let mut char_data = [0u8; 4];
    for c in string.chars() {
        let bytes = c.encode_utf8(&mut char_data).as_bytes();
        let Some((a, b)) = buffer_usable.split_at_mut_checked(bytes.len()) else {
            return
        };
        a.copy_from_slice(bytes);
        buffer_usable = b;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_force_refresh_screens(
    frontend: &mut SuperShuckieFrontend
) {
    frontend.force_refresh_screens();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_set_video_scale(
    frontend: &mut SuperShuckieFrontend,
    scale: u8
) {
    frontend.set_video_scale(NonZeroU8::new(scale).unwrap_or(unsafe { NonZeroU8::new_unchecked(1) }));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_custom_setting(
    frontend: &SuperShuckieFrontend,
    setting: *const c_char
) -> *const c_char {
    frontend.get_custom_setting(unsafe { CStr::from_ptr(setting) }.to_str().expect("supershuckie_frontend_get_custom_setting bad setting"))
        .map(|i| i.as_c_str().as_ptr())
        .unwrap_or(null())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_start_recording_replay(
    frontend: &mut SuperShuckieFrontend,
    name: *const c_char,
    result: *mut u8,
    result_len: usize
) -> bool {
    let name = if !name.is_null() { Some(unsafe { CStr::from_ptr(name) }.to_str().expect("name not UTF-8")) } else { None };
    let (success, msg) = match frontend.start_recording_replay(name) {
        Ok(n) => (true, n),
        Err(n) => (false, n)
    };

    write_str_to_data(msg.as_str(), unsafe { from_raw_parts_mut(result, result_len) });
    success
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_stop_recording_replay(
    frontend: &mut SuperShuckieFrontend
) {
    frontend.stop_recording_replay();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_recording_replay_file(
    frontend: &SuperShuckieFrontend
) -> *const c_char {
    frontend.get_replay_file_info().map(|i| i.final_replay_name.as_c_str().as_ptr()).unwrap_or(null())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_create_save_state(
    frontend: &mut SuperShuckieFrontend,
    name: *const c_char,
    result: *mut u8,
    result_len: usize
) -> bool {
    let name = if !name.is_null() { Some(unsafe { CStr::from_ptr(name) }.to_str().expect("name not UTF-8")) } else { None };
    let (success, msg) = match frontend.create_save_state(name) {
        Ok(n) => (true, n),
        Err(n) => (false, n)
    };

    write_str_to_data(msg.as_str(), unsafe { from_raw_parts_mut(result, result_len) });
    success
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_undo_load_save_state(
    frontend: &mut SuperShuckieFrontend
) -> bool {
    frontend.undo_load_save_state()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_redo_load_save_state(
    frontend: &mut SuperShuckieFrontend
) -> bool {
    frontend.redo_load_save_state()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_load_save_state(
    frontend: &mut SuperShuckieFrontend,
    name: *const c_char,
    error: *mut u8,
    error_len: usize
) -> bool {
    let name = unsafe { CStr::from_ptr(name) }.to_str().expect("name not UTF-8");
    match frontend.load_save_state_if_exists(name) {
        Ok(true) => true,
        Ok(false) => {
            if error_len >= 1 {
                unsafe { *error = 0 };
            }
            false
        }
        Err(_) if error_len == 0 => false,
        Err(e) => {
            write_str_to_data(e.as_str(), unsafe { from_raw_parts_mut(error, error_len) });
            false
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_is_pokeabyte_enabled(
    frontend: &mut SuperShuckieFrontend,
    error: *mut u8,
    error_len: usize
) -> bool {
    match frontend.is_pokeabyte_enabled() {
        Ok(n) => {
            unsafe { *error = 0 };
            n
        },
        Err(e) => {
            write_str_to_data(e.as_str(), unsafe { from_raw_parts_mut(error, error_len) });
            false
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_external_commands_enabled(
    frontend: &mut SuperShuckieFrontend,
    error: *mut u8,
    error_len: usize
) -> bool {
    match frontend.get_external_commands_enabled() {
        Ok(n) => {
            unsafe { *error = 0 };
            n
        },
        Err(e) => {
            write_str_to_data(e.as_str(), unsafe { from_raw_parts_mut(error, error_len) });
            false
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_is_paused(
    frontend: &SuperShuckieFrontend
) -> bool {
    frontend.is_paused()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_set_pokeabyte_enabled(
    frontend: &mut SuperShuckieFrontend,
    enabled: bool,
    error: *mut u8,
    error_len: usize
) -> bool {
    match frontend.set_pokeabyte_enabled(enabled) {
        Ok(_) => true,
        Err(e) => {
            write_str_to_data(e.as_str(), unsafe { from_raw_parts_mut(error, error_len) });
            false
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_set_external_commands_enabled(
    frontend: &mut SuperShuckieFrontend,
    enabled: bool,
    error: *mut u8,
    error_len: usize
) -> bool {
    match frontend.set_external_commands_enabled(enabled) {
        Ok(_) => true,
        Err(e) => {
            write_str_to_data(e.as_str(), unsafe { from_raw_parts_mut(error, error_len) });
            false
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_auto_stop_playback_on_input_setting(
    frontend: &mut SuperShuckieFrontend,
    new_setting: bool
) {
    frontend.set_auto_stop_playback_on_input_setting(new_setting);
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_auto_stop_playback_on_input_setting(frontend: &SuperShuckieFrontend) -> bool {
    frontend.get_auto_stop_playback_on_input_setting()
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_auto_unpause_on_input_setting(
    frontend: &mut SuperShuckieFrontend,
    new_setting: bool
) {
    frontend.set_auto_unpause_on_input_setting(new_setting);
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_auto_unpause_on_input_setting(frontend: &SuperShuckieFrontend) -> bool {
    frontend.get_auto_unpause_on_input_setting()
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_auto_pause_on_record_setting(
    frontend: &mut SuperShuckieFrontend,
    new_setting: bool
) {
    frontend.set_auto_pause_on_record_setting(new_setting);
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_auto_pause_on_record_setting(frontend: &SuperShuckieFrontend) -> bool {
    frontend.get_auto_pause_on_record_setting()
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_auto_decompress_replays_upfront_setting(
    frontend: &mut SuperShuckieFrontend,
    new_setting: bool
) {
    frontend.set_auto_decompress_replays_upfront_setting(new_setting);
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_auto_decompress_replays_upfront_setting(frontend: &SuperShuckieFrontend) -> bool {
    frontend.get_auto_decompress_replays_upfront_setting()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_save_sram(
    frontend: &mut SuperShuckieFrontend,
    error: *mut u8,
    error_len: usize
) -> bool {
    match frontend.save_sram() {
        Ok(_) => true,
        Err(_) if error_len == 0 => false,
        Err(e) => {
            write_str_to_data(e.as_str(), unsafe { from_raw_parts_mut(error, error_len) });
            false
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_set_custom_setting(
    frontend: &mut SuperShuckieFrontend,
    setting: *const c_char,
    value: *const c_char
) {
    frontend.set_custom_setting(
        unsafe { CStr::from_ptr(setting) }.to_str().expect("supershuckie_frontend_set_custom_setting bad setting"),
        if value.is_null() {
            None
        }
        else {
            Some(UTF8CString::from_cstr(unsafe { CStr::from_ptr(value) }))
        }
    );
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_rom_name(
    frontend: &SuperShuckieFrontend
) -> *const c_char {
    frontend.get_current_rom_name_c_str().map(|i| i.as_ptr()).unwrap_or(null())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_write_settings(
    frontend: &SuperShuckieFrontend
) {
    frontend.write_config();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_speed_settings(
    frontend: &SuperShuckieFrontend,
    base: *mut f64,
    turbo: *mut f64
) {
    let base = unsafe { nullable_reference!(base) };
    let turbo = unsafe { nullable_reference!(turbo) };
    frontend.get_speed_settings(base, turbo);
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_speed_settings(
    frontend: &mut SuperShuckieFrontend,
    base: f64,
    turbo: f64
) {
    frontend.set_speed_settings(base, turbo);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_free(
    frontend: *mut SuperShuckieFrontend
) {
    if !frontend.is_null() {
        let _ = unsafe { Box::from_raw(frontend) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_all_replays_for_rom(
    frontend: &SuperShuckieFrontend,
    rom: *const c_char
) -> *mut SuperShuckieStringArray {
    let array = match unsafe { current_rom_or_null(frontend, rom) } {
        Some(rom) => SuperShuckieStringArray(frontend.get_all_replays_for_rom(rom)),
        None => SuperShuckieStringArray::default()
    };
    Box::into_raw(Box::new(array))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_all_saves_for_rom(
    frontend: &SuperShuckieFrontend,
    rom: *const c_char
) -> *mut SuperShuckieStringArray {
    let array = match unsafe { current_rom_or_null(frontend, rom) } {
        Some(rom) => SuperShuckieStringArray(frontend.get_all_saves_for_rom(rom)),
        None => SuperShuckieStringArray::default()
    };
    Box::into_raw(Box::new(array))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_all_save_states_for_rom(
    frontend: &SuperShuckieFrontend,
    rom: *const c_char
) -> *mut SuperShuckieStringArray {
    let array = match unsafe { current_rom_or_null(frontend, rom) } {
        Some(rom) => SuperShuckieStringArray(frontend.get_all_save_states_for_rom(rom)),
        None => SuperShuckieStringArray::default()
    };
    Box::into_raw(Box::new(array))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_elapsed_time(
    frontend: &SuperShuckieFrontend,
    elapsed_frames: *mut u32,
    elapsed_milliseconds: *mut u32
) {
    let elapsed_frames = unsafe { nullable_reference!(elapsed_frames) };
    let elapsed_milliseconds = unsafe { nullable_reference!(elapsed_milliseconds) };

    *elapsed_milliseconds = frontend.get_elapsed_milliseconds();
    *elapsed_frames = frontend.get_elapsed_frames();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_replay_playback_time(
    frontend: &SuperShuckieFrontend,
    total_frames: *mut u32,
    total_milliseconds: *mut u32
) -> bool {
    let total_frames = unsafe { nullable_reference!(total_frames) };
    let total_milliseconds = unsafe { nullable_reference!(total_milliseconds) };

    match frontend.get_replay_playback_stats() {
        Some(n) => {
            *total_frames = n.total_frames;
            *total_milliseconds = n.total_milliseconds;
            true
        },
        None => {
            *total_frames = 0;
            *total_milliseconds = 0;
            false
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_load_replay(
    frontend: &mut SuperShuckieFrontend,
    name: *const c_char,
    override_errors: bool,
    error: *mut u8,
    error_len: usize
) -> bool {
    let name = unsafe { CStr::from_ptr(name).to_str().expect("replay name is not UTF-8") };

    match frontend.load_replay_if_exists(name, override_errors) {
        Ok(_) => true,
        Err(e) => {
            write_str_to_data(e.as_str(), unsafe { from_raw_parts_mut(error, error_len) });
            false
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_stop_replay_playback(
    frontend: &mut SuperShuckieFrontend
) {
    frontend.stop_replay_playback();
}

unsafe fn current_rom_or_null(frontend: &SuperShuckieFrontend, rom: *const c_char) -> Option<&str> {
    if rom.is_null() {
        frontend.get_current_rom_name()
    }
    else {
        Some(unsafe { CStr::from_ptr(rom) }.to_str().expect("save file not utf-8"))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_control_settings(
    frontend: &SuperShuckieFrontend,
    emulator_type: u8
) -> *mut SuperShuckieControlSettings {
    let Ok(emulator_type) = SuperShuckieEmulatorType::try_from(emulator_type) else { panic!("Unknown emulator_type {emulator_type}") };
    Box::into_raw(Box::new(SuperShuckieControlSettings(frontend.get_control_settings(emulator_type).clone())))
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_control_settings(
    frontend: &mut SuperShuckieFrontend,
    settings: &SuperShuckieControlSettings,
    emulator_type: u8
) {
    let Ok(emulator_type) = SuperShuckieEmulatorType::try_from(emulator_type) else { panic!("Unknown emulator_type {emulator_type}") };
    frontend.set_control_settings(settings.0.clone(), emulator_type)
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_connected_controllers(
    frontend: &SuperShuckieFrontend
) -> *mut SuperShuckieStringArray {
    Box::into_raw(Box::new(SuperShuckieStringArray(frontend.get_connected_controllers())))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_connect_controller(
    frontend: &mut SuperShuckieFrontend,
    controller: *const c_char
) -> ConnectedControllerIndex {
    let controller_name = unsafe { CStr::from_ptr(controller).to_str().expect("controller name not UTF-8") };
    frontend.connect_controller(controller_name)
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_disconnect_controller(
    frontend: &mut SuperShuckieFrontend,
    controller: ConnectedControllerIndex
) {
    frontend.disconnect_controller(controller);
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_name_of_controller(
    frontend: &SuperShuckieFrontend,
    controller: ConnectedControllerIndex
) -> *const c_char {
    frontend.name_of_controller_c_str(controller).map(|i| i.as_ptr()).unwrap_or(null())
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_playback_frame(
    frontend: &mut SuperShuckieFrontend,
    frame: u32
) {
    frontend.go_to_replay_frame(frame)
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_advance_playback_frames(
    frontend: &mut SuperShuckieFrontend,
    frames: i32
) {
    frontend.advance_playback_frames(frames)
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_playback_frozen(
    frontend: &mut SuperShuckieFrontend,
    paused: bool
) {
    frontend.set_playback_frozen(paused)
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_replay_state(
    frontend: &SuperShuckieFrontend
) -> SuperShuckieReplayState {
    frontend.get_replay_state()
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_gbc_mode(frontend: &SuperShuckieFrontend) -> GameBoyMode {
    frontend.get_gbc_mode()
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_gbc_mode(frontend: &mut SuperShuckieFrontend, mode: u32) {
    if let Ok(m) = GameBoyMode::try_from(mode) {
        frontend.set_gbc_mode(m)
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_is_sgb_enabled(frontend: &SuperShuckieFrontend) -> bool {
    frontend.is_sgb_enabled()
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_sgb_enabled(frontend: &mut SuperShuckieFrontend, enabled: bool) {
    frontend.set_sgb_enabled(enabled);
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_touch(frontend: &mut SuperShuckieFrontend, enabled: bool, x: u8, y: u8) {
    frontend.set_touch(enabled.then_some((x, y)))
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_nds_date(frontend: &SuperShuckieFrontend, date: &mut NintendoDSDate) {
    *date = frontend.get_nds_date().get_cleaned();
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_nds_date(frontend: &mut SuperShuckieFrontend, date: &NintendoDSDate) {
    frontend.set_nds_date(*date);
}


#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_nds_jit(frontend: &SuperShuckieFrontend) -> bool {
    frontend.get_jit_enabled()
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_set_nds_jit(frontend: &mut SuperShuckieFrontend, enabled: bool) {
    frontend.set_jit_enabled(enabled)
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_recent_roms(frontend: &SuperShuckieFrontend) -> *mut SuperShuckieStringArray {
    Box::into_raw(Box::new(SuperShuckieStringArray(frontend.get_recent_roms())))
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_clear_recent_roms(frontend: &mut SuperShuckieFrontend) {
    frontend.clear_recent_roms()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_frontend_get_current_data_directory(
    frontend: &SuperShuckieFrontend,
    dir: *mut u8,
    dir_len: usize
) -> usize {
    let path = frontend.get_dir_for_current_rom().unwrap_or_else(|| frontend.get_user_dir());
    let path_bytes = path.as_c_str().to_bytes_with_nul();
    let path_bytes_len = path_bytes.len();

    if dir_len >= path_bytes_len {
        unsafe { from_raw_parts_mut(dir, path_bytes_len) }.copy_from_slice(path_bytes)
    }

    path_bytes_len
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_reload_core(
    frontend: &mut SuperShuckieFrontend
) {
    frontend.reload_core();
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_emulator_type(
    frontend: &SuperShuckieFrontend
) -> c_int {
    frontend.get_emulator_type().map(|i| i as c_int).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_get_emulator_type_name(
    emulator_type: u8
) -> *const c_char {
    match SuperShuckieEmulatorType::try_from(emulator_type) {
        Ok(n) => n.name_cstr().as_ptr(),
        _ => null()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_frontend_emulator_type_uses_shared_config(
    emulator_type: u8
) -> bool {
    match SuperShuckieEmulatorType::try_from(emulator_type) {
        Ok(n) => n.uses_shared_config(),
        _ => false
    }
}

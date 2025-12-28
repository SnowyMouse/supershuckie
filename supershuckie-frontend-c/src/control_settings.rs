use std::ffi::{c_char, CStr};
use std::ptr::null;
use std::slice::from_raw_parts_mut;
use supershuckie_frontend::settings::{Control, ControlModifier, ControlSetting, ControllerSettings, Controls};

pub struct SuperShuckieControlSettings(pub Controls);

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_control_settings_modifier_name(
    modifier: u32
) -> *const c_char {
    ControlModifier::try_from(modifier).map(|i| i.as_c_str().as_ptr()).unwrap_or(null())
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_control_settings_control_name(
    control: u32
) -> *const c_char {
    Control::try_from(control).map(|i| i.as_c_str().as_ptr()).unwrap_or(null())
}

#[unsafe(no_mangle)]
pub extern "C" fn supershuckie_control_settings_control_is_button(
    control: u32
) -> bool {
    Control::try_from(control).map(|i| i.is_button()).unwrap_or(false)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_control_settings_clear_controls_for_device(
    settings: &mut SuperShuckieControlSettings,

    device_name: *const c_char,
    control: u32,
    modifier: u32
) {
    let Ok(control) = Control::try_from(control) else { return };
    let Ok(modifier) = ControlModifier::try_from(modifier) else { return };

    let retain_fn = |_: &i32, control_setting: &mut ControlSetting| {
        control_setting.control != control || control_setting.modifier != modifier
    };

    if device_name.is_null() {
        settings.0.keyboard_controls.retain(retain_fn);
    }
    else {
        let device_name = unsafe { CStr::from_ptr(device_name).to_str().expect("device name not UTF-8") };
        let Some(s) = settings.0.controller_controls.get_mut(device_name) else {
            return
        };
        s.buttons.retain(retain_fn);
        s.axis.retain(retain_fn);
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_control_settings_get_controls_for_device(
    settings: &SuperShuckieControlSettings,

    device_name: *const c_char,
    is_axis: bool,

    control: u32,
    modifier: u32,

    input_codes: *mut i32,
    input_codes_count: usize
) -> usize {
    if device_name.is_null() && is_axis {
        return 0
    }

    let Ok(control) = Control::try_from(control) else { return 0 };
    let Ok(modifier) = ControlModifier::try_from(modifier) else { return 0 };

    let mut count = 0usize;
    let key_codes = if input_codes_count == 0 { &mut [] } else { unsafe { from_raw_parts_mut(input_codes, input_codes_count) } };

    let map = if device_name.is_null() { &settings.0.keyboard_controls } else {
        let device_name = unsafe { CStr::from_ptr(device_name).to_str().expect("device name not UTF-8") };
        match settings.0.controller_controls.get(device_name) {
            Some(n) => if is_axis { &n.axis } else { &n.buttons },
            None => return 0
        }
    };

    for (code, setting) in map {
        if setting.control == control && setting.modifier == modifier {
            if let Some(c) = key_codes.get_mut(count) {
                *c = *code;
            }
            count += 1;
        }
    }

    count
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_control_settings_set_control_for_device(
    settings: &mut SuperShuckieControlSettings,

    device_name: *const c_char,
    is_axis: bool,

    code: i32,
    control: u32,
    modifier: u32,
) {
    if device_name.is_null() && is_axis {
        panic!("No axis support for keyboards");
    }

    let Ok(control) = Control::try_from(control) else { panic!("Unknown control {control}") };
    let Ok(modifier) = ControlModifier::try_from(modifier) else { panic!("Unknown modifier {modifier}") };

    if !control.is_button() && modifier != ControlModifier::Normal {
        panic!("{control:?} cannot have non-normal modifiers (not a button)")
    }

    let map = loop {
        let map = if device_name.is_null() { &mut settings.0.keyboard_controls } else {
            let device_name = unsafe { CStr::from_ptr(device_name).to_str().expect("device name not UTF-8") };
            match settings.0.controller_controls.get_mut(device_name) {
                Some(n) => if is_axis { &mut n.axis } else { &mut n.buttons },
                None => {
                    settings.0.controller_controls.insert(device_name.to_owned(), ControllerSettings::default());
                    continue;
                }
            }
        };
        break map;
    };

    map.insert(code, ControlSetting { control, modifier });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn supershuckie_control_settings_free(
    settings: *mut SuperShuckieControlSettings
) {
    if !settings.is_null() {
        let _ = unsafe { Box::from_raw(settings) };
    }
}

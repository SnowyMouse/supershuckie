use alloc::vec::Vec;
use spin::Lazy;
use crate::emulator::{EmulatorCore, Input, RunTime, ScreenData, ScreenDataEncoding};
use alloc::string::String;
use supershuckie_replay_recorder::replay_file::{ReplayConsoleType, ReplayHeaderBlake3Hash};

/// An emulator that does nothing.
///
/// It has a single screen that is empty.
pub struct NullEmulatorCore;

static NULL_EMULATOR_SCREEN: Lazy<ScreenData> = Lazy::new(|| {
    let width = 160;
    let height = 144;

    ScreenData {
        pixels: alloc::vec![0xFF000000; width * height],
        width,
        height,
        encoding: ScreenDataEncoding::A8R8G8B8
    }
});

#[allow(unused_variables)]
impl EmulatorCore for NullEmulatorCore {
    fn run(&mut self) -> RunTime {
        RunTime {
            frames: 0
        }
    }

    fn run_unlocked(&mut self) -> RunTime {
        self.run()
    }

    fn read_ram(&self, address: u32, into: &mut [u8]) -> Result<(), &'static str> {
        Err("unsupported")
    }

    fn write_ram(&mut self, address: u32, from: &[u8]) -> Result<(), &'static str> {
        Err("unsupported")
    }

    fn set_speed(&mut self, speed: f64) {

    }

    fn save_sram(&self) -> Vec<u8> {
        Vec::new()
    }

    fn create_save_state(&self) -> Vec<u8> {
        Vec::new()
    }

    fn load_save_state(&mut self, state: &[u8]) -> Result<(), String> {
        Ok(())
    }

    fn encode_input(&self, input: Input, into: &mut Vec<u8>) {
        into.clear();
    }

    fn set_input_encoded(&mut self, input: &[u8]) {

    }

    fn get_screens(&self) -> &[ScreenData] {
        core::slice::from_ref(&*NULL_EMULATOR_SCREEN)
    }

    fn swap_screen_data(&mut self, screens: &mut [ScreenData]) {
        screens.fill(NULL_EMULATOR_SCREEN.clone())
    }

    fn hard_reset(&mut self) {
        
    }

    fn replay_console_type(&self) -> Option<ReplayConsoleType> {
        None
    }

    fn rom_checksum(&self) -> &ReplayHeaderBlake3Hash {
        &const { unsafe { core::mem::zeroed() } }
    }

    fn bios_checksum(&self) -> &ReplayHeaderBlake3Hash {
        &const { unsafe { core::mem::zeroed() } }
    }

    fn core_name(&self) -> &'static str {
        "Null"
    }

    fn is_null(&self) -> bool {
        true
    }
}

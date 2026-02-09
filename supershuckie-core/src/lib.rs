//! TODO
#![no_std]
#![warn(missing_docs)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

use crate::emulator::{EmulatorCore, Input, PartialReplayRecordMetadata, RunTime};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{Display, Formatter};
use core::num::NonZeroU64;
use alloc::collections::BTreeMap;
use supershuckie_replay_recorder::replay_file::playback::{ReplayFilePlayer, ReplaySeekError};
use supershuckie_replay_recorder::replay_file::record::{NonBlockingReplayFileRecorder, ReplayFileRecorder, ReplayFileRecorderFns, ReplayFileSink, ReplayFileWriteError};
use supershuckie_replay_recorder::replay_file::{blake3_hash_to_ascii, ReplayFileMetadata, ReplayHeaderBlake3Hash, ReplayPatchFormat};
use supershuckie_replay_recorder::{ByteVec, Packet, SignedInteger, TimestampMillis, UnsignedInteger};

pub mod emulator;

pub use supershuckie_replay_recorder::Speed;

#[cfg(feature = "std")]
mod thread;

#[cfg(feature = "std")]
pub use thread::*;

/// Wrapper for [`EmulatorCore`] that provides useful desktop emulator functionality.
pub struct SuperShuckieCore {
    core: Box<dyn EmulatorCore>,
    replay_file_recorder: Option<Box<dyn ReplayFileRecorderFns>>,
    replay_counters: Option<BTreeMap<String, SignedInteger>>,

    timestamp_provider: Box<dyn MonotonicTimestampProvider>,

    replay_player: Option<ReplayFilePlayer>,

    /// The current user-defined input.
    base_input: Input,

    /// The input to apply next frame.
    next_input: Option<Input>,

    /// Rapid fire input, if any.
    ///
    /// This input is applied every interval for a set number of frames.
    rapid_fire_input: Option<SuperShuckieRapidFire>,

    /// Queued writes, if any
    writes: Vec<QueuedWrite>,

    /// Toggled input, if any.
    ///
    /// This input is always applied.
    toggled_input: Option<Input>,

    /// The "total" input that was actually applied.
    current_input: Input,

    mid_frame: bool,
    replay_stalled: bool,

    input_scratch_buffer: Vec<u8>,
    starting_milliseconds: TimestampMillis,
    total_milliseconds: TimestampMillis,
    paused_timer_at: Option<TimestampMillis>,
    game_speed: Speed,
    replay_playback_speed: Speed,

    frames_since_last_keyframe: u64,
    frames_per_keyframe: u64,
    total_frames: u64,
    ignore_speed_changes_in_replays: bool,
    auto_resync_keyframes_in_replays: bool
}

#[derive(Clone, Debug)]
struct QueuedWrite {
    address: u32,
    data: ByteVec
}

/// Defines parameters for rapid fire.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct SuperShuckieRapidFire {
    /// Input state to use.
    pub input: Input,

    /// Number of frames the button(s) are held down between intervals.
    ///
    /// Note that when rapid fire is enabled, the button will be held down immediately for this many
    /// frames.
    pub hold_length: NonZeroU64,

    /// Number of frames the button(s) are released between intervals.
    pub interval: NonZeroU64,

    /// The current stage of the duty cycle.
    current_frame: u64,

    /// The sum of hold_length + interval.
    total_frames: u64,
}

impl Default for SuperShuckieRapidFire {
    fn default() -> Self {
        Self {
            input: Input::default(),
            hold_length: NonZeroU64::new(1).unwrap(),
            interval: NonZeroU64::new(1).unwrap(),
            current_frame: 0,
            total_frames: 0
        }
    }
}

impl SuperShuckieCore {
    /// Wrap `emulator_core`.
    pub fn new(emulator_core: Box<dyn EmulatorCore>, mut timestamp_provider: Box<dyn MonotonicTimestampProvider>) -> Self {
        Self {
            replay_file_recorder: None,
            base_input: Input::default(),
            next_input: None,
            rapid_fire_input: None,
            writes: Vec::new(),
            toggled_input: None,
            current_input: Default::default(),
            mid_frame: false,
            input_scratch_buffer: Vec::new(),
            total_milliseconds: 0.into(),
            starting_milliseconds: timestamp_provider.get_timestamp_milliseconds().into(),
            game_speed: Default::default(),
            replay_playback_speed: Default::default(),
            frames_since_last_keyframe: 0,
            frames_per_keyframe: 0,
            total_frames: 0,
            replay_player: None,
            replay_stalled: false,
            paused_timer_at: None,
            replay_counters: None,
            core: emulator_core,
            timestamp_provider,
            ignore_speed_changes_in_replays: false,
            auto_resync_keyframes_in_replays: false
        }
    }

    /// Run the emulator core for the shortest amount of time.
    pub fn run(&mut self) {
        self.do_run_fn(EmulatorCore::run);
    }

    /// Run the emulator core for the shortest amount of time without any timekeeping.
    pub fn run_unlocked(&mut self) {
        self.do_run_fn(EmulatorCore::run_unlocked);
    }

    /// Get the current replay counters.
    pub fn get_replay_counters(&self) -> Option<&BTreeMap<String, SignedInteger>> {
        self.replay_counters.as_ref()
    }

    fn do_run_fn(&mut self, run_fn: fn(&mut dyn EmulatorCore) -> RunTime) {
        if !self.replay_stalled {
            self.before_run();
        }

        if !self.replay_stalled {
            let time = run_fn(Box::as_mut(&mut self.core));
            self.after_run(&time);
        }
    }

    /// Run unlocked until the next frame.
    pub fn finish_current_frame(&mut self) {
        while self.mid_frame && !self.replay_stalled {
            self.run_unlocked();
        }
    }

    /// Enqueue a write for the next frame.
    pub fn enqueue_write(&mut self, address: u32, data: ByteVec) {
        self.writes.push(QueuedWrite { address, data });
        self.flush_writes();
    }

    /// Pause the current timer.
    pub fn pause_timer(&mut self) {
        self.paused_timer_at = Some((self.total_milliseconds.0 + self.starting_milliseconds.0).into());
    }

    /// Unpause the current timer if it is currently paused.
    pub fn unpause_timer(&mut self) {
        let Some(paused_time) = self.paused_timer_at.take() else {
            return
        };
        let unpaused_time = self.timestamp_provider.get_timestamp_milliseconds();

        self.starting_milliseconds = self.starting_milliseconds.0.wrapping_add(unpaused_time.wrapping_sub(paused_time.0)).into();
    }

    fn restart_timer(&mut self) {
        self.paused_timer_at = None;
        self.starting_milliseconds = self.timestamp_provider.get_timestamp_milliseconds().into();
        self.total_milliseconds = 0.into();
        self.total_frames = 0;
    }

    /// Get an immutable reference to the underlying core.
    pub fn get_core(&self) -> &dyn EmulatorCore {
        self.core.as_ref()
    }

    /// Set the speed multiplier of the game.
    pub fn set_speed(&mut self, speed: Speed) {
        self.game_speed = Speed::from_multiplier_float(speed.into_multiplier_float());
        self.core.set_speed(speed.into_multiplier_float());
        self.with_recorder(|r| r.set_speed(speed));
    }

    /// Mark the start of the replay, returning the timestamp.
    /// 
    /// Set the timer offset to the given offset.
    pub fn mark_start(&mut self, timer_offset: TimestampMillis) -> Option<(UnsignedInteger, TimestampMillis)> {
        if self.replay_file_recorder.is_none() {
            return None
        }

        self.with_recorder(|r| r.mark_start(timer_offset));
        Some((self.total_frames, self.total_milliseconds))
    }

    /// Mark the end of the replay, returning the timestamp.
    pub fn mark_end(&mut self) -> Option<(UnsignedInteger, TimestampMillis)> {
        if self.replay_file_recorder.is_none() {
            return None
        }

        self.with_recorder(|r| r.mark_end());
        Some((self.total_frames, self.total_milliseconds))
    }

    fn handle_replay(&mut self) {
        if self.replay_stalled {
            return
        }

        if self.mid_frame {
            return
        }

        let Some(mut player) = self.replay_player.take() else {
            return
        };

        loop {
            match player.next_packet() {
                Ok(None) => {
                    self.replay_stalled = true;
                    break;
                },
                Ok(Some(n)) => {
                    match n {
                        Packet::NoOp => {}
                        Packet::NextFrame { timestamp_delta } => {
                            self.total_milliseconds = self.total_milliseconds.0.wrapping_add(timestamp_delta.0).into();
                            break;
                        }
                        Packet::WriteMemory { address, data } => {
                            self.core.write_ram(*address as u32, data.as_slice()).expect("failed to write RAM (and this was not handled)");
                        }
                        Packet::ChangeInput { data } => {
                            self.core.set_input_encoded(data.as_slice());
                        }
                        Packet::ChangeSpeed { speed } => {
                            self.replay_playback_speed = *speed;
                            self.match_replay_playback_speed();
                        }
                        Packet::ResetConsole => {
                            self.core.hard_reset();
                        }
                        Packet::LoadSaveState { state } => {
                            let _ = self.core.load_save_state(state.as_slice());
                        },
                        Packet::Bookmark { .. } => {}
                        Packet::Keyframe { state, .. } => {
                            if self.auto_resync_keyframes_in_replays {
                                let _ = self.core.load_save_state(state.as_slice());
                            }
                        }
                        Packet::DeltaKeyframe { .. } => {},
                        Packet::CompressedBlob { .. } => unreachable!("compressed blob"),
                        Packet::IncrementCounter { name, delta } => {
                            self.change_replay_counter_map(&name, *delta);
                        }
                    }
                }
                Err(_) => {
                    self.replay_stalled = true;
                    break
                }
            }
        }

        self.replay_player = Some(player);
    }

    fn before_run(&mut self) {
        self.handle_replay();
        self.update_input();
        self.flush_writes();
    }

    fn after_run(&mut self, time: &RunTime) {
        self.do_frame_timekeeping(&time);
        self.push_keyframe_if_needed();
    }

    fn flush_writes(&mut self) {
        if self.replay_player.is_some() {
            return
        }

        if self.mid_frame {
            return
        }

        let mut writes = core::mem::take(&mut self.writes);

        for write in writes.drain(..) {
            let _ = self.core.write_ram(write.address, write.data.as_slice());
            self.with_recorder(|recorder| recorder.write_memory(write.address as UnsignedInteger, write.data));
        }

        // reuse the allocation
        self.writes = writes;
    }

    /// Enqueue an input for the next frame.
    pub fn enqueue_input(&mut self, input: Input) {
        self.next_input = Some(input);
    }

    /// Do a hard reset.
    pub fn hard_reset(&mut self) {
        if self.replay_player.is_some() {
            return;
        }
        self.finish_current_frame();
        self.core.hard_reset();
        self.with_recorder(|r| r.reset_console());
    }

    /// Set the current rapid fire input.
    pub fn set_rapid_fire_input(&mut self, input: Option<SuperShuckieRapidFire>) {
        let Some(mut input) = input else {
            self.rapid_fire_input = None;
            return
        };

        input.total_frames = input.hold_length.get().saturating_add(input.interval.get());

        if let Some(old_input) = self.rapid_fire_input.take() && input.hold_length == old_input.hold_length && input.interval == old_input.interval {
            // copy over the duty cycle
            input.current_frame = old_input.current_frame;
        }
        else {
            // reset the duty cycle so that the button is activated on the very next frame
            if self.mid_frame {
                input.current_frame = input.total_frames - 1;
            }
            else {
                input.current_frame = 0;
            }
        }

        self.rapid_fire_input = Some(input);
    }

    /// Create a save state.
    pub fn create_save_state(&self) -> Vec<u8> {
        self.core.create_save_state()
    }

    /// Get the SRAM.
    pub fn save_sram(&self) -> Vec<u8> {
        self.core.save_sram()
    }

    /// Load a save state.
    pub fn load_save_state(&mut self, state: &[u8]) {
        if self.replay_player.is_some() {
            return
        }

        self.mid_frame = false;
        let _ = self.core.load_save_state(state);

        if self.replay_file_recorder.is_some() {
            self.with_recorder(|r| r.load_save_state(state.into()));
        }
        else {
            self.mid_frame = true;
            self.finish_current_frame();
            let _ = self.core.load_save_state(state);
        }
    }

    /// Set the current toggled input.
    ///
    /// Any activated buttons will be "stuck".
    pub fn set_toggled_input(&mut self, input: Option<Input>) {
        self.toggled_input = input;
    }

    /// Modify a counter, adding `delta`.
    pub fn change_replay_counter(&mut self, name: String, delta: SignedInteger) {
        if self.replay_file_recorder.is_none() {
            return
        }
        self.change_replay_counter_map(&name, delta);
        self.with_recorder(|r| r.change_counter(name, delta))
    }

    fn change_replay_counter_map(&mut self, name: &String, delta: SignedInteger) {
        let counters = self.replay_counters
            .as_mut()
            .expect("replay_counters is None even though we're recording a replay");
        if let Some(v) = counters.get_mut(name) {
            *v = v.wrapping_add(delta)
        }
        else {
            counters.insert(name.to_owned(), delta);
        }
    }

    /// Start recording a replay.
    pub fn start_recording_replay<
        FS: ReplayFileSink + Send + Sync + 'static,
        TS: ReplayFileSink + Send + Sync + 'static
    >(&mut self, partial_replay_record_metadata: PartialReplayRecordMetadata<FS, TS>) -> Result<(), ReplayFileWriteError> {
        self.stop_recording_replay();
        self.detach_replay_player();

        let console_type = self.core.replay_console_type().expect("NO CONSOLE_TYPE WHEN STARTING REPLAY OH NO");
        let rom_checksum = self.core.rom_checksum().to_owned();
        let bios_checksum = self.core.bios_checksum().to_owned();
        let emulator_core_name = self.core.core_name().to_owned();
        let initial_input = self.current_input;
        let initial_speed = self.game_speed;

        self.finish_current_frame();

        let initial_state = ByteVec::Heap(self.core.create_save_state());
        let mut initial_input_data = Vec::new();
        self.core.encode_input(initial_input, &mut initial_input_data);
        self.core.set_input_encoded(&initial_input_data);
        self.restart_timer();

        let recorder = NonBlockingReplayFileRecorder::new(ReplayFileRecorder::new_with_metadata(
            ReplayFileMetadata {
                console_type,
                rom_name: partial_replay_record_metadata.rom_name,
                rom_filename: partial_replay_record_metadata.rom_filename,
                rom_checksum,
                bios_checksum,
                emulator_core_name,
                patch_format: ReplayPatchFormat::Unpatched,
                patch_target_checksum: ReplayHeaderBlake3Hash::default(),
                crop_start: None,
                crop_end: None,
                timer_offset: None
            },

            ByteVec::new(),
            partial_replay_record_metadata.settings,
            self.total_milliseconds,

            ByteVec::Heap(initial_input_data),
            initial_speed,
            initial_state,
            partial_replay_record_metadata.final_file,
            partial_replay_record_metadata.temp_file
        )?);

        self.frames_per_keyframe = partial_replay_record_metadata.frames_per_keyframe.get();
        self.replay_file_recorder = Some(Box::new(recorder));
        self.replay_counters = Some(BTreeMap::new());

        Ok(())
    }

    /// Get number of milliseconds
    ///
    /// This will reset to 0 whenever a replay is started.
    pub fn get_recording_milliseconds(&self) -> TimestampMillis {
        self.total_milliseconds
    }

    /// Stop recording the current replay.
    ///
    /// Returns None if no replay was being recorded. Otherwise, returns Some(true) if successfully closed, or Some(false) if not.
    pub fn stop_recording_replay(&mut self) -> Option<bool> {
        if let Some(mut old_recorder) = self.replay_file_recorder.take() {
            self.replay_counters = None;
            return if !old_recorder.is_closed() {
                Some(old_recorder.close().is_ok())
            }
            else {
                Some(true)
            }
        }

        None
    }

    /// Forcibly stop recording the current replay.
    pub fn force_stop_recording_replay(&mut self) {
        self.replay_file_recorder = None;
    }

    fn with_recorder<F: FnOnce(&mut dyn ReplayFileRecorderFns) -> Result<(), ReplayFileWriteError>>(&mut self, what: F) {
        if let Some(n) = self.replay_file_recorder.as_mut() {
            let _ = what(Box::as_mut(n));
        }
    }

    fn update_input(&mut self) {
        if self.replay_player.is_some() {
            return
        }

        if self.mid_frame {
            return
        }

        if let Some(pending_input) = self.next_input.take() {
            self.base_input = pending_input;
        };

        let mut new_input = self.base_input;
        if let Some(rapid_fire_input) = self.rapid_fire_input && rapid_fire_input.current_frame < rapid_fire_input.hold_length.get() {
            new_input |= rapid_fire_input.input;
        }

        if let Some(toggled_input) = self.toggled_input {
            new_input |= toggled_input
        }

        self.current_input = new_input;
        self.input_scratch_buffer.clear();

        self.core.encode_input(self.current_input, &mut self.input_scratch_buffer);
        self.core.set_input_encoded(self.input_scratch_buffer.as_slice());

        if self.replay_file_recorder.is_some() {
            let mut data = ByteVec::with_capacity(self.input_scratch_buffer.len());
            data.extend_from_slice(self.input_scratch_buffer.as_slice());
            self.with_recorder(|f| f.set_input(data));
        }
    }

    fn do_frame_timekeeping(&mut self, time: &RunTime) {
        self.frames_since_last_keyframe += time.frames;
        self.total_frames = self.total_frames.wrapping_add(time.frames);
        self.mid_frame = time.frames == 0;

        if let Some(rapid_fire) = self.rapid_fire_input.as_mut() {
            rapid_fire.current_frame = rapid_fire.current_frame.wrapping_add(1) % rapid_fire.total_frames;
        }

        if self.replay_player.is_none() && !self.mid_frame {
            let ms = self.timestamp_provider.get_timestamp_milliseconds() - self.starting_milliseconds.0;
            self.total_milliseconds = ms.into();

            self.with_recorder(|f| f.next_frame(ms.into()));
        }

    }

    fn push_keyframe_if_needed(&mut self) {
        if self.mid_frame || self.replay_file_recorder.is_none() || self.frames_since_last_keyframe < self.frames_per_keyframe {
            return
        }

        self.frames_since_last_keyframe = 0;
        let ms = self.total_milliseconds;
        let save_state = ByteVec::Heap(self.core.create_save_state());
        self.with_recorder(|f| f.insert_keyframe(save_state, ms));
    }

    /// Attach a replay file player to the core.
    pub fn attach_replay_player(&mut self, mut player: ReplayFilePlayer, allow_mismatched: bool) -> Result<(), ReplayPlayerAttachError> {
        self.stop_recording_replay();
        self.detach_replay_player();

        let metadata = player.get_replay_metadata();
        let core_console_type = self.core.replay_console_type();

        if Some(metadata.console_type) != core_console_type {
            return Err(ReplayPlayerAttachError::Incompatible {
                description: format!("Console types don't match! (replay: {:?}, rom: {core_console_type:?})", metadata.console_type)
            })
        }

        if !allow_mismatched {
            let mut mismatched_list = Vec::new();

            let rom_checksum = *self.core.rom_checksum();
            let bios_checksum = *self.core.bios_checksum();
            let core_name = self.core.core_name();

            if metadata.rom_checksum != rom_checksum {
                mismatched_list.push(ReplayPlayerMetadataMismatchKind::ROMChecksumMismatch { replay: metadata.rom_checksum, loaded: bios_checksum })
            }

            if metadata.bios_checksum != bios_checksum {
                mismatched_list.push(ReplayPlayerMetadataMismatchKind::BIOSChecksumMismatch { replay: metadata.bios_checksum, loaded: bios_checksum })
            }

            if metadata.emulator_core_name != core_name {
                mismatched_list.push(ReplayPlayerMetadataMismatchKind::CoreMismatch { replay: metadata.emulator_core_name.clone(), loaded: core_name.to_owned() })
            }

            if !mismatched_list.is_empty() {
                return Err(ReplayPlayerAttachError::MismatchedMetadata { issues: mismatched_list })
            }
        }

        if let Err(e) = player.go_to_keyframe(0) {
            todo!("can't go to 0th keyframe (and can't handle this error TODO): {e:?}")
        }

        self.current_input = Input::new();
        self.next_input = None;
        self.replay_player = Some(player);
        self.replay_counters = Some(BTreeMap::new());
        self.replay_stalled = false;
        self.restart_timer();

        self.go_to_replay_frame_inner(0, 0);

        Ok(())
    }

    /// Detach the current replay player.
    pub fn detach_replay_player(&mut self) {
        if self.replay_player.is_none() {
            return;
        }

        self.replay_stalled = false;
        self.replay_player = None;
        self.replay_counters = None;
        self.reset_input();
    }

    /// Reset the current input.
    pub fn reset_input(&mut self) {
        self.enqueue_input(Input::new());
    }

    /// Seek to the given frame (if playing back).
    pub fn go_to_replay_frame(&mut self, frame: UnsignedInteger) {
        // go one frame before so that we play the actually desired frame (so it is rendered)
        let before_frame = frame.saturating_sub(1);
        self.go_to_replay_frame_inner(before_frame, before_frame);
    }

    fn go_to_replay_frame_inner(&mut self, frame: UnsignedInteger, desired: UnsignedInteger) {
        let Some(p) = self.replay_player.as_mut() else {
            return
        };

        let desired = desired.min(p.get_total_frames().saturating_sub(1));
        if desired >= p.get_total_frames() {
            return
        }

        if let Err(e) = p.go_to_keyframe(frame) {
            match e {
                ReplaySeekError::ReadError { error } => todo!("can't go to {frame}: {error:?} (can't handle this error TODO)"),
                ReplaySeekError::NoSuchKeyframe { best, .. } => {
                    return self.go_to_replay_frame_inner(best, desired);
                }
            }
        }

        let Ok(Some(Packet::Keyframe { metadata, state })) = p.next_packet() else {
            todo!("replay file is broken (no keyframe found at frame {frame}!! and error handling not yet implemented)")
        };

        let speed = metadata.speed;

        self.core.load_save_state(state.as_slice()).expect("replay file is broken (can't load save state) and error handling not yet implemented!");

        self.mid_frame = false;
        self.total_frames = metadata.elapsed_frames;
        self.total_milliseconds = metadata.elapsed_millis;
        self.replay_stalled = false;
        self.frames_since_last_keyframe = 0;
        self.replay_counters = Some(metadata.counters.iter().map(|c| (c.name.clone(), c.value)).collect());
        self.replay_playback_speed = speed;

        self.match_replay_playback_speed();

        while self.total_frames <= desired && !self.replay_stalled {
            self.run_unlocked();
        }
    }

    /// Get any errors for the replay writes.
    ///
    /// This should be called to ensure that it is still recording a replay.
    pub fn poll_replay_recording_errors(&mut self) -> Vec<ReplayFileWriteError> {
        self.replay_file_recorder
            .as_mut()
            .map(|r| r.get_errors())
            .unwrap_or(Vec::new())
    }

    /// Set whether or not to ignore speed changes in replays
    pub fn set_ignore_speed_changes_in_replays(&mut self, ignored: bool) {
        self.ignore_speed_changes_in_replays = ignored;
        if self.replay_player.is_some() {
            self.match_replay_playback_speed();
        }
    }

    /// Set whether or not to automatically resync keyframes on playback
    pub fn set_auto_resync_keyframes_in_replays(&mut self, resync: bool) {
        self.auto_resync_keyframes_in_replays = resync;
    }

    fn match_replay_playback_speed(&mut self) {
        if !self.ignore_speed_changes_in_replays {
            self.set_speed(self.replay_playback_speed);
        }
    }
}

/// Returns when an error occurs.
#[derive(Clone, Debug)]
pub enum ReplayPlayerAttachError {
    /// Metadata is mismatched. It may desync.
    #[allow(missing_docs)]
    MismatchedMetadata {
        issues: Vec<ReplayPlayerMetadataMismatchKind>
    },

    /// Metadata is mismatched.
    #[allow(missing_docs)]
    Incompatible {
        description: String
    }
}

/// Describes a metadata mismatch.
#[derive(Clone, Debug)]
#[allow(missing_docs)]
pub enum ReplayPlayerMetadataMismatchKind {
    ROMChecksumMismatch {
        replay: ReplayHeaderBlake3Hash,
        loaded: ReplayHeaderBlake3Hash
    },

    BIOSChecksumMismatch {
        replay: ReplayHeaderBlake3Hash,
        loaded: ReplayHeaderBlake3Hash
    },

    CoreMismatch {
        replay: String,
        loaded: String
    }
}

impl Display for ReplayPlayerMetadataMismatchKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            ReplayPlayerMetadataMismatchKind::ROMChecksumMismatch { replay, loaded } => {
                f.write_fmt(format_args!(
                    "ROM checksum mismatch! Either the wrong ROM is loaded, or it was modified.\n\n  Replay: {}\n  Loaded: {}\n\nThis can cause potential desyncs.",
                    blake3_hash_to_ascii(*replay), blake3_hash_to_ascii(*loaded)
                ))
            }
            ReplayPlayerMetadataMismatchKind::BIOSChecksumMismatch { replay, loaded } => {
                f.write_fmt(format_args!(
                    "BIOS checksum mismatch! Either the wrong BIOS is loaded, or it was modified.\n\n  Replay: {}\n  Loaded: {}\n\nThis can cause potential desyncs.",
                    blake3_hash_to_ascii(*replay), blake3_hash_to_ascii(*loaded)
                ))
            }
            ReplayPlayerMetadataMismatchKind::CoreMismatch { replay, loaded } => {
                f.write_fmt(format_args!(
                    "ROM core mismatch! Different cores or different versions of cores were used.\n\n  Replay: {}\n  Loaded: {}\n\nThis can cause potential desyncs UNLESS both cores have equal accuracy.",
                    replay, loaded
                ))
            }
        }
    }
}

#[allow(missing_docs)]
pub type TimestampMicros = u64;

/// Function that monotonically produces a timestamp.
///
/// The timestamp must never go backwards, although it does not necessarily always have to go
/// forwards, either.
pub trait MonotonicTimestampProvider: Send {
    /// Get the timestamp in milliseconds.
    fn get_timestamp_microseconds(&mut self) -> TimestampMicros;

    /// Get the timestamp in microseconds.
    ///
    /// This does not need to be implemented.
    fn get_timestamp_milliseconds(&mut self) -> u64 {
        self.get_timestamp_microseconds() / 1000
    }
}

#[cfg(feature = "std")]
/// Generate a timestamp provider backed by [`std::time::Instant`]
pub fn std_timestamp_provider() -> Box<dyn MonotonicTimestampProvider> {
    Box::new(std_timestamp_provider::StdTimestampProvider::new())
}

#[cfg(feature = "std")]
mod std_timestamp_provider {
    use std::time::Instant;
    use supershuckie_replay_recorder::UnsignedInteger;
    use crate::MonotonicTimestampProvider;

    pub struct StdTimestampProvider {
        reference_time: Instant
    }

    impl StdTimestampProvider {
        pub fn new() -> Self {
            Self { reference_time: Instant::now() }
        }
    }

    impl MonotonicTimestampProvider for StdTimestampProvider {
        fn get_timestamp_microseconds(&mut self) -> u64 {
            (Instant::now() - self.reference_time).as_micros() as UnsignedInteger
        }
    }
}

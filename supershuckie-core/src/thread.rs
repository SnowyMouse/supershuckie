use crate::emulator::{EmulatorCore, Input, PartialReplayRecordMetadata, ScreenData};
use crate::{std_timestamp_provider, ReplayPlayerAttachError, Speed};
use crate::{SuperShuckieCore, SuperShuckieRapidFire};
use std::borrow::ToOwned;
use std::boxed::Box;
use std::collections::BTreeMap;
use std::fs::File;
use std::string::String;
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, TryLockError, Weak};
use std::time::Duration;
use std::vec::Vec;
use std::format;

#[cfg(feature = "pokeabyte")]
use supershuckie_pokeabyte_integration::PokeAByteIntegrationServer;
use supershuckie_pokeabyte_integration::PokeAByteWriteCommand;
use supershuckie_replay_recorder::replay_file::playback::ReplayFilePlayer;
use supershuckie_replay_recorder::replay_file::record::ReplayFileWriteError;
use supershuckie_replay_recorder::{ByteVec, UnsignedInteger};

/// A (mostly) non-blocking, threaded wrapper for [`SuperShuckieCore`].
pub struct ThreadedSuperShuckieCore {
    screens: Arc<Mutex<Vec<ScreenData>>>,
    sender: Sender<ThreadCommand>,
    receiver_close: Receiver<()>,

    frame_count: Arc<AtomicU32>,
    elapsed_milliseconds: Arc<AtomicU32>,
    desired_replay_frame: Arc<AtomicU32>,
    delta_replay_frames: Arc<AtomicI32>,

    playback: bool,
    playback_total_frames: UnsignedInteger,
    playback_total_milliseconds: UnsignedInteger,
    replay_errors: Arc<Mutex<Vec<ReplayFileWriteError>>>
}

impl ThreadedSuperShuckieCore {
    /// Wrap the given `core`.
    pub fn new(emulator_core: Box<dyn EmulatorCore>) -> Self {
        let frame_count = Arc::new(AtomicU32::new(0));
        let screens = Arc::new(Mutex::new(emulator_core.get_screens().to_vec()));
        let (sender, receiver) = channel();
        let (sender_close, receiver_close) = channel();

        let replay_milliseconds = Arc::new(AtomicU32::new(0));
        let playback_total_frames = 0;
        let playback_total_milliseconds = 0;
        let desired_replay_frame = Arc::new(AtomicU32::new(u32::MAX));
        let delta_replay_frames = Arc::new(AtomicI32::new(0));
        let replay_errors = Arc::new(Mutex::new(Vec::new()));

        {
            let frame_count = frame_count.clone();
            let screens = Arc::downgrade(&screens);
            let replay_milliseconds = replay_milliseconds.clone();
            let desired_replay_frame = desired_replay_frame.clone();
            let delta_replay_frames = delta_replay_frames.clone();
            let replay_errors = replay_errors.clone();
            let _ = std::thread::Builder::new().name("ThreadedSuperShuckieCore".to_owned()).spawn(move || {
                ThreadedSuperShuckieCoreThread {
                    screens,
                    screens_queued: emulator_core.get_screens().to_vec(),
                    screen_ready_for_copy: false,
                    is_running: false,
                    core: SuperShuckieCore::new(emulator_core, std_timestamp_provider()),
                    pokeabyte_integration: None,
                    receiver,
                    sender_close,
                    desired_replay_frame,
                    frame_count,
                    replay_milliseconds,
                    delta_replay_frames,
                    replay_errors,
                    playback_frozen: false,
                    freezes: BTreeMap::new()
                }.run_thread();
            });
        }

        Self {
            sender,
            screens,
            receiver_close,
            frame_count,
            elapsed_milliseconds: replay_milliseconds,
            playback_total_frames,
            playback_total_milliseconds,
            replay_errors,
            playback: false,
            desired_replay_frame,
            delta_replay_frames
        }
    }

    /// Get the elapsed frame count.
    ///
    /// This can be called to ensure that a unique frame is ready to be read. Note, however, that
    /// this number may be slightly outdated.
    pub fn get_elapsed_frames(&self) -> u32 {
        self.frame_count.load(Ordering::Relaxed)
    }

    /// Read the screens.
    ///
    /// Note that while this function is running, the screen buffer will be blocked from being
    /// updated and may not be immediately updated until later.
    pub fn read_screens<T, F: FnOnce(&[ScreenData]) -> T>(&self, reader: F) -> T {
        let lock = self.screens.lock().expect("screen mutex is poisoned");
        reader(lock.as_slice())
    }

    /// Start running continuously.
    pub fn start(&self) {
        self.sender.send(ThreadCommand::Start)
            .expect("Start - the core thread has crashed");
    }

    /// Pause running.
    pub fn pause(&self) {
        self.sender.send(ThreadCommand::Pause)
            .expect("Pause - the core thread has crashed");
    }

    /// Pause running temporarily.
    pub fn set_playback_frozen(&self, paused: bool) {
        self.sender.send(ThreadCommand::SetPlaybackFrozen(paused))
            .expect("SetPlaybackFrozen - the core thread has crashed");
    }

    /// Attach/detach a Poke-A-Byte integration server.
    pub fn set_pokeabyte_enabled(&self, enabled: bool) -> Result<(), String> {
        let (sender, receiver) = channel();

        self.sender.send(ThreadCommand::SetPokeAByteEnabled(enabled, sender))
            .expect("SetPokeAByteEnabled - the core thread has crashed");

        receiver.recv().ok().unwrap_or(Ok(()))
    }

    /// Stop recording replay.
    pub fn start_recording_replay(&self, metadata: PartialReplayRecordMetadata<std::io::BufWriter<File>, std::io::BufWriter<File>>) {
        self.sender.send(ThreadCommand::StartRecordingReplay(metadata))
            .expect("StopRecordingReplay - the core thread has crashed");
    }

    /// Stop recording replay.
    pub fn stop_recording_replay(&self) -> bool {
        let (sender, receiver) = channel();

        self.sender.send(ThreadCommand::StopRecordingReplay(sender))
            .expect("StopRecordingReplay - the core thread has crashed");

        receiver.recv().ok().unwrap_or(false)
    }

    /// Enqueue an input.
    pub fn enqueue_input(&self, input: Input) {
        self.sender.send(ThreadCommand::EnqueueInput(input))
            .expect("EnqueueInput - the core thread has crashed");
    }

    /// Set the speed.
    pub fn set_speed(&self, speed: Speed) {
        self.sender.send(ThreadCommand::SetSpeed(speed))
            .expect("SetSpeed - the core thread has crashed");
    }

    /// Set the speed.
    pub fn hard_reset(&self) {
        self.sender.send(ThreadCommand::HardReset)
            .expect("HardReset - the core thread has crashed");
    }

    /// Set the rapid fire input.
    pub fn set_rapid_fire_input(&self, input: Option<SuperShuckieRapidFire>) {
        self.sender.send(ThreadCommand::SetRapidFireInput(input))
            .expect("SetRapidFireInput - the core thread has crashed");
    }

    /// Set the toggle input.
    pub fn set_toggled_input(&self, input: Option<Input>) {
        self.sender.send(ThreadCommand::SetToggledInput(input))
            .expect("SetToggledInput - the core thread has crashed");
    }

    /// Create a save state.
    ///
    /// Returns `None` if no save state could be created for some unknown reason.
    ///
    /// NOTE: This is blocking.
    pub fn create_save_state(&self) -> Option<Vec<u8>> {
        let (sender, receiver) = channel();
        self.sender.send(ThreadCommand::CreateSaveState(sender))
            .expect("CreateSaveState - the core thread has crashed");
        receiver.recv().ok()
    }

    /// Load a save state.
    pub fn load_save_state(&self, state: Vec<u8>) {
        self.sender.send(ThreadCommand::LoadSaveState(state))
            .expect("LoadSaveState - the core thread has crashed");
    }

    /// Get SRAM.
    ///
    /// Returns `None` if SRAM could not be read for some unknown reason.
    ///
    /// NOTE: This is blocking.
    pub fn get_sram(&self) -> Option<Vec<u8>> {
        let (sender, receiver) = channel();
        self.sender.send(ThreadCommand::SaveSRAM(sender))
            .expect("SaveSRAM - the core thread has crashed");
        receiver.recv().ok()
    }

    /// Get the number of milliseconds a replay has been recorded.
    #[inline]
    pub fn get_elapsed_milliseconds(&self) -> u32 {
        self.elapsed_milliseconds.load(Ordering::Relaxed)
    }

    /// Get whether or not a replay is being played back.
    #[inline]
    pub fn is_playing_back(&self) -> bool {
        self.playback
    }

    /// Get the total number of frames in the current playback.
    #[inline]
    pub fn get_playback_total_frames(&self) -> u32 {
        self.playback_total_frames as u32
    }

    /// Get the total number of frames in the current playback.
    #[inline]
    pub fn get_playback_total_milliseconds(&self) -> u32 {
        self.playback_total_milliseconds as u32
    }

    /// Load the replay.
    pub fn attach_replay_player(&mut self, mut player: ReplayFilePlayer, allow_mismatch: bool) -> Result<(), ReplayPlayerAttachError> {
        player.enable_threading();

        let total_ticks = player.get_total_milliseconds();
        let total_frames = player.get_total_frames();

        let (sender, receiver) = channel();

        self.sender.send(ThreadCommand::AttachReplayPlayer {
            player,
            allow_mismatched: allow_mismatch,
            errors: sender
        }).expect("AttachReplayPlayer - the core thread has crashed");

        match receiver.recv() {
            Err(_) => {
                self.playback_total_frames = total_frames;
                self.playback_total_milliseconds = total_ticks;
                self.playback = true;
                Ok(())
            },
            Ok(n) => Err(n)
        }
    }

    /// Detach a replay
    pub fn detach_replay_player(&mut self) {
        self.playback_total_frames = 0;
        self.playback_total_milliseconds = 0;
        self.playback = false;
        self.sender.send(ThreadCommand::DetachReplayPlayer)
            .expect("DetachReplayPlayer - the core thread has crashed")
    }

    /// Go to the desired frame.
    pub fn go_to_replay_frame(&self, frame: u32) {
        // we use an AtomicU32 instead of just directly going to a frame
        // because we do not want to clog the queue with goto requests
        self.desired_replay_frame.store(frame, Ordering::Relaxed);
    }

    /// Advance or go back some frames.
    pub fn advance_playback_frames(&self, amount: i32) {
        // similarly use AtomicI32 to avoid clogging the queue
        self.delta_replay_frames.store(amount, Ordering::Relaxed);
    }

    /// Get any replay recording errors.
    pub fn get_replay_recording_errors(&mut self) -> Vec<ReplayFileWriteError> {
        self.replay_errors.clear_poison();
        core::mem::take(&mut *self.replay_errors.lock().expect("get_replay_recording_errors fainted due to poison"))
    }
}

impl Drop for ThreadedSuperShuckieCore {
    fn drop(&mut self) {
        // we couldn't really care less if these succeed or fail; we just want to ensure that
        // the replay file is closed, and it should be (if it didn't error)
        let _ = self.sender.send(ThreadCommand::Close);
        let _ = self.receiver_close.recv();
    }
}

// TODO: Option to run just a single frame? Maybe also skip around a replay file to a given
//       keyframe...
enum ThreadCommand {
    Start,
    Pause,
    SetPlaybackFrozen(bool),
    SetPokeAByteEnabled(bool, Sender<Result<(), String>>),
    StartRecordingReplay(PartialReplayRecordMetadata<std::io::BufWriter<File>, std::io::BufWriter<File>>),
    StopRecordingReplay(Sender<bool>),
    AttachReplayPlayer {
        player: ReplayFilePlayer,
        allow_mismatched: bool,
        errors: Sender<ReplayPlayerAttachError>
    },
    DetachReplayPlayer,
    EnqueueInput(Input),
    SetRapidFireInput(Option<SuperShuckieRapidFire>),
    SetToggledInput(Option<Input>),
    SetSpeed(Speed),
    HardReset,
    CreateSaveState(Sender<Vec<u8>>),
    LoadSaveState(Vec<u8>),
    SaveSRAM(Sender<Vec<u8>>),
    Close
}

struct ThreadedSuperShuckieCoreThread {
    screens: Weak<Mutex<Vec<ScreenData>>>,

    screens_queued: Vec<ScreenData>,
    screen_ready_for_copy: bool,
    frame_count: Arc<AtomicU32>,
    replay_milliseconds: Arc<AtomicU32>,
    desired_replay_frame: Arc<AtomicU32>,
    delta_replay_frames: Arc<AtomicI32>,
    replay_errors: Arc<Mutex<Vec<ReplayFileWriteError>>>,
    playback_frozen: bool,

    core: SuperShuckieCore,
    receiver: Receiver<ThreadCommand>,
    is_running: bool,
    pokeabyte_integration: Option<PokeAByteIntegrationServer>,
    sender_close: Sender<()>,

    freezes: BTreeMap<u64, ByteVec>
}

impl ThreadedSuperShuckieCoreThread {
    fn run_thread(mut self) {
        loop {
            if let Ok(cmd) = self.receiver.try_recv() {
                if matches!(cmd, ThreadCommand::Close) {
                    break
                }

                self.handle_command(cmd);
                continue
            }

            self.handle_replay_recording_errors();
            self.go_to_desired_frame();
            self.refresh_screen_data();
            self.update_queued_screens();
            self.handle_pokeabyte_integration();
            self.replay_milliseconds.store(self.core.get_recording_milliseconds() as u32, Ordering::Relaxed);

            if self.is_running {
                if !self.playback_frozen {
                    self.core.run();
                }
            }
            else if self.core.replay_player.is_none() {
                // unfortunately we can't just block until we're running again because we still need
                // to handle pokeabyte writes
                std::thread::sleep(Duration::from_millis(100));
            }
            else {
                // sleep for a reduced time so seeking can still be responsive
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        self.core.stop_recording_replay();
        self.pokeabyte_integration = None;

        let _ = self.sender_close.send(());
    }

    fn go_to_desired_frame(&mut self) {
        let delta = self.delta_replay_frames.swap(0, Ordering::Relaxed);
        let frame = self.desired_replay_frame.swap(u32::MAX, Ordering::Relaxed);
        if frame != u32::MAX {
            self.core.go_to_replay_frame(frame as UnsignedInteger);
        }
        else if delta != 0 {
            self.core.go_to_replay_frame(self.core.total_frames.saturating_add_signed(delta as i64));
        }
        else {
            return
        }

        // We aren't really too focused on smooth playback as opposed to updating the buffer now!
        self.force_refresh_screen_data();
    }

    /// If the mutex was blocked, we can copy it in when it's no longer blocked.
    fn update_queued_screens(&mut self) {
        if !self.screen_ready_for_copy {
            return
        }

        let Some(screen_data) = self.screens.upgrade() else {
            panic!("update_queued_screens Can't get screen_data: owning thread must have crashed");
        };

        let mut out_screens = match screen_data.try_lock() {
            Ok(n) => n,
            Err(TryLockError::WouldBlock) => return,
            Err(e) => panic!("update_queued_screens Can't get screens mutex: {e}")
        };

        self.screen_ready_for_copy = false;

        let in_screens = &mut self.screens_queued;
        core::mem::swap(in_screens, &mut *out_screens);

        self.frame_count.store(self.core.total_frames as u32, Ordering::Relaxed);
    }

    /// Attempt to copy the screen data, or store it for later.
    fn refresh_screen_data(&mut self) {
        if self.is_running && self.core.mid_frame {
            return
        }

        let Some(screen_data) = self.screens.upgrade() else {
            panic!("refresh_screen_data Can't get screen_data: owning thread must have crashed");
        };

        let mut out_screens_maybe = screen_data.try_lock();

        let out_screens_result = match out_screens_maybe.as_mut() {
            Ok(n) => {
                self.screen_ready_for_copy = false;

                // this is safe to update early since we have the mutex locked
                self.frame_count.store(self.core.total_frames as u32, Ordering::Relaxed);
                &mut *n
            },
            Err(TryLockError::WouldBlock) => {
                self.screen_ready_for_copy = true;
                &mut self.screens_queued
            },
            Err(e) => panic!("refresh_screen_data Can't get screens mutex: {e}")
        };

        self.core.core.swap_screen_data(out_screens_result.as_mut_slice());
    }

    fn handle_replay_recording_errors(&mut self) {
        let errors = self.core.poll_replay_recording_errors();
        if errors.is_empty() {
            return;
        }

        self.core.force_stop_recording_replay();
        self.replay_errors.lock().expect("could not get replay errors mutex").extend(errors.into_iter());
    }

    fn force_refresh_screen_data(&mut self) {
        let Some(screen_data) = self.screens.upgrade() else {
            panic!("force_refresh_screen_data Can't get screen_data: owning thread must have crashed");
        };

        let mut out_screens = screen_data
            .lock()
            .expect("can't get screens mutex force_get_screen_data");

        self.frame_count.store(self.core.total_frames as u32, Ordering::Relaxed);
        self.screen_ready_for_copy = false;

        for (screen_from, screen_to) in self.core.core.get_screens().iter().zip(out_screens.iter_mut()) {
            screen_to.pixels.copy_from_slice(screen_from.pixels.as_slice());
        }
    }

    /// Update RAM read/writes
    fn handle_pokeabyte_integration(&mut self) {
        let Some(integration) = self.pokeabyte_integration.as_ref() else {
            return
        };

        let mut session_lock = integration.get_session();
        let Some(session) = session_lock.as_mut() else {
            return;
        };

        for write in &mut session.writes {
            match write {
                PokeAByteWriteCommand::Write { address, data } => {
                    self.core.enqueue_write(address as u32, data);
                },
                PokeAByteWriteCommand::Freeze { address, data } => {
                    self.freezes.insert(address, data);
                },
                PokeAByteWriteCommand::Unfreeze { address } => {
                    self.freezes.remove(&address);
                }
            }
        }

        for (address, data) in &self.freezes {
            self.core.enqueue_write(*address as u32, data.clone());
        }

        // don't update reads mid-frame; it's too slow
        if self.core.mid_frame && self.is_running {
            return;
        }

        // handle frame skipping unless we're paused
        if self.is_running && let Some(skipping) = session.config.frame_skip && self.core.total_frames % ((skipping as u64) + 1) != 0 {
            return
        }

        // SAFETY: "Only one way to find out"
        let ram = unsafe { session.shared_memory.get_memory_mut() };
        for read in &session.config.blocks {
            let into = ram.get_mut(read.range.clone()).expect("read range was wrong (this should have been checked!)");
            let _ = self.core.get_core().read_ram(read.game_address, into); // TODO: handle this?
        }
    }

    fn handle_command(&mut self, command: ThreadCommand) {
        match command {
            ThreadCommand::Start => {
                if !self.is_running {
                    self.is_running = true;
                    self.core.unpause_timer();
                }
            }
            ThreadCommand::Pause => {
                if self.is_running {
                    self.is_running = false;
                    self.core.pause_timer();
                }
            }
            ThreadCommand::SetPokeAByteEnabled(enabled, err) => {
                if !enabled && self.pokeabyte_integration.is_some() {
                    self.pokeabyte_integration = None;
                    let _ = err.send(Ok(()));
                }
                else if enabled {
                    let integration = match PokeAByteIntegrationServer::begin_listen() {
                        Ok(n) => {
                            let _ = err.send(Ok(()));
                            n
                        },
                        Err(e) => {
                            let _ = err.send(Err(format!("{e:?}")));
                            return
                        }
                    };
                    self.pokeabyte_integration = Some(integration)
                } else {
                    let _ = err.send(Ok(()));
                }
            }
            ThreadCommand::StartRecordingReplay(metadata) => {
                self.replay_errors.lock().expect("start recording replay failed to get replay errors").clear();

                // FIXME: error if this fails
                self.core.start_recording_replay(metadata).expect("FAILED TO START RECORDING REPLAY OH NO");
                if !self.is_running {
                    self.core.pause_timer();
                }
            }
            ThreadCommand::StopRecordingReplay(sender) => {
                let _ = sender.send(self.core.stop_recording_replay() == Some(true));
            }
            ThreadCommand::EnqueueInput(input) => {
                self.core.enqueue_input(input);
            }
            ThreadCommand::SetSpeed(speed) => {
                self.core.set_speed(speed);
            }
            ThreadCommand::SetRapidFireInput(input) => {
                self.core.set_rapid_fire_input(input);
            }
            ThreadCommand::SetToggledInput(input) => {
                self.core.set_toggled_input(input);
            }
            ThreadCommand::HardReset => {
                self.core.hard_reset();
            }
            ThreadCommand::CreateSaveState(sender) => {
                self.core.finish_current_frame();
                let _ = sender.send(self.core.create_save_state());
            }
            ThreadCommand::LoadSaveState(state) => {
                self.core.load_save_state(&state);
            }
            ThreadCommand::SetPlaybackFrozen(paused) => {
                self.playback_frozen = paused;
            }
            ThreadCommand::SaveSRAM(sender) => {
                let _ = sender.send(self.core.save_sram());
            }
            ThreadCommand::Close => {
                unreachable!("handle_command(ThreadCommand::Close) should not happen")
            },
            ThreadCommand::AttachReplayPlayer { player, allow_mismatched, errors } => {
                if let Err(e) = self.core.attach_replay_player(player, allow_mismatched) {
                    let _ = errors.send(e);
                }
                if !self.is_running {
                    self.core.pause_timer();
                }
            }
            ThreadCommand::DetachReplayPlayer => {
                self.core.detach_replay_player();
            }
        }
    }
}

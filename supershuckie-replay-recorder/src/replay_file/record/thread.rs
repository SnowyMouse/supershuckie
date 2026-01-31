use super::{ReplayFileWriteError, ReplayFileRecorder, ReplayFileSink, ReplayFileRecorderFns};
use crate::{ByteVec, InputBuffer, SignedInteger, Speed, TimestampMillis, UnsignedInteger};
use alloc::borrow::ToOwned;
use alloc::string::String;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Mutex;
use std::sync::{Arc, Weak};
use alloc::vec::Vec;
use std::time::Duration;

type RecorderMutex<Final, Temp> = Mutex<ReplayFileRecorder<Final, Temp>>;

/// File recorder that records in a separate thread and is non-blocking.
///
/// The `std` feature is required to use this.
pub struct NonBlockingReplayFileRecorder<Final: ReplayFileSink + Send + 'static, Temp: ReplayFileSink + Send + 'static> {
    recorder: Option<Arc<RecorderMutex<Final, Temp>>>,

    sender: Sender<ThreadedReplayFileRecorderCommand>,
    errors: Receiver<ReplayFileWriteError>,
    closed: Receiver<()>
}

impl<Final: ReplayFileSink + Send + 'static, Temp: ReplayFileSink + Send + 'static> NonBlockingReplayFileRecorder<Final, Temp> {
    /// Instantiate a non-blocking replay recorder.
    pub fn new(recorder: ReplayFileRecorder<Final, Temp>) -> NonBlockingReplayFileRecorder<Final, Temp> {
        let recorder = Arc::new(Mutex::new(recorder));

        let (sender_main, receiver_helper) = channel();
        let (sender_helper, receiver_main) = channel();
        let (closed_helper, closed_main) = channel();

        let helper = ThreadedReplayFileRecorderThread {
            recorder: Arc::downgrade(&recorder),
            error_sender: sender_helper,
            receiver: receiver_helper,
            closed: closed_helper
        };

        std::thread::Builder::new()
            .name("ThreadedReplayFileRecorderThread".to_owned())
            .spawn(move || {
                helper.run();
            })
            .expect("failed to start a thread...");

        Self {
            sender: sender_main,
            errors: receiver_main,
            recorder: Some(recorder),
            closed: closed_main
        }
    }

    /// Return `true` if the recorder was already closed.
    #[inline]
    pub fn is_closed(&self) -> bool {
        self.recorder.is_none()
    }

    /// Close the replay file recorder, blocking until closed.
    ///
    /// # Panics
    ///
    /// Panics if already closed.
    pub fn close(&mut self) -> Result<(Final, Temp), (Final, Temp, ReplayFileWriteError)> {
        // Close it
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::Close);

        // Wait for it to be fully closed (thus all writes are processed, etc.)
        let _ = self.closed.recv();

        // If the other thread is busy, we'll need to spin here until it's done.
        let mut a = self.recorder.take().expect("recorder already closed");
        let recorder = loop {
            match Arc::try_unwrap(a) {
                Ok(n) => break n,
                Err(e) => a = e
            }
            std::thread::sleep(Duration::from_millis(25));
        };

        // This should work unless the thread panicked.
        let mut recorder = recorder.into_inner().expect("failed to get the inner value");

        // Done.
        recorder.close()
    }

    /// Advance a new frame.
    pub fn next_frame(&mut self, timestamp: TimestampMillis) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::NextFrame { timestamp });
    }

    /// Add a bookmark.
    pub fn add_bookmark<S: Into<String>>(&mut self, name: S) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::AddBookmark { bookmark: name.into() });
    }

    /// Add a new keyframe.
    pub fn insert_keyframe(&mut self, state: ByteVec, timestamp: TimestampMillis) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::NewKeyframe { state, timestamp });
    }

    /// Set the current input.
    pub fn set_input(&mut self, input: InputBuffer) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::SetInput { input });
    }

    /// Hard-reset the console.
    pub fn reset_console(&mut self) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::ResetConsole);
    }

    /// Write RAM to an address.
    pub fn write_memory(&mut self, address: UnsignedInteger, data: ByteVec) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::WriteMemory { address, data });
    }

    /// Set the current speed.
    pub fn set_speed(&mut self, speed: Speed) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::SetSpeed { speed });
    }

    /// Load the keyframe at the given frame index.
    pub fn load_save_state(&mut self, state: ByteVec) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::LoadSaveState { state });
    }

    /// Check for errors, if any.
    pub fn poll_errors(&mut self) -> Vec<ReplayFileWriteError> {
        let mut errors = Vec::new();
        
        while let Ok(error) = self.errors.try_recv() {
            errors.push(error);
        }
        
        errors
    }

    /// Mark the start of the replay.
    pub fn mark_start(&mut self, timer_offset: TimestampMillis) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::MarkStart { timer_offset });
    }

    /// Mark the end of the replay.
    pub fn mark_end(&mut self) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::MarkEnd);
    }

    /// Change the counter.
    pub fn change_counter(&mut self, name: String, delta: SignedInteger) {
        let _ = self.sender.send(ThreadedReplayFileRecorderCommand::IncrementCounter { name, delta });
    }
}

struct ThreadedReplayFileRecorderThread<Final: ReplayFileSink, Temp: ReplayFileSink> {
    recorder: Weak<RecorderMutex<Final, Temp>>,

    // note: the success of sending will never be checked; we do not care because this thread will
    // eventually be closed if it fails
    error_sender: Sender<ReplayFileWriteError>,
    receiver: Receiver<ThreadedReplayFileRecorderCommand>,
    closed: Sender<()>
}

impl<Final: ReplayFileSink, Temp: ReplayFileSink> ThreadedReplayFileRecorderThread<Final, Temp> {
    fn run(mut self) {
        loop {
            // If any of these fails, abort the thread.
            let Ok(command) = self.receiver.recv() else {
                break
            };
            if matches!(command, ThreadedReplayFileRecorderCommand::Close) {
                break
            }
            let Some(recorder) = self.recorder.upgrade() else {
                break
            };
            let Ok(mut recorder) = recorder.lock() else {
                break
            };

            if let Err(e) = self.handle_command(command, &mut recorder) {
                let _ = self.error_sender.send(e);
            }
        }

        let _ = self.closed.send(());
    }

    fn handle_command(&mut self, command: ThreadedReplayFileRecorderCommand, recorder: &mut ReplayFileRecorder<Final, Temp>) -> Result<(), ReplayFileWriteError> {
        match command {
            ThreadedReplayFileRecorderCommand::Close => Ok(()),
            ThreadedReplayFileRecorderCommand::WriteMemory { address, data } => {
                recorder.write_memory(address, data)
            },
            ThreadedReplayFileRecorderCommand::NewKeyframe { timestamp, state } => {
                let _ = recorder.insert_keyframe(state, timestamp)?;
                Ok(())
            }
            ThreadedReplayFileRecorderCommand::SetInput { input } => {
                recorder.set_input(input)
            },
            ThreadedReplayFileRecorderCommand::SetSpeed { speed } => {
                recorder.set_speed(speed)
            },
            ThreadedReplayFileRecorderCommand::AddBookmark { bookmark } => {
                recorder.add_bookmark(bookmark)
            },
            ThreadedReplayFileRecorderCommand::ResetConsole => {
                recorder.reset_console()
            },
            ThreadedReplayFileRecorderCommand::NextFrame { timestamp } => {
                recorder.next_frame(timestamp)
            },
            ThreadedReplayFileRecorderCommand::LoadSaveState { state } => {
                recorder.load_save_state(state)
            }
            ThreadedReplayFileRecorderCommand::MarkStart { timer_offset } => {
                recorder.mark_start(timer_offset)
            }
            ThreadedReplayFileRecorderCommand::MarkEnd => {
                recorder.mark_end()
            }
            ThreadedReplayFileRecorderCommand::IncrementCounter { name, delta } => {
                recorder.change_counter(name, delta)
            }
        }
    }
}

enum ThreadedReplayFileRecorderCommand {
    NextFrame { timestamp: TimestampMillis },
    AddBookmark { bookmark: String },
    NewKeyframe { state: ByteVec, timestamp: TimestampMillis },
    SetInput { input: InputBuffer },
    SetSpeed { speed: Speed },
    WriteMemory { address: UnsignedInteger, data: ByteVec },
    LoadSaveState { state: ByteVec },
    IncrementCounter { name: String, delta: SignedInteger },
    MarkStart { timer_offset: TimestampMillis },
    MarkEnd,
    ResetConsole,
    Close
}

impl<Final: ReplayFileSink + Sync + Send + 'static, Temp: ReplayFileSink + Sync + Send + 'static> ReplayFileRecorderFns for NonBlockingReplayFileRecorder<Final, Temp> {
    #[inline]
    fn is_closed(&self) -> bool {
        self.is_closed()
    }

    #[inline]
    fn close(&mut self) -> Result<(), ReplayFileWriteError> {
        self.close().map_err(|e| e.2)?;
        Ok(())
    }

    #[inline]
    fn next_frame(&mut self, timestamp_millis: TimestampMillis) -> Result<(), ReplayFileWriteError> {
        self.next_frame(timestamp_millis);
        Ok(())
    }

    #[inline]
    fn add_bookmark(&mut self, name: String) -> Result<(), ReplayFileWriteError> {
        self.add_bookmark(name);
        Ok(())
    }

    #[inline]
    fn insert_keyframe(&mut self, state: ByteVec, timestamp: TimestampMillis) -> Result<(), ReplayFileWriteError> {
        self.insert_keyframe(state, timestamp);
        Ok(())
    }

    #[inline]
    fn set_input(&mut self, input_buffer: InputBuffer) -> Result<(), ReplayFileWriteError> {
        self.set_input(input_buffer);
        Ok(())
    }

    #[inline]
    fn reset_console(&mut self) -> Result<(), ReplayFileWriteError> {
        self.reset_console();
        Ok(())
    }

    #[inline]
    fn write_memory(&mut self, address: UnsignedInteger, data: ByteVec) -> Result<(), ReplayFileWriteError> {
        self.write_memory(address, data);
        Ok(())
    }

    #[inline]
    fn set_speed(&mut self, speed: Speed) -> Result<(), ReplayFileWriteError> {
        self.set_speed(speed);
        Ok(())
    }

    #[inline]
    fn load_save_state(&mut self, state: ByteVec) -> Result<(), ReplayFileWriteError> {
        self.load_save_state(state);
        Ok(())
    }

    #[inline]
    fn get_errors(&mut self) -> Vec<ReplayFileWriteError> {
        self.poll_errors()
    }

    #[inline]
    fn mark_start(&mut self, timer_offset: TimestampMillis) -> Result<(), ReplayFileWriteError> {
        self.mark_start(timer_offset);
        Ok(())
    }

    #[inline]
    fn mark_end(&mut self) -> Result<(), ReplayFileWriteError> {
        self.mark_end();
        Ok(())
    }

    #[inline]
    fn change_counter(&mut self, counter: String, delta: SignedInteger) -> Result<(), ReplayFileWriteError> {
        self.change_counter(counter, delta);
        Ok(())
    }
}

// TODO: write unit tests

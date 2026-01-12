use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::*;
use rouille::{Response, Server};
use serde::Serialize;

pub struct SuperShuckieWebserver {
    backlog: Receiver<SuperShuckieServerCommand>,
    should_continue: Arc<AtomicBool>
}

impl SuperShuckieWebserver {
    /// Instantiate the server.
    pub fn new<S: ToSocketAddrs>(addr: S) -> Result<Self, String> {
        let (backlog_sender, backlog_receiver) = channel();
        let should_continue = Arc::new(AtomicBool::new(true));

        let server = Server::new(addr, move |request| {
            let url = request.url();
            match url.as_str() {
                "/stats" => {
                    let (responder, response) = channel();
                    let _ = backlog_sender.send(SuperShuckieServerCommand::Stats(responder));

                    match response.recv() {
                        Ok(n) => Response::json(&n),
                        _ => Response::empty_404()
                    }
                },
                "/mark-start" => {
                    let (responder, response) = channel();
                    let _ = backlog_sender.send(SuperShuckieServerCommand::MarkStart(responder));

                    match response.recv() {
                        Ok(true) => Response::empty_204(),
                        _ => Response::empty_404()
                    }
                },
                "/mark-end" => {
                    let (responder, response) = channel();
                    let _ = backlog_sender.send(SuperShuckieServerCommand::MarkEnd(responder));

                    match response.recv() {
                        Ok(true) => Response::empty_204(),
                        _ => Response::empty_404()
                    }
                },
                _ => Response::empty_400()
            }.with_unique_header("Access-Control-Allow-Origin", "*")
        }).map_err(|e| format!("Failed to make SuperShuckieServer:\n\n{e}"))?;

        let should_continue_inner = should_continue.clone();

        std::thread::spawn(move || {
            while should_continue_inner.load(Ordering::Relaxed) {
                server.poll();
            }
        });

        Ok(Self {
            backlog: backlog_receiver,
            should_continue
        })
    }

    /// Get the next server command.
    #[inline]
    pub fn next_server_command(&mut self) -> Option<SuperShuckieServerCommand> {
        self.backlog.try_recv().ok()
    }
}

impl Drop for SuperShuckieWebserver {
    fn drop(&mut self) {
        self.should_continue.store(false, Ordering::Relaxed);

        // clear the backlog
        while self.backlog.recv().is_ok() {}
    }
}

pub enum SuperShuckieServerCommand {
    Stats(Sender<Stats>),
    MarkStart(Sender<bool>),
    MarkEnd(Sender<bool>)
}

#[derive(Copy, Clone, Serialize)]
pub struct Stats {
    pub time_start: Option<u32>,
    pub time_end: Option<u32>,
    pub time_current: Option<u32>,

    pub total_elapsed_time: u32,
    pub total_elapsed_frames: u32,

    pub is_recording: bool,
    pub is_playing_back: bool,
    
    pub current_speed: f64
}

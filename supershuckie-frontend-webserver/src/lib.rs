use std::collections::BTreeMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::*;
use std::time::Duration;
use rouille::{Response, Server};
use serde::Serialize;

pub struct SuperShuckieWebserver {
    backlog: Receiver<SuperShuckieServerCommand>,
    should_continue: Arc<AtomicBool>
}

#[derive(Serialize)]
struct Error {
    error: String
}

impl SuperShuckieWebserver {
    /// Instantiate the server.
    pub fn new<S: ToSocketAddrs>(addr: S) -> Result<Self, String> {
        let (backlog_sender, backlog_receiver) = sync_channel(1024);
        let should_continue = Arc::new(AtomicBool::new(true));
        let emulator_not_available_error = || {
            fixup_response(Response::json(&Error {
                error: "failed (emulator is not available)".to_owned()
            }).with_status_code(503))
        };

        let server = Server::new(addr, move |request| {
            let url = request.url();
            fixup_response(match url.as_str() {
                "/stats" => {
                    let (responder, response) = channel();

                    if backlog_sender.try_send(SuperShuckieServerCommand::Stats(responder)).is_err() {
                        return emulator_not_available_error();
                    }

                    match response.recv_timeout(Duration::from_secs(60)) {
                        Ok(n) => Response::json(Arc::as_ref(&n)),
                        Err(_) => return emulator_not_available_error()
                    }
                },
                "/mark-start" => {
                    let (responder, response) = channel();

                    let offset = match request.get_param("offset") {
                        None => 0,
                        Some(n) => match n.parse() {
                            Ok(n) => n,
                            Err(_) => return fixup_response(
                                Response::json(&Error {
                                    error: format!("failed (can't parse {n} as an unsigned integer)")
                                }).with_status_code(400)
                            )
                        }
                    };

                    if backlog_sender.try_send(SuperShuckieServerCommand::MarkStart(responder, offset)).is_err() {
                        return emulator_not_available_error();
                    }

                    match response.recv_timeout(Duration::from_secs(60)) {
                        Ok(true) => Response::empty_204(),
                        Ok(false) => Response::json(&Error {
                            error: "error (probably not recording a replay)".to_owned()
                        }).with_status_code(404),
                        Err(_) => return emulator_not_available_error()
                    }
                },
                "/mark-end" => {
                    let (responder, response) = channel();
                    if backlog_sender.try_send(SuperShuckieServerCommand::MarkEnd(responder)).is_err() {
                        return emulator_not_available_error()
                    }

                    match response.recv_timeout(Duration::from_secs(60)) {
                        Ok(true) => Response::empty_204(),
                        Ok(false) => Response::json(&Error {
                            error: "error (probably not recording a replay)".to_owned()
                        }).with_status_code(404),
                        Err(_) => return emulator_not_available_error()
                    }
                },
                "/increment-counter" => {
                    let Some(name) = request.get_param("name") else {
                        return fixup_response(
                            Response::json(&Error {
                                error: "failed (missing the name parameter)".to_owned()
                            }).with_status_code(400)
                        )
                    };

                    let by = match request.get_param("by") {
                        None => 1,
                        Some(n) => match n.parse() {
                            Ok(n) => n,
                            Err(_) => return fixup_response(
                                Response::json(&Error {
                                    error: format!("failed (can't parse {n} as an integer)")
                                }).with_status_code(400)
                            )
                        }
                    };

                    let (sender, response) = channel();

                    if backlog_sender.try_send(SuperShuckieServerCommand::IncrementCounter(sender, name, by)).is_err() {
                        return emulator_not_available_error();
                    }

                    match response.recv_timeout(Duration::from_secs(60)) {
                        Ok(true) => Response::empty_204(),
                        Ok(false) => Response::json(&Error {
                            error: "error (probably not recording a replay)".to_owned()
                        }).with_status_code(404),
                        Err(_) => return emulator_not_available_error()
                    }
                }
                "/enumerate-replays" => {
                    let (sender, response) = channel();

                    if backlog_sender.try_send(SuperShuckieServerCommand::EnumerateReplays(sender)).is_err() {
                        return emulator_not_available_error();
                    }

                    match response.recv_timeout(Duration::from_secs(60)) {
                        Ok(n) => Response::json(&n),
                        Err(_) => return emulator_not_available_error()
                    }
                }
                "/set-playback-speed" => {
                    let speed = match request.get_param("speed") {
                        None => return fixup_response(
                            Response::json(&Error {
                                error: "failed (missing the speed parameter)".to_owned()
                            }).with_status_code(400)
                        ),
                        Some(n) => match n.parse::<f64>() {
                            Ok(n) if n.is_finite() && n.is_sign_positive() => n,
                            _ => return fixup_response(
                                Response::json(&Error {
                                    error: format!("failed (can't parse {n} as an float)")
                                }).with_status_code(400)
                            )
                        }
                    };

                    let (sender, response) = channel();

                    if backlog_sender.try_send(SuperShuckieServerCommand::SetPlaybackSpeed(sender, speed)).is_err() {
                        return emulator_not_available_error();
                    }

                    match response.recv_timeout(Duration::from_secs(60)) {
                        Ok(true) => Response::empty_204(),
                        Ok(false) => Response::json(&Error {
                            error: "error (probably not playing a game)".to_owned()
                        }).with_status_code(404),
                        Err(_) => return emulator_not_available_error()
                    }
                }
                "/load-replay" => {
                    let Some(name) = request.get_param("name") else {
                        return fixup_response(
                            Response::json(&Error {
                                error: "failed (missing the name parameter)".to_owned()
                            }).with_status_code(400)
                        )
                    };

                    let (sender, response) = channel();

                    if backlog_sender.try_send(SuperShuckieServerCommand::LoadReplay(sender, name)).is_err() {
                        return emulator_not_available_error();
                    }

                    match response.recv_timeout(Duration::from_secs(60)) {
                        Ok(true) => Response::empty_204(),
                        Ok(false) => Response::json(&Error {
                            error: "error (replay probably not found or is invalid)".to_owned()
                        }).with_status_code(404),
                        Err(_) => return emulator_not_available_error()
                    }
                }
                "/set-paused" => {
                    let paused = match request.get_param("paused") {
                        None => return fixup_response(
                            Response::json(&Error {
                                error: "failed (missing the frame parameter)".to_owned()
                            }).with_status_code(400)
                        ),
                        Some(n) => match n.parse() {
                            Ok(paused) => paused,
                            Err(_) => return fixup_response(
                                Response::json(&Error {
                                    error: format!("failed (can't parse {n} as an integer)")
                                }).with_status_code(400)
                            )
                        }
                    };

                    let (sender, response) = channel();

                    if backlog_sender.try_send(SuperShuckieServerCommand::SetPaused(sender, paused)).is_err() {
                        return emulator_not_available_error();
                    }

                    match response.recv_timeout(Duration::from_secs(60)) {
                        Ok(true) => Response::empty_204(),
                        Ok(false) => Response::json(&Error {
                            error: "error (unknown reason)".to_owned()
                        }).with_status_code(404),
                        Err(_) => return emulator_not_available_error()
                    }
                }
                "/go-to-frame" => {
                    let frame = match request.get_param("frame") {
                        None => return fixup_response(
                            Response::json(&Error {
                                error: "failed (missing the frame parameter)".to_owned()
                            }).with_status_code(400)
                        ),
                        Some(n) => match n.parse() {
                            Ok(n) => n,
                            Err(_) => return fixup_response(
                                Response::json(&Error {
                                    error: format!("failed (can't parse {n} as an integer)")
                                }).with_status_code(400)
                            )
                        }
                    };

                    let (sender, response) = channel();

                    if backlog_sender.try_send(SuperShuckieServerCommand::GoToFrame(sender, frame)).is_err() {
                        return emulator_not_available_error();
                    }

                    match response.recv_timeout(Duration::from_secs(60)) {
                        Ok(true) => Response::empty_204(),
                        Ok(false) => Response::json(&Error {
                            error: "error (probably not playing back a replay)".to_owned()
                        }).with_status_code(404),
                        Err(_) => return emulator_not_available_error()
                    }
                }
                "/client.js" => {
                    Response::text(include_str!("../js/client.js"))
                        .with_unique_header(
                            "Content-Type",
                            "text/javascript; charset=utf-8"
                        )
                }
                _ => Response::empty_400()
            })
        }).map_err(|e| format!("Failed to make SuperShuckieServer:\n\n{e}"))?;

        let should_continue_inner = should_continue.clone();

        std::thread::spawn(move || {
            while should_continue_inner.load(Ordering::Relaxed) {
                server.poll();
                std::thread::sleep(Duration::from_millis(1));
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

fn fixup_response(response: Response) -> Response {
    response.with_unique_header("Access-Control-Allow-Origin", "*")
}

impl Drop for SuperShuckieWebserver {
    fn drop(&mut self) {
        self.should_continue.store(false, Ordering::Relaxed);

        // clear the backlog
        while self.backlog.recv().is_ok() {}
    }
}

pub enum SuperShuckieServerCommand {
    Stats(Sender<Arc<Stats>>),
    MarkStart(Sender<bool>, u32),
    MarkEnd(Sender<bool>),
    IncrementCounter(Sender<bool>, String, i64),
    GoToFrame(Sender<bool>, u32),
    SetPaused(Sender<bool>, bool),
    LoadReplay(Sender<bool>, String),
    EnumerateReplays(Sender<Vec<String>>),
    SetPlaybackSpeed(Sender<bool>, f64),
}

#[derive(Clone, Serialize)]
pub struct Stats {
    pub time_start: Option<u32>,
    pub time_end: Option<u32>,
    pub time_offset: Option<u32>,
    pub time_current: Option<u32>,

    pub total_elapsed_time: u32,
    pub total_elapsed_frames: u32,

    pub is_recording: bool,
    pub is_playing_back: bool,
    pub is_paused: bool,

    pub counters: BTreeMap<String, i64>,
    
    pub current_speed: f64
}

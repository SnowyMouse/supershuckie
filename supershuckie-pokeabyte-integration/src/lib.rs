use std::borrow::Cow;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use tinyvec::{ArrayVec, TinyVec};
use crate::protocol::{Instruction, MetadataHeader, PokeAByteProtocolRequestPacket, PokeAByteProtocolRequestReadBlock, MAX_NUMBER_OF_READ_BLOCKS};
use crate::shared_memory::PokeAByteSharedMemory;

#[cfg(not(target_pointer_width = "64"))]
compile_error!("must be compiled for 64-bit");

// FIXME: this is not currently configurable
const POKEABYTE_UDP: &str = "127.0.0.1:55356";

pub enum PokeAByteWriteCommand {
    Write {
        address: u64,
        data: TinyVec<[u8; 16]>,
    },
    Freeze {
        address: u64,
        data: TinyVec<[u8; 16]>,
    },
    Unfreeze {
        address: u64
    }
}

pub struct PokeAByteIntegrationServer {
    session: Arc<Mutex<Option<PokeAByteSession>>>,
    server_close_notifier: Mutex<Receiver<()>>
}

/// All session-related data from Poke-A-Byte.
pub struct PokeAByteSession {
    /// Shared memory block.
    pub shared_memory: PokeAByteSharedMemory,

    /// Writes requested from Poke-A-Byte.
    pub writes: PokeAByteWriteQueue,

    /// Current setup configuration from the Poke-A-Byte client.
    pub config: PokeAByteSetup
}

/// Write queue from Poke-A-Byte.
pub struct PokeAByteWriteQueue {
    queue: Receiver<PokeAByteWriteCommand>
}

impl Iterator for PokeAByteWriteQueue {
    type Item = PokeAByteWriteCommand;
    fn next(&mut self) -> Option<Self::Item> {
        self.queue.try_recv().ok()
    }
}

/// Configuration shared from Poke-A-Byte.
#[derive(Debug)]
pub struct PokeAByteSetup {
    /// Block mapping.
    ///
    /// This indicates what RAM address in the game corresponds to what offset (and span) in the
    /// shared memory buffer.
    pub blocks: ArrayVec<[PokeAByteProtocolRequestReadBlock; MAX_NUMBER_OF_READ_BLOCKS]>,

    /// Suggested number of frames to skip, if any.
    ///
    /// The emulator can (and ideally should) respect this configuration.
    pub frame_skip: Option<u32>,

    _cant_let_you_instantiate_that_stair_fax: ()
}

impl Drop for PokeAByteIntegrationServer {
    fn drop(&mut self) {
        self.session = Arc::new(Mutex::new(None));
        let _ = self.server_close_notifier.lock().and_then(|i| Ok(i.recv()));
    }
}

impl PokeAByteIntegrationServer {
    /// Begin listening.
    pub fn begin_listen() -> Result<Self, PokeAByteError> {
        let socket = UdpSocket::bind(&POKEABYTE_UDP)
            .map_err(|e| PokeAByteError::SocketFailure { explanation: Cow::Owned(format!("Failed to bind: {e:?}")) })?;

        let _ = socket.set_read_timeout(Some(Duration::from_millis(500)));
        let _ = socket.set_write_timeout(Some(Duration::from_millis(500)));

        let (sender, receiver) = channel();

        let session = Arc::new(Mutex::new(None));
        let session_downgraded = Arc::downgrade(&session);

        let this = Self {
            session,
            server_close_notifier: Mutex::new(receiver)
        };

        let _ = std::thread::Builder::new().name("PokeAByteIntegrationServer".to_owned()).spawn(move || {
            PokeAByteIntegrationServer::thread(session_downgraded, socket, sender)
        });

        Ok(this)
    }

    /// Get the current session, if any.
    pub fn get_session(&self) -> MutexGuard<'_, Option<PokeAByteSession>> {
        self.session.lock().expect("could not get session???")
    }

    fn thread(session: Weak<Mutex<Option<PokeAByteSession>>>, socket: UdpSocket, close_notifier: Sender<()>) {
        let mut buffer = vec![0u8; 65536];

        let mut last_setup_user: Option<SocketAddr> = None;
        let mut writer: Option<Sender<PokeAByteWriteCommand>> = None;

        loop {
            let Some(promotion) = session.upgrade() else {
                if let Some(addr) = last_setup_user {
                    let _ = socket.send_to(&MetadataHeader::new_response(Instruction::Close).into_bytes(), addr);
                    eprintln!("[PABP] Disconnecting because server is being terminated/restarted");
                }
                drop(socket);
                let _ = close_notifier.send(());
                return
            };

            let Ok((len, addr)) = socket.recv_from(&mut buffer) else {
                continue
            };

            let bytes_received = &buffer.as_slice()[..len];
            let packet = match PokeAByteProtocolRequestPacket::parse_bytes(bytes_received) {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("[PABP] Error from client @ {addr}: {e:?}");
                    continue
                }
            };

            match packet {
                PokeAByteProtocolRequestPacket::Ping => {
                    let _ = socket.send_to(&MetadataHeader::new_response(Instruction::Ping).into_bytes(), addr);
                },
                PokeAByteProtocolRequestPacket::NoOp => {},
                PokeAByteProtocolRequestPacket::Close => {
                    last_setup_user = None;
                    continue;
                },
                PokeAByteProtocolRequestPacket::Setup { blocks, frame_skip } => {
                    let memory_size = blocks
                        .iter()
                        .map(|i| i.range.end)
                        .max()
                        .unwrap_or(0);

                    let mut session = promotion.lock().expect("Failed to lock: crash?");
                    *session = None; // For cleaning up the old SHM and clearing the file descriptor.

                    // Safety: We're going to zero-initialize this before we use it.
                    let mut shared_memory = unsafe { PokeAByteSharedMemory::new(memory_size) }
                        .expect("Failed to initialize shared memory");

                    let (writer_queue, writes_queue) = channel();

                    let writes = PokeAByteWriteQueue { queue: writes_queue };
                    writer = Some(writer_queue);

                    // note down the address
                    last_setup_user = Some(addr);
                    eprintln!("[PABP] Accepted new session from client @ {addr}");

                    // let Poke-A-Byte know that we're open for business, since zero initialization
                    // is not instant (though it'll probably still be quick)
                    let _ = socket.send_to(&MetadataHeader::new_response(Instruction::Setup).into_bytes(), addr);

                    // Zero-initialize
                    unsafe { shared_memory.get_memory_mut() }.fill(0);

                    *session = Some(PokeAByteSession {
                        shared_memory,
                        writes,
                        config: PokeAByteSetup {
                            blocks, frame_skip, _cant_let_you_instantiate_that_stair_fax: ()
                        },
                    });

                },
                PokeAByteProtocolRequestPacket::Write { data, address } => {
                    if Some(addr) != last_setup_user {
                        if last_setup_user.is_none() {
                            eprintln!("[PABP] Ignoring write from client @ {addr} (no session yet)");
                        }
                        else {
                            eprintln!("[PABP] Ignoring write from client @ {addr} (address mismatch)");
                        }
                        continue
                    }

                    if data.is_empty() {
                        continue
                    }

                    let Some(writer) = writer.as_ref() else {
                        continue
                    };

                    let _ = writer.send(PokeAByteWriteCommand::Write {
                        address, data: data.into()
                    });
                },
                PokeAByteProtocolRequestPacket::Freeze { address, data } => {
                    if Some(addr) != last_setup_user {
                        if last_setup_user.is_none() {
                            eprintln!("[PABP] Ignoring freeze from client @ {addr} (no session yet)");
                        }
                        else {
                            eprintln!("[PABP] Ignoring freeze from client @ {addr} (address mismatch)");
                        }
                        continue
                    }

                    if data.is_empty() {
                        continue;
                    }

                    let Some(writer) = writer.as_ref() else {
                        continue
                    };

                    let _ = writer.send(PokeAByteWriteCommand::Freeze {
                        address, data: data.into()
                    });
                }
                PokeAByteProtocolRequestPacket::Unfreeze { address } => {
                    if Some(addr) != last_setup_user {
                        if last_setup_user.is_none() {
                            eprintln!("[PABP] Ignoring freeze from client @ {addr} (no session yet)");
                        }
                        else {
                            eprintln!("[PABP] Ignoring freeze from client @ {addr} (address mismatch)");
                        }
                        continue
                    }

                    let Some(writer) = writer.as_ref() else {
                        continue
                    };

                    let _ = writer.send(PokeAByteWriteCommand::Unfreeze {
                        address
                    });
                }
            }
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum PokeAByteError {
    SharedMemoryFailure { explanation: Cow<'static, str> },
    SocketFailure { explanation: Cow<'static, str> },
    BadPacketFromClient { explanation: Cow<'static, str> }
}

mod shared_memory;
mod protocol;

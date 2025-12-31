use std::time::Duration;
use supershuckie_pokeabyte_integration::PokeAByteIntegrationServer;

// Simulate an integration server where all bytes count up by 1.
fn main() {
    let server = PokeAByteIntegrationServer::begin_listen().unwrap();

    let mut is_connected = false;

    loop {
        {
            std::thread::sleep(Duration::from_secs(1));

            let mut session_lock = server.get_session();
            let Some(session) = session_lock.as_mut() else {
                println!("Not currently connected... Please start Poke-A-Byte and load a mapper!");
                continue;
            };

            if !is_connected {
                is_connected = true;
                println!("Connected!");
            }

            let memory = unsafe { session.shared_memory.get_memory_mut() };

            for write in &mut session.writes {
                println!("Received a write request: address=0x{}, data={:02X?}", write.address, write.data);

                let start_address = u32::try_from(write.address).expect("start_address not u32"); // all of the blocks are stored as u32, so we don't expect address to exceed u32::MAX
                let end_address = start_address + write.data.len() as u32;

                for i in &session.config.blocks {
                    if start_address >= i.game_address && end_address <= i.game_address + i.range.len() as u32 {

                        let offset = start_address as usize - i.game_address as usize;
                        let data = memory
                            .get_mut(i.range.clone())
                            .expect("bad memory range???");
                        let asdf = &mut data[offset..][..write.data.len()];
                        asdf.copy_from_slice(write.data.as_slice());
                        break
                    }
                }
            }

            for i in memory {
                *i = i.wrapping_add(1);
            }

        }
    }
}

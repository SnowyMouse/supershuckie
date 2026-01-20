//! Replay file player functionality
//!
//! See [`ReplayFilePlayer`].

#[cfg(not(feature = "std"))]
use spin::Mutex;

#[cfg(not(feature = "std"))]
macro_rules! unwrap_mutex_lock {
    ($e:expr) => {$e};
}

#[cfg(feature = "std")]
use std::sync::Mutex;

#[cfg(feature = "std")]
macro_rules! unwrap_mutex_lock {
    ($e:expr) => {$e.unwrap()};
}

use alloc::borrow::Cow;
use alloc::format;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::borrow::ToOwned;
use core::mem::transmute;
use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use alloc::vec;
use crate::replay_file::{ReplayFileMetadata, ReplayHeaderBytes, ReplayHeaderRaw};
use crate::{apply_diff, BookmarkMetadata, ByteVec, KeyframeMetadata, Packet, PacketIO, PacketReadError, TimestampMillis, UnsignedInteger};
use crate::util::{decompress_data, launder_reference};

type KeyframeMap<'a> = BTreeMap<UnsignedInteger, Vec<&'a KeyframeMetadata>>;
type BookmarkMap<'a> = BTreeMap<String, Vec<&'a BookmarkMetadata>>;

/// Object that iterates through packets in a replay file.
pub struct ReplayFilePlayer {
    replay_file_metadata: ReplayFileMetadata,
    header_raw: ReplayHeaderRaw,
    patch_data: Option<Vec<u8>>,
    all_uncompressed_packets: Arc<Vec<Packet>>,
    keyframes: KeyframeMap<'static>,
    bookmarks: BookmarkMap<'static>,

    total_frame_count: UnsignedInteger,
    total_millis: TimestampMillis,

    compressed_blobs_decompressing: BTreeMap<usize, Option<Arc<Mutex<PacketDecompressionStatus>>>>,
    compressed_blobs_finished: BTreeMap<usize, Option<Arc<Vec<Packet>>>>,
    compressed_blob_uncompressed_packet_indices: Vec<usize>,
    cleanup_enabled: bool,

    next_uncompressed_packet_index: usize,
    next_compressed_packet_index: Option<usize>,

    #[cfg(feature = "std")]
    threading: bool
}

impl ReplayFilePlayer {
    /// Try to read a buffer.
    ///
    /// If `allow_some_corruption`, then the parser will break early if it detects a corrupted
    /// packet and there is still some sort of usable stream. Otherwise, it will return `Err`.
    pub fn new<B: AsRef<[u8]>>(data: B, allow_some_corruption: bool) -> Result<ReplayFilePlayer, ReplayFileReadError> {
        let buffer_bytes = data.as_ref();
        let Some(header_buffer) = buffer_bytes.get(..size_of::<ReplayHeaderBytes>()) else {
            return Err(ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("cannot read header") });
        };

        let header_buffer_bytes: &ReplayHeaderBytes = header_buffer.try_into().expect("should be able to convert array");
        let header_raw = ReplayHeaderRaw::from_bytes(header_buffer_bytes);
        let replay_file_metadata = header_raw
            .parse()
            .map_err(|e| ReplayFileReadError::InvalidReplayFile { explanation: Cow::Owned(format!("Failed to read header: {e}")) })?;

        let patch_start = header_buffer_bytes.len();
        let patch_length = usize::try_from(header_raw.patch_data_length)
            .map_err(|_| ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("Cannot read patch length (exceeds usize)") })?;
        let patch_end = patch_length.checked_add(patch_start)
            .ok_or_else(|| ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("Cannot read patch end (overflowed usize)") })?;

        let patch_data;
        if patch_length > 0 {
            let patch_range = patch_start..patch_end;
            let patch_bytes = buffer_bytes
                .get(patch_range)
                .ok_or_else(|| ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("Cannot read patch end (out-of-bounds)") })?;

            patch_data = Some(patch_bytes.to_owned());
        }
        else {
            patch_data = None;
        }

        let mut replay_data = buffer_bytes.get(patch_end..)
            .ok_or_else(|| ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("Cannot read replay data (out-of-bounds)") })?;

        let mut all_packets = Vec::new();

        while !replay_data.is_empty() {
            match Packet::read_all(&mut replay_data, header_raw.replay_version) {
                Ok(n) => all_packets.push(n),
                Err(_) if allow_some_corruption => break,
                Err(PacketReadError::NotEnoughData) => return Err(ReplayFileReadError::BrokenPacket { explanation: Cow::Borrowed("not enough data for a packet") }),
                Err(PacketReadError::ParseFail { explanation }) => return Err(ReplayFileReadError::BrokenPacket { explanation: Cow::Owned(format!("Parse failure: {explanation}")) })
            }
        }

        let mut current_keyframe_state = None;
        for i in &mut all_packets {
            match i {
                Packet::Keyframe { state, .. } => {
                    current_keyframe_state = Some(state.to_owned());
                }
                Packet::CompressedBlob { .. } => {
                    current_keyframe_state = None;
                },
                Packet::DeltaKeyframe { metadata, diff } => {
                    let Some(current) = current_keyframe_state.take() else {
                        if allow_some_corruption {
                            continue
                        }
                        return Err(ReplayFileReadError::BrokenPacket { explanation: Cow::Borrowed("Delta keyframe without a prior keyframe") })
                    };

                    let Some(applied) = apply_diff(current.as_slice(), diff.as_slice()) else {
                        if allow_some_corruption {
                            continue
                        }
                        return Err(ReplayFileReadError::BrokenPacket { explanation: Cow::Borrowed("Delta keyframe failed to apply diff") })
                    };

                    *i = Packet::Keyframe { metadata: core::mem::take(metadata), state: ByteVec::Heap(applied.clone()) };
                    current_keyframe_state = Some(ByteVec::Heap(applied));
                },
                _ => {}
            }
        }

        let all_packets = Arc::new(all_packets);

        let Some(first_packet) = all_packets.get(0) else {
            return Err(ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("No packets detected in replay file") })
        };

        match first_packet {
            Packet::CompressedBlob { keyframes, .. } => {
                if keyframes.is_empty() {
                    return Err(ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("Replay starts with a compressed blob with no keyframes") })
                }
            }
            Packet::Keyframe { .. } => {},
            _ => return Err(ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("Replay does not start with a keyframe") })
        }

        let mut all_keyframes = KeyframeMap::new();
        let mut all_bookmarks = BookmarkMap::new();

        let mut total_frame_count: UnsignedInteger = 0;
        let mut total_millis: UnsignedInteger = 0;

        macro_rules! add_keyframe {
            ($metadata:expr) => {{
                total_frame_count = $metadata.elapsed_frames;
                match all_keyframes.get_mut(&$metadata.elapsed_frames) {
                    Some(n) => n.push($metadata),
                    None => { all_keyframes.insert( $metadata.elapsed_frames, vec![$metadata]); }
                }
            }};
        }

        macro_rules! add_bookmark {
            ($metadata:expr) => {
                match all_bookmarks.get_mut(&$metadata.name) {
                    Some(n) => n.push($metadata),
                    None => { all_bookmarks.insert($metadata.name.clone(), vec![$metadata]); }
                }
            };
        }

        let mut compressed_blobs = BTreeMap::new();
        let mut compressed_blobs_finished = BTreeMap::new();
        let mut compressed_blob_indices = Vec::new();

        for (packet_index, packet) in all_packets.iter().enumerate() {
            match packet {
                Packet::CompressedBlob {
                    keyframes,
                    bookmarks,
                    uncompressed_size,
                    timestamp_end,
                    elapsed_frames_end,
                    ..
                } => {
                    // Vec works with up to isize maximum elements
                    if isize::try_from(*uncompressed_size).is_err() {
                        return Err(ReplayFileReadError::Other { explanation: Cow::Borrowed("Replay has a compressed blob that decompressed beyond the current architectural limits") });
                    }

                    compressed_blobs.insert(packet_index, None);
                    compressed_blobs_finished.insert(packet_index, None);
                    compressed_blob_indices.push(packet_index);

                    if keyframes.is_empty() {
                        return Err(ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("Replay has a compressed blob with no keyframes") })
                    }

                    #[allow(unused)]
                    for i in keyframes {
                        add_keyframe!(i)
                    }
                    for i in bookmarks {
                        add_bookmark!(i)
                    }

                    total_frame_count = *elapsed_frames_end;
                    total_millis = timestamp_end.0;
                },
                Packet::Keyframe { metadata, .. } => {
                    add_keyframe!(metadata);
                },
                Packet::NextFrame { timestamp_delta } => {
                    total_frame_count += 1;
                    total_millis += timestamp_delta.0;
                }
                Packet::Bookmark { metadata } => {
                    add_bookmark!(metadata);
                    total_millis = metadata.elapsed_millis.0;
                },
                _ => {}
            }
        }

        if all_keyframes.get(&0).is_none() {
            return Err(ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("Replay has no keyframe at index 0") })
        }

        let player = ReplayFilePlayer {
            patch_data,
            replay_file_metadata,
            keyframes: unsafe { transmute::<KeyframeMap, KeyframeMap<'static>>(all_keyframes) },
            bookmarks: unsafe { transmute::<BookmarkMap, BookmarkMap<'static>>(all_bookmarks) },
            all_uncompressed_packets: all_packets,
            next_uncompressed_packet_index: 0usize,
            next_compressed_packet_index: None,
            compressed_blob_uncompressed_packet_indices: compressed_blob_indices,
            compressed_blobs_decompressing: compressed_blobs,
            compressed_blobs_finished,
            total_frame_count,
            total_millis: TimestampMillis(total_millis),
            cleanup_enabled: true,
            header_raw: *header_raw,

            #[cfg(feature = "std")]
            threading: false
        };

        Ok(player)
    }

    /// Get the total frame count.
    pub fn get_total_frames(&self) -> UnsignedInteger {
        self.total_frame_count
    }

    /// Get the total milliseconds of the replay.
    pub fn get_total_milliseconds(&self) -> TimestampMillis {
        self.total_millis
    }

    /// Enable decompression on a separate thread.
    ///
    /// The next compressed blob will be automatically decompressed in the background.
    ///
    /// This cannot be turned off once activated.
    ///
    /// The `std` feature is required to enable this.
    #[cfg(feature = "std")]
    pub fn enable_threading(&mut self) {
        self.threading = true;
    }

    /// Get a reference to a map of keyframes.
    ///
    /// The key is the frame count.
    pub fn all_keyframes(&self) -> &BTreeMap<UnsignedInteger, Vec<&KeyframeMetadata>> {
        &self.keyframes
    }

    /// Get a reference to a map of bookmarks.
    ///
    /// The key is the bookmark name.
    pub fn all_bookmarks(&self) -> &BTreeMap<String, Vec<&BookmarkMetadata>> {
        &self.bookmarks
    }

    /// Get all top-level uncompressed packets.
    pub fn all_uncompressed_packets(&self) -> &[Packet] {
        self.all_uncompressed_packets.as_slice()
    }

    /// Get the replay metadata.
    pub fn get_replay_metadata(&self) -> &ReplayFileMetadata {
        &self.replay_file_metadata
    }

    /// Get the patch data, if any.
    pub fn get_patch_data(&self) -> Option<&[u8]> {
        self.patch_data.as_ref().map(|i| i.as_slice())
    }

    /// Go to the given keyframe.
    ///
    /// On failure, `Err` is returned.
    pub fn go_to_keyframe(&mut self, keyframe_frames_index: UnsignedInteger) -> Result<(), ReplaySeekError> {
        if self.keyframes.get(&keyframe_frames_index).is_none() {
            return Err(ReplaySeekError::NoSuchKeyframe {
                given: keyframe_frames_index,
                best: self.keyframes.keys().copied().filter(|i| *i <= keyframe_frames_index).max().expect("there is always a keyframe at frame index 0")
            })
        };

        self.next_compressed_packet_index = None;

        for (uncompressed_index, packet) in self.all_uncompressed_packets.iter().enumerate() {
            match packet {
                Packet::Keyframe { metadata, .. } => {
                    if metadata.elapsed_frames == keyframe_frames_index {
                        self.next_uncompressed_packet_index = uncompressed_index;
                        return Ok(());
                    }
                },
                Packet::CompressedBlob { keyframes, .. } => {
                    if keyframes.iter().any(|k| k.elapsed_frames == keyframe_frames_index) {
                        self.next_uncompressed_packet_index = uncompressed_index;
                        break;
                    }
                },
                _ => continue
            }
        }

        if let Err(error) = self.decompress_immediately(self.next_uncompressed_packet_index) {
            return Err(ReplaySeekError::ReadError { error })
        }

        let decompressed_packets = self.compressed_blobs_finished
            .get(&self.next_uncompressed_packet_index)
            .expect("somehow did not find the blob we just found in compressed_blobs_finished...")
            .as_ref()
            .expect("somehow the blob we just decompressed is not decompressed");

        for (subpacket_index, packet) in decompressed_packets.iter().enumerate() {
            match packet {
                Packet::Keyframe { metadata, .. } => {
                    if metadata.elapsed_frames == keyframe_frames_index {
                        self.next_compressed_packet_index = Some(subpacket_index);
                        return Ok(())
                    }
                },
                _ => continue
            }
        }

        unreachable!("failed to find keyframe somehow even though we somehow had it in self.keyframes...");
    }

    fn decompress_immediately(&mut self, blob_packet_index: usize) -> Result<(), ReplayFileReadError> {
        let Some(Packet::CompressedBlob { compressed_data, uncompressed_size, .. }) = self.all_uncompressed_packets.get(blob_packet_index) else {
            panic!("decompress_immediately on {blob_packet_index} failed because it's not a compressed blob packet...")
        };

        let decompressed_packets = self.compressed_blobs_finished
            .get_mut(&blob_packet_index)
            .expect("compressed blob should be in finished cache");

        let working_blob = self.compressed_blobs_decompressing
            .get_mut(&blob_packet_index)
            .expect("compressed blob should be in working cache");

        if decompressed_packets.is_some() {
            return Ok(())
        }

        loop {
            let Some(working_blob_ref) = working_blob.as_ref() else {
                // we have to decompress on the main thread. sad.
                let packets = decompress_compressed_blob(
                    &self.header_raw,
                    compressed_data.as_slice(),
                    usize::try_from(*uncompressed_size).expect("we checked uncompressed size converting earlier")
                )?;
                *decompressed_packets = Some(packets);
                return Ok(());
            };

            let status = unwrap_mutex_lock!(working_blob_ref.lock());
            match &*status {
                PacketDecompressionStatus::InProgress => {
                    continue;
                }
                PacketDecompressionStatus::Failed { error } => {
                    let error = error.clone();
                    drop(status);
                    *working_blob = None;
                    return Err(error)
                }
                PacketDecompressionStatus::Decompressed { packets } => {
                    let packets = packets.clone();
                    drop(status);
                    *decompressed_packets = Some(packets);
                    *working_blob = None;
                    return Ok(());
                }
            }
        }
    }

    /// Get the next packet in the stream.
    ///
    /// If there is no packet, `Ok(None)` will be returned.
    pub fn next_packet(&mut self) -> Result<Option<&Packet>, ReplayFileReadError> {
        let packet_index = self.next_uncompressed_packet_index;
        if packet_index >= self.all_uncompressed_packets.len() {
            return Ok(None)
        }

        self.hint_decompress_next_blob_and_cleanup();

        // SAFETY: This will never be mutated or moved.
        let next_packet = unsafe { launder_reference({
            match self.all_uncompressed_packets.get(packet_index) {
                Some(n) => n,
                None => return Ok(None)
            }
        }) };

        if let Packet::CompressedBlob { .. } = next_packet {
            self.decompress_immediately(packet_index)?;

            // SAFETY: the call to next_packet() errors because we're still borrowing it even if we
            // will never actually do anything with the reference after returning
            let packets = unsafe { launder_reference(&self.compressed_blobs_finished) }
                .get(&packet_index)
                .expect("compressed blob not found in finished cache")
                .as_ref()
                .expect("should be decompressed but wasn't for some reason???");

            let inner_index = match self.next_compressed_packet_index {
                Some(n) => {
                    self.next_compressed_packet_index = Some(n + 1);
                    n
                },
                None => {
                    self.next_compressed_packet_index = Some(1);
                    0
                }
            };

            match packets.get(inner_index) {
                Some(n) => Ok(Some(n)),
                None => {
                    self.next_compressed_packet_index = None;
                    self.next_uncompressed_packet_index += 1;
                    self.next_packet()
                }
            }
        }
        else {
            self.next_uncompressed_packet_index += 1;
            Ok(Some(next_packet))
        }
    }

    /// Decompress all blobs.
    pub fn decompress_all_blobs(&mut self) {
        self.cleanup_enabled = false;

        for (index, packet) in self.all_uncompressed_packets.clone().iter().enumerate() {
            if let Packet::CompressedBlob { .. } = packet {
                let _ = self.decompress_immediately(index);
            }
        }
    }

    fn hint_decompress_next_blob_and_cleanup(&mut self) {
        let current_frame_index = self.next_uncompressed_packet_index;

        if self.cleanup_enabled {
            let last_compressed_blob = self
                .compressed_blob_uncompressed_packet_indices
                .iter()
                .copied()
                .filter(|frame_index| *frame_index < current_frame_index)
                .last();

            if let Some(last_compressed_blob_packet_index) = last_compressed_blob {
                for i in 0..last_compressed_blob_packet_index {
                    self.compressed_blobs_finished.insert(i, None);
                    self.compressed_blobs_decompressing.insert(i, None);
                }
            }
        }

        #[cfg(feature = "std")]
        if self.threading {
            let next_compressed_blob = self
                .compressed_blob_uncompressed_packet_indices
                .iter()
                .copied()
                .filter(|frame_index| *frame_index > current_frame_index)
                .next();

            if let Some(next_compressed_blob_index) = next_compressed_blob {
                self.decompress_blob_threaded(next_compressed_blob_index);
            }
        }
    }

    #[cfg(feature = "std")]
    fn decompress_blob_threaded(&mut self, blob_index: usize) {
        if !self.threading {
            return;
        }

        if self.compressed_blobs_finished[&blob_index].is_some() {
            return;
        }

        let q = self.compressed_blobs_decompressing
            .get_mut(&blob_index)
            .expect("compressed_blobs_decompressing exploded");

        let Some(status) = q else {
            // Not decompressed; start decompression...

            let status = Arc::new(Mutex::new(PacketDecompressionStatus::InProgress));
            *q = Some(status.clone());
            let status_ref = Arc::downgrade(&status);
            let packets = self.all_uncompressed_packets.clone();
            let header = self.header_raw;

            match std::thread::Builder::new()
                .name("ReplayFilePlayer-decompression-thread".to_owned())
                .spawn(move || {
                    let Packet::CompressedBlob { uncompressed_size, compressed_data, .. } = packets
                        .get(blob_index)
                        .expect("failed to get packet") else {
                        panic!("compressed blob wasn't a compressed blob NOOOOO")
                    };
                    let decompressed = decompress_compressed_blob(&header, compressed_data.as_slice(), usize::try_from(*uncompressed_size).expect("we checked this could be a usize!"));
                    let Some(r) = status_ref.upgrade() else {
                        return
                    };

                    let mut r = unwrap_mutex_lock!(r.lock());

                    match decompressed {
                        Ok(n) => {
                            *r = PacketDecompressionStatus::Decompressed { packets: n }
                        },
                        Err(error) => {
                            *r = PacketDecompressionStatus::Failed { error }
                        }
                    }

                }) {
                Ok(_) => {
                    return
                },
                Err(_) => {
                    *q = None;
                    return
                }
            }
        };

        // Decompression was at least started at some point?

        let lock;
        #[cfg(feature = "std")]
        {
            lock = status.try_lock().ok();
        }

        #[cfg(not(feature = "std"))]
        {
            lock = status.try_lock();
        }

        if let Some(f) = lock.as_ref() {
            match &**f {
                PacketDecompressionStatus::InProgress => return,
                PacketDecompressionStatus::Failed { .. } => return,
                PacketDecompressionStatus::Decompressed { packets } => {
                    self.compressed_blobs_finished.insert(blob_index, Some(packets.clone()));
                }
            }
        }
        else {
            return
        }

        drop(lock);
        *q = None;
    }
}

/// An error when seeking to a given a keyframe.
#[derive(Clone, PartialEq, Debug)]
pub enum ReplaySeekError {
    /// No keyframe at the given frame index.
    ///
    /// The keyframe before the given frame index is provided at `best`, instead.
    #[allow(missing_docs)]
    NoSuchKeyframe { given: UnsignedInteger, best: UnsignedInteger },

    /// An error occurred when seeking (usually a decompression error).
    #[allow(missing_docs)]
    ReadError { error: ReplayFileReadError }
}

/// An error that occurred when reading
#[derive(Clone, PartialEq, Debug)]
#[allow(missing_docs)]
pub enum ReplayFileReadError {
    InvalidReplayFile { explanation: Cow<'static, str> },
    BrokenPacket { explanation: Cow<'static, str> },
    EndOfStream,
    Other { explanation: Cow<'static, str> }
}

fn decompress_compressed_blob(header: &ReplayHeaderRaw, blob_data: &[u8], uncompressed_size: usize) -> Result<Arc<Vec<Packet>>, ReplayFileReadError> {
    let decompressed_data = decompress_data(blob_data, uncompressed_size)
        .map_err(|e| ReplayFileReadError::Other { explanation: Cow::Owned(format!("Decompression error: {e}")) })?;

    let mut b = decompressed_data.as_slice();
    let mut packets = Vec::new();

    while !b.is_empty() {
        packets.push(
            Packet::read_all(&mut b, header.replay_version).map_err(|i| ReplayFileReadError::BrokenPacket { explanation: Cow::Owned(format!("Failed to read packet - {i:?}")) })?
        )
    }

    let Some(Packet::Keyframe { state, .. }) = packets.get(0) else {
        return Err(ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("first packet in a blob was not a keyframe") });
    };

    let mut current_state = state.to_owned();
    for i in &mut packets {
        match i {
            Packet::Keyframe { state, .. } => {
                current_state.clear();
                current_state.extend_from_slice(state.as_slice());
            },

            Packet::DeltaKeyframe { diff, metadata } => {
                match apply_diff(current_state.as_slice(), diff.as_slice()) {
                    Some(n) => current_state = ByteVec::Heap(n),
                    None => return Err(ReplayFileReadError::InvalidReplayFile { explanation: Cow::Borrowed("de-diffing error") })
                }

                let new_keyframe = Packet::Keyframe {
                    metadata: core::mem::take(metadata),
                    state: current_state.clone()
                };

                *i = new_keyframe;
            },

            _ => {}
        }
    }

    Ok(Arc::new(packets))
}

#[derive(Clone)]
#[cfg_attr(not(feature = "std"), expect(dead_code))]
enum PacketDecompressionStatus {
    InProgress,
    Failed { error: ReplayFileReadError },
    Decompressed { packets: Arc<Vec<Packet>> }
}

// TODO: WRITE UNIT TESTS

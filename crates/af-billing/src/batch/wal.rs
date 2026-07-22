use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{self, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{
    buckets::{BillingBuckets, checked_next_sequence},
    types::{
        BillingBatch, BillingBatchError, BillingBatchEvent, BillingBatchRecordOutcome,
        BillingWriterId,
    },
};
use af_domain::BillingReservationId;

mod codec;

use codec::{
    DecodedFile, SequencedEvent, create_active_file, decode_file, encode_record,
    validate_sealed_range,
};

const LOCK_FILE_NAME: &str = ".billing-wal.lock";
const ACTIVE_FILE_NAME: &str = "active.wal";
const SEGMENT_PREFIX: &str = "segment-";
const SEGMENT_SUFFIX: &str = ".wal";

pub(super) struct PreparedSegment {
    pub(super) segment_key: String,
    pub(super) batch: Arc<BillingBatch>,
}

struct ActiveSegment {
    file: Option<File>,
    base_sequence: u64,
    last_sequence: u64,
    events: Vec<SequencedEvent>,
    buckets: BillingBuckets,
}

struct SealedSegment {
    key: String,
    path: PathBuf,
    batch: Arc<BillingBatch>,
}

pub(super) struct WalStore {
    directory: PathBuf,
    active_path: PathBuf,
    _lock_file: File,
    writer_id: BillingWriterId,
    active: ActiveSegment,
    sealed: VecDeque<SealedSegment>,
    pending_events: HashMap<BillingReservationId, SequencedEvent>,
    poisoned: bool,
}

impl WalStore {
    pub(super) fn open(directory: &Path) -> Result<Self, BillingBatchError> {
        fs::create_dir_all(directory).map_err(map_io)?;
        let lock_path = directory.join(LOCK_FILE_NAME);
        let lock_file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(map_io)?;
        match fs2::FileExt::try_lock_exclusive(&lock_file) {
            Ok(()) => {}
            Err(error) if is_lock_contention(&error) => {
                return Err(BillingBatchError::WalLocked);
            }
            Err(_) => return Err(BillingBatchError::WalIo),
        }

        let active_path = directory.join(ACTIVE_FILE_NAME);
        let mut sealed_files = Vec::new();
        for entry in fs::read_dir(directory).map_err(map_io)? {
            let entry = entry.map_err(map_io)?;
            let file_type = entry.file_type().map_err(map_io)?;
            if !file_type.is_file() {
                continue;
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| BillingBatchError::CorruptWal)?;
            if name == ACTIVE_FILE_NAME || name == LOCK_FILE_NAME {
                continue;
            }
            if name.starts_with(SEGMENT_PREFIX) || name.ends_with(SEGMENT_SUFFIX) {
                let (start, end) = parse_segment_name(&name)?;
                sealed_files.push((start, end, name, entry.path()));
            }
        }
        sealed_files.sort_by_key(|(start, _, _, _)| *start);

        let mut decoded_sealed = Vec::with_capacity(sealed_files.len());
        for (start, end, key, path) in sealed_files {
            let decoded = decode_file(&path, false)?;
            validate_sealed_range(&decoded, start, end)?;
            decoded_sealed.push((start, end, key, path, decoded));
        }
        let decoded_active = active_path
            .exists()
            .then(|| decode_file(&active_path, true))
            .transpose()?;

        let writer_id = decoded_sealed
            .first()
            .map(|(_, _, _, _, decoded)| decoded.header.writer_id)
            .or_else(|| {
                decoded_active
                    .as_ref()
                    .map(|decoded| decoded.header.writer_id)
            })
            .unwrap_or_else(BillingWriterId::generate);
        for (_, _, _, _, decoded) in &decoded_sealed {
            if decoded.header.writer_id != writer_id {
                return Err(BillingBatchError::CorruptWal);
            }
        }
        if decoded_active
            .as_ref()
            .is_some_and(|decoded| decoded.header.writer_id != writer_id)
        {
            return Err(BillingBatchError::CorruptWal);
        }

        validate_segment_continuity(&decoded_sealed, decoded_active.as_ref())?;

        let mut pending_events = HashMap::new();
        let mut sealed = VecDeque::with_capacity(decoded_sealed.len());
        for (start, end, key, path, decoded) in decoded_sealed {
            register_pending(&mut pending_events, &decoded.events)?;
            let batch = Arc::new(batch_from_events(writer_id, start, end, &decoded.events)?);
            sealed.push_back(SealedSegment { key, path, batch });
        }

        let previous_sequence = sealed
            .back()
            .map_or(0, |segment| segment.batch.end_sequence());
        let (active, active_file) = match decoded_active {
            Some(decoded) => {
                register_pending(&mut pending_events, &decoded.events)?;
                let buckets = buckets_from_events(&decoded.events)?;
                let mut file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&active_path)
                    .map_err(map_io)?;
                file.seek(SeekFrom::End(0)).map_err(map_io)?;
                (
                    ActiveSegment {
                        file: None,
                        base_sequence: decoded.header.base_sequence,
                        last_sequence: decoded
                            .events
                            .last()
                            .map_or(decoded.header.last_sequence, |event| event.sequence),
                        events: decoded.events,
                        buckets,
                    },
                    file,
                )
            }
            None => {
                let file = create_active_file(&active_path, writer_id, previous_sequence)?;
                (
                    ActiveSegment {
                        file: None,
                        base_sequence: previous_sequence,
                        last_sequence: previous_sequence,
                        events: Vec::new(),
                        buckets: BillingBuckets::default(),
                    },
                    file,
                )
            }
        };
        let mut active = active;
        active.file = Some(active_file);

        Ok(Self {
            directory: directory.to_path_buf(),
            active_path,
            _lock_file: lock_file,
            writer_id,
            active,
            sealed,
            pending_events,
            poisoned: false,
        })
    }

    pub(super) const fn writer_id(&self) -> BillingWriterId {
        self.writer_id
    }

    pub(super) fn pending_event_count(&self) -> usize {
        self.pending_events.len()
    }

    pub(super) fn append(
        &mut self,
        event: BillingBatchEvent,
    ) -> Result<BillingBatchRecordOutcome, BillingBatchError> {
        self.ensure_usable()?;
        if let Some(existing) = self.pending_events.get(&event.event_id()) {
            if existing.event == event {
                return Ok(BillingBatchRecordOutcome::Existing {
                    sequence: existing.sequence,
                });
            }
            return Err(BillingBatchError::EventConflict);
        }

        // 必须先完成 checked 预演，避免 WAL 已追加后才发现内存分桶无法表示。
        let patch = self.active.buckets.preview(event)?;
        let sequence = checked_next_sequence(self.active.last_sequence)?;
        let record = encode_record(sequence, event);
        let Some(file) = self.active.file.as_mut() else {
            self.poisoned = true;
            return Err(BillingBatchError::WalPoisoned);
        };
        if file.seek(SeekFrom::End(0)).is_err()
            || file.write_all(&record).is_err()
            || file.sync_data().is_err()
        {
            // 写入或同步失败后的持久化结果未知，当前实例禁止继续追加；重启会按校验和恢复。
            self.poisoned = true;
            return Err(BillingBatchError::WalIo);
        }

        let sequenced = SequencedEvent { sequence, event };
        self.active.buckets.apply(patch);
        self.active.events.push(sequenced);
        self.active.last_sequence = sequence;
        self.pending_events.insert(event.event_id(), sequenced);
        Ok(BillingBatchRecordOutcome::Applied { sequence })
    }

    pub(super) fn prepare_flush(&mut self) -> Result<Option<PreparedSegment>, BillingBatchError> {
        self.ensure_usable()?;
        if self.sealed.is_empty() && !self.active.events.is_empty() {
            self.seal_active()?;
        }
        Ok(self.sealed.front().map(|segment| PreparedSegment {
            segment_key: segment.key.clone(),
            batch: Arc::clone(&segment.batch),
        }))
    }

    pub(super) fn confirm_flush(&mut self, segment_key: &str) -> Result<(), BillingBatchError> {
        self.ensure_usable()?;
        let Some(segment) = self.sealed.front() else {
            return Err(BillingBatchError::CorruptWal);
        };
        if segment.key != segment_key {
            return Err(BillingBatchError::CorruptWal);
        }
        match fs::remove_file(&segment.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(BillingBatchError::WalIo),
        }
        let segment = self
            .sealed
            .pop_front()
            .ok_or(BillingBatchError::CorruptWal)?;
        for event_id in segment.batch.event_ids() {
            self.pending_events.remove(event_id);
        }
        Ok(())
    }

    fn seal_active(&mut self) -> Result<(), BillingBatchError> {
        let start_sequence = checked_next_sequence(self.active.base_sequence)?;
        let end_sequence = self.active.last_sequence;
        if start_sequence > end_sequence || self.active.events.is_empty() {
            return Err(BillingBatchError::CorruptWal);
        }
        let event_ids = self
            .active
            .events
            .iter()
            .map(|event| event.event.event_id())
            .collect();
        let batch = Arc::new(self.active.buckets.clone().into_batch(
            self.writer_id,
            start_sequence,
            end_sequence,
            event_ids,
        )?);
        let key = segment_name(start_sequence, end_sequence);
        let path = self.directory.join(&key);
        if path.exists() {
            return Err(BillingBatchError::CorruptWal);
        }

        let Some(active_file) = self.active.file.take() else {
            self.poisoned = true;
            return Err(BillingBatchError::WalPoisoned);
        };
        if active_file.sync_all().is_err() {
            self.active.file = Some(active_file);
            self.poisoned = true;
            return Err(BillingBatchError::WalIo);
        }
        drop(active_file);
        if fs::rename(&self.active_path, &path).is_err() {
            self.active.file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.active_path)
                .ok();
            if self.active.file.is_none() {
                self.poisoned = true;
            }
            return Err(BillingBatchError::WalIo);
        }

        let new_active = match create_active_file(&self.active_path, self.writer_id, end_sequence) {
            Ok(file) => file,
            Err(error) => {
                // 已封存分段仍然完整；禁止当前实例继续写入，重启后会创建新的活动文件。
                self.sealed.push_back(SealedSegment { key, path, batch });
                self.poisoned = true;
                return Err(error);
            }
        };
        self.sealed.push_back(SealedSegment { key, path, batch });
        self.active = ActiveSegment {
            file: Some(new_active),
            base_sequence: end_sequence,
            last_sequence: end_sequence,
            events: Vec::new(),
            buckets: BillingBuckets::default(),
        };
        Ok(())
    }

    fn ensure_usable(&self) -> Result<(), BillingBatchError> {
        if self.poisoned {
            Err(BillingBatchError::WalPoisoned)
        } else {
            Ok(())
        }
    }
}

fn validate_segment_continuity(
    sealed: &[(u64, u64, String, PathBuf, DecodedFile)],
    active: Option<&DecodedFile>,
) -> Result<(), BillingBatchError> {
    let mut previous_end: Option<u64> = None;
    for (start, end, _, _, _) in sealed {
        if start > end
            || previous_end.is_some_and(|previous| previous.checked_add(1) != Some(*start))
        {
            return Err(BillingBatchError::CorruptWal);
        }
        previous_end = Some(*end);
    }
    if let (Some(previous), Some(active)) = (previous_end, active)
        && active.header.base_sequence != previous
    {
        return Err(BillingBatchError::CorruptWal);
    }
    Ok(())
}

fn register_pending(
    pending: &mut HashMap<BillingReservationId, SequencedEvent>,
    events: &[SequencedEvent],
) -> Result<(), BillingBatchError> {
    for event in events {
        if pending.insert(event.event.event_id(), *event).is_some() {
            return Err(BillingBatchError::CorruptWal);
        }
    }
    Ok(())
}

fn buckets_from_events(events: &[SequencedEvent]) -> Result<BillingBuckets, BillingBatchError> {
    let mut buckets = BillingBuckets::default();
    for event in events {
        let patch = buckets
            .preview(event.event)
            .map_err(|_| BillingBatchError::CorruptWal)?;
        buckets.apply(patch);
    }
    Ok(buckets)
}

fn batch_from_events(
    writer_id: BillingWriterId,
    start_sequence: u64,
    end_sequence: u64,
    events: &[SequencedEvent],
) -> Result<BillingBatch, BillingBatchError> {
    let buckets = buckets_from_events(events)?;
    let event_ids = events.iter().map(|event| event.event.event_id()).collect();
    buckets
        .into_batch(writer_id, start_sequence, end_sequence, event_ids)
        .map_err(|_| BillingBatchError::CorruptWal)
}

fn segment_name(start: u64, end: u64) -> String {
    format!("{SEGMENT_PREFIX}{start:020}-{end:020}{SEGMENT_SUFFIX}")
}

fn parse_segment_name(name: &str) -> Result<(u64, u64), BillingBatchError> {
    let range = name
        .strip_prefix(SEGMENT_PREFIX)
        .and_then(|value| value.strip_suffix(SEGMENT_SUFFIX))
        .ok_or(BillingBatchError::CorruptWal)?;
    let (start, end) = range.split_once('-').ok_or(BillingBatchError::CorruptWal)?;
    if start.len() != 20 || end.len() != 20 {
        return Err(BillingBatchError::CorruptWal);
    }
    let start = start.parse().map_err(|_| BillingBatchError::CorruptWal)?;
    let end = end.parse().map_err(|_| BillingBatchError::CorruptWal)?;
    if start == 0 || start > end || end > i64::MAX as u64 {
        return Err(BillingBatchError::CorruptWal);
    }
    Ok((start, end))
}

fn map_io(_: io::Error) -> BillingBatchError {
    BillingBatchError::WalIo
}

fn is_lock_contention(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::WouldBlock
        || error.kind() == io::ErrorKind::PermissionDenied
        // Windows 的 LockFileEx 以 ERROR_LOCK_VIOLATION 表示同一文件已被独占锁定。
        || error.raw_os_error() == Some(33)
}

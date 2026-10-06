use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

use af_domain::{BillingReservationId, ChannelId, QuotaDelta, TokenId, UserId};

use crate::batch::{
    buckets::checked_next_sequence,
    types::{
        BillingBatchError, BillingBatchEvent, BillingWriterId, ChannelBillingDelta,
        TokenBillingDelta, UserBillingDelta,
    },
};

const HEADER_MAGIC: &[u8; 8] = b"AFBWAL01";
const HEADER_VERSION: u16 = 1;
const HEADER_SIZE: usize = 48;
const RECORD_MAGIC: &[u8; 4] = b"EVT1";
const RECORD_PAYLOAD_SIZE: usize = 100;
const RECORD_SIZE: usize = 104;

pub(super) struct FileHeader {
    pub(super) writer_id: BillingWriterId,
    pub(super) base_sequence: u64,
    pub(super) last_sequence: u64,
}

#[derive(Clone, Copy)]
pub(super) struct SequencedEvent {
    pub(super) sequence: u64,
    pub(super) event: BillingBatchEvent,
}

pub(super) struct DecodedFile {
    pub(super) header: FileHeader,
    pub(super) events: Vec<SequencedEvent>,
}

pub(super) fn create_active_file(
    path: &Path,
    writer_id: BillingWriterId,
    base_sequence: u64,
) -> Result<File, BillingBatchError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)
        .map_err(map_io)?;
    write_header(&mut file, writer_id, base_sequence, base_sequence).map_err(map_io)?;
    file.sync_all().map_err(map_io)?;
    file.seek(SeekFrom::End(0)).map_err(map_io)?;
    Ok(file)
}

pub(super) fn decode_file(path: &Path, active: bool) -> Result<DecodedFile, BillingBatchError> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(active)
        .open(path)
        .map_err(map_io)?;
    let mut header_bytes = [0_u8; HEADER_SIZE];
    file.read_exact(&mut header_bytes)
        .map_err(|_| BillingBatchError::CorruptWal)?;
    let header = decode_header(&header_bytes)?;
    let mut events = Vec::new();
    let mut expected_sequence = header.base_sequence;
    let mut valid_length = HEADER_SIZE as u64;
    loop {
        let mut record = [0_u8; RECORD_SIZE];
        let mut read = 0;
        while read < RECORD_SIZE {
            match file.read(&mut record[read..]) {
                Ok(0) => break,
                Ok(bytes) => read += bytes,
                Err(_) => return Err(BillingBatchError::WalIo),
            }
        }
        if read == 0 {
            break;
        }
        if read < RECORD_SIZE {
            if !active {
                return Err(BillingBatchError::CorruptWal);
            }
            // 崩溃只允许留下活动文件的半条尾记录；完整记录的校验失败必须关闭启动。
            file.set_len(valid_length).map_err(map_io)?;
            file.sync_all().map_err(map_io)?;
            break;
        }
        let event = decode_record(&record)?;
        expected_sequence =
            checked_next_sequence(expected_sequence).map_err(|_| BillingBatchError::CorruptWal)?;
        if event.sequence != expected_sequence {
            return Err(BillingBatchError::CorruptWal);
        }
        valid_length = valid_length
            .checked_add(RECORD_SIZE as u64)
            .ok_or(BillingBatchError::CorruptWal)?;
        events.push(event);
    }

    let actual_last = events
        .last()
        .map_or(header.base_sequence, |event| event.sequence);
    if header.last_sequence > actual_last
        || (events.is_empty() && header.last_sequence != header.base_sequence)
    {
        return Err(BillingBatchError::CorruptWal);
    }
    Ok(DecodedFile { header, events })
}

pub(super) fn encode_record(sequence: u64, event: BillingBatchEvent) -> [u8; RECORD_SIZE] {
    let mut record = [0_u8; RECORD_SIZE];
    record[0..4].copy_from_slice(RECORD_MAGIC);
    record[4..12].copy_from_slice(&sequence.to_le_bytes());
    record[12..28].copy_from_slice(&event.event_id().bytes());
    let (user_id, user_quota, user_used, requests) = event.user().map_or((0, 0, 0, 0), |delta| {
        (
            delta.user_id().get(),
            delta.quota_delta().units(),
            delta.used_quota_delta().units(),
            delta.request_count_delta(),
        )
    });
    let (token_id, token_remain, token_used) = event.token().map_or((0, 0, 0), |delta| {
        (
            delta.token_id().get(),
            delta.remain_quota_delta().units(),
            delta.used_quota_delta().units(),
        )
    });
    let (channel_id, channel_used) = event.channel().map_or((0, 0), |delta| {
        (delta.channel_id().get(), delta.used_quota_delta().units())
    });
    for (offset, value) in [
        (28, user_id),
        (36, user_quota),
        (44, user_used),
        (52, requests),
        (60, token_id),
        (68, token_remain),
        (76, token_used),
        (84, channel_id),
        (92, channel_used),
    ] {
        record[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    let checksum = checksum(&record[..RECORD_PAYLOAD_SIZE]);
    record[RECORD_PAYLOAD_SIZE..RECORD_SIZE].copy_from_slice(&checksum.to_le_bytes());
    record
}

pub(super) fn validate_sealed_range(
    decoded: &DecodedFile,
    start: u64,
    end: u64,
) -> Result<(), BillingBatchError> {
    let first = decoded
        .events
        .first()
        .ok_or(BillingBatchError::CorruptWal)?;
    let last = decoded.events.last().ok_or(BillingBatchError::CorruptWal)?;
    if decoded.header.base_sequence.checked_add(1) != Some(start)
        || decoded.header.last_sequence > end
        || first.sequence != start
        || last.sequence != end
    {
        return Err(BillingBatchError::CorruptWal);
    }
    Ok(())
}

fn write_header(
    file: &mut File,
    writer_id: BillingWriterId,
    base_sequence: u64,
    last_sequence: u64,
) -> io::Result<()> {
    let mut header = [0_u8; HEADER_SIZE];
    header[0..8].copy_from_slice(HEADER_MAGIC);
    header[8..10].copy_from_slice(&HEADER_VERSION.to_le_bytes());
    header[16..32].copy_from_slice(&writer_id.bytes());
    header[32..40].copy_from_slice(&base_sequence.to_le_bytes());
    header[40..48].copy_from_slice(&last_sequence.to_le_bytes());
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&header)?;
    file.seek(SeekFrom::End(0))?;
    Ok(())
}

fn decode_header(header: &[u8; HEADER_SIZE]) -> Result<FileHeader, BillingBatchError> {
    if &header[0..8] != HEADER_MAGIC
        || u16::from_le_bytes([header[8], header[9]]) != HEADER_VERSION
        || header[10..16] != [0_u8; 6]
    {
        return Err(BillingBatchError::CorruptWal);
    }
    let mut writer_bytes = [0_u8; 16];
    writer_bytes.copy_from_slice(&header[16..32]);
    let writer_id =
        BillingWriterId::from_bytes(writer_bytes).map_err(|_| BillingBatchError::CorruptWal)?;
    let base_sequence = read_u64(&header[32..40]);
    let last_sequence = read_u64(&header[40..48]);
    if base_sequence > last_sequence || last_sequence > i64::MAX as u64 {
        return Err(BillingBatchError::CorruptWal);
    }
    Ok(FileHeader {
        writer_id,
        base_sequence,
        last_sequence,
    })
}

fn decode_record(record: &[u8; RECORD_SIZE]) -> Result<SequencedEvent, BillingBatchError> {
    if &record[0..4] != RECORD_MAGIC
        || checksum(&record[..RECORD_PAYLOAD_SIZE])
            != u32::from_le_bytes(
                record[RECORD_PAYLOAD_SIZE..RECORD_SIZE]
                    .try_into()
                    .map_err(|_| BillingBatchError::CorruptWal)?,
            )
    {
        return Err(BillingBatchError::CorruptWal);
    }
    let sequence = read_u64(&record[4..12]);
    if sequence == 0 || sequence > i64::MAX as u64 {
        return Err(BillingBatchError::CorruptWal);
    }
    let mut event_id = [0_u8; 16];
    event_id.copy_from_slice(&record[12..28]);
    let event_id =
        BillingReservationId::new(event_id).map_err(|_| BillingBatchError::CorruptWal)?;
    let user_id = read_i64(&record[28..36]);
    let user_quota = read_i64(&record[36..44]);
    let user_used = read_i64(&record[44..52]);
    let requests = read_i64(&record[52..60]);
    let token_id = read_i64(&record[60..68]);
    let token_remain = read_i64(&record[68..76]);
    let token_used = read_i64(&record[76..84]);
    let channel_id = read_i64(&record[84..92]);
    let channel_used = read_i64(&record[92..100]);

    let user = if user_id == 0 {
        if user_quota != 0 || user_used != 0 || requests != 0 {
            return Err(BillingBatchError::CorruptWal);
        }
        None
    } else {
        if requests < 0 {
            return Err(BillingBatchError::CorruptWal);
        }
        Some(
            UserBillingDelta::new(
                UserId::new(user_id).map_err(|_| BillingBatchError::CorruptWal)?,
                QuotaDelta::new(user_quota).map_err(|_| BillingBatchError::CorruptWal)?,
                QuotaDelta::new(user_used).map_err(|_| BillingBatchError::CorruptWal)?,
                u64::try_from(requests).map_err(|_| BillingBatchError::CorruptWal)?,
            )
            .map_err(|_| BillingBatchError::CorruptWal)?,
        )
    };
    let token = if token_id == 0 {
        if token_remain != 0 || token_used != 0 {
            return Err(BillingBatchError::CorruptWal);
        }
        None
    } else {
        Some(
            TokenBillingDelta::new(
                TokenId::new(token_id).map_err(|_| BillingBatchError::CorruptWal)?,
                QuotaDelta::new(token_remain).map_err(|_| BillingBatchError::CorruptWal)?,
                QuotaDelta::new(token_used).map_err(|_| BillingBatchError::CorruptWal)?,
            )
            .map_err(|_| BillingBatchError::CorruptWal)?,
        )
    };
    let channel = if channel_id == 0 {
        if channel_used != 0 {
            return Err(BillingBatchError::CorruptWal);
        }
        None
    } else {
        Some(
            ChannelBillingDelta::new(
                ChannelId::new(channel_id).map_err(|_| BillingBatchError::CorruptWal)?,
                QuotaDelta::new(channel_used).map_err(|_| BillingBatchError::CorruptWal)?,
            )
            .map_err(|_| BillingBatchError::CorruptWal)?,
        )
    };
    let event = BillingBatchEvent::new(event_id, user, token, channel)
        .map_err(|_| BillingBatchError::CorruptWal)?;
    Ok(SequencedEvent { sequence, event })
}

fn read_i64(bytes: &[u8]) -> i64 {
    i64::from_le_bytes(bytes.try_into().expect("固定长度记录切片必须是八字节"))
}

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes.try_into().expect("固定长度记录切片必须是八字节"))
}

fn checksum(bytes: &[u8]) -> u32 {
    // FNV-1a 的 wrapping 是校验算法定义，不参与任何额度计算。
    bytes.iter().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    })
}

fn map_io(_: io::Error) -> BillingBatchError {
    BillingBatchError::WalIo
}

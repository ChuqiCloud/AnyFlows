use std::io::{Cursor, ErrorKind};

use af_domain::AfError;
use af_protocol::{AudioDuration, AudioFile, AudioSpeechOutputFormat};
use symphonia::core::{
    errors::Error as SymphoniaError,
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
    units::TimeBase,
};

const NANOSECONDS_PER_SECOND: u128 = 1_000_000_000;
const MAX_DURATION_PROBE_PACKETS: usize = 2_000_000;
const SPEECH_PCM_SAMPLE_RATE: u64 = 24_000;
const SPEECH_PCM_BYTES_PER_SAMPLE: u64 = 2;

/// 在阻塞线程中探测已通过协议签名校验的音频时长。
pub(crate) async fn probe_audio_duration(file: &AudioFile) -> Result<AudioDuration, AfError> {
    let bytes = file.bytes().clone();
    let extension = file.format().extension();
    tokio::task::spawn_blocking(move || probe_blocking(bytes, extension))
        .await
        .map_err(|_| AfError::Internal)?
        .map_err(|_| AfError::InvalidRequest)
}

/// 探测已通过 Speech 协议签名校验的生成音频时长。
pub(crate) async fn probe_speech_duration(
    bytes: &af_adapter::Bytes,
    format: AudioSpeechOutputFormat,
) -> Result<AudioDuration, AfError> {
    if format == AudioSpeechOutputFormat::Pcm {
        return speech_pcm_duration(bytes.len()).map_err(|_| AfError::Internal);
    }
    let bytes = bytes.clone();
    let extension = format.as_str();
    tokio::task::spawn_blocking(move || probe_blocking(bytes, extension))
        .await
        .map_err(|_| AfError::Internal)?
        .map_err(|_| AfError::Internal)
}

fn speech_pcm_duration(byte_length: usize) -> Result<AudioDuration, AudioDurationProbeError> {
    let byte_length =
        u64::try_from(byte_length).map_err(|_| AudioDurationProbeError::DurationOverflow)?;
    let sample_count = byte_length
        .checked_div(SPEECH_PCM_BYTES_PER_SAMPLE)
        .ok_or(AudioDurationProbeError::DurationOverflow)?;
    let nanoseconds = u128::from(sample_count)
        .checked_mul(NANOSECONDS_PER_SECOND)
        .map(|value| value.div_ceil(u128::from(SPEECH_PCM_SAMPLE_RATE)))
        .and_then(|value| u64::try_from(value).ok())
        .ok_or(AudioDurationProbeError::DurationOverflow)?;
    AudioDuration::from_nanoseconds(nanoseconds)
        .map_err(|_| AudioDurationProbeError::DurationOverflow)
}

fn probe_blocking(
    bytes: af_adapter::Bytes,
    extension: &'static str,
) -> Result<AudioDuration, AudioDurationProbeError> {
    let mut hint = Hint::new();
    hint.with_extension(extension);
    let stream = MediaSourceStream::new(
        Box::new(Cursor::new(bytes)),
        MediaSourceStreamOptions::default(),
    );
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| AudioDurationProbeError::InvalidMedia)?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or(AudioDurationProbeError::MissingAudioTrack)?;
    let track_id = track.id;
    let time_base = track.codec_params.time_base;
    let frame_count = track.codec_params.n_frames;
    let start_timestamp = track.codec_params.start_ts;

    if let (Some(time_base), Some(frame_count)) = (time_base, frame_count) {
        return duration_from_ticks(time_base, frame_count);
    }
    let time_base = time_base.ok_or(AudioDurationProbeError::MissingTimeBase)?;
    let mut packet_count = 0_usize;
    let mut end_timestamp = None::<u64>;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => {
                break;
            }
            Err(_) => return Err(AudioDurationProbeError::InvalidMedia),
        };
        packet_count = packet_count
            .checked_add(1)
            .ok_or(AudioDurationProbeError::ProbeBudgetExceeded)?;
        if packet_count > MAX_DURATION_PROBE_PACKETS {
            return Err(AudioDurationProbeError::ProbeBudgetExceeded);
        }
        if packet.track_id() != track_id {
            continue;
        }
        let packet_end = packet
            .ts()
            .checked_add(packet.dur())
            .ok_or(AudioDurationProbeError::DurationOverflow)?;
        end_timestamp = Some(end_timestamp.map_or(packet_end, |current| current.max(packet_end)));
    }
    let duration_ticks = end_timestamp
        .ok_or(AudioDurationProbeError::MissingDuration)?
        .checked_sub(start_timestamp)
        .ok_or(AudioDurationProbeError::InvalidMedia)?;
    duration_from_ticks(time_base, duration_ticks)
}

fn duration_from_ticks(
    time_base: TimeBase,
    ticks: u64,
) -> Result<AudioDuration, AudioDurationProbeError> {
    if ticks == 0 || time_base.numer == 0 || time_base.denom == 0 {
        return Err(AudioDurationProbeError::MissingDuration);
    }
    let numerator = u128::from(ticks)
        .checked_mul(u128::from(time_base.numer))
        .and_then(|value| value.checked_mul(NANOSECONDS_PER_SECOND))
        .ok_or(AudioDurationProbeError::DurationOverflow)?;
    let nanoseconds = numerator.div_ceil(u128::from(time_base.denom));
    let nanoseconds =
        u64::try_from(nanoseconds).map_err(|_| AudioDurationProbeError::DurationOverflow)?;
    AudioDuration::from_nanoseconds(nanoseconds)
        .map_err(|_| AudioDurationProbeError::DurationOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AudioDurationProbeError {
    InvalidMedia,
    MissingAudioTrack,
    MissingTimeBase,
    MissingDuration,
    ProbeBudgetExceeded,
    DurationOverflow,
}

#[cfg(test)]
mod tests {
    use af_adapter::Bytes;

    use super::*;

    #[test]
    fn one_second_pcm_wav_has_exact_duration() {
        let duration = probe_blocking(one_second_wav(), "wav").unwrap();
        assert_eq!(duration.as_nanoseconds(), 1_000_000_000);
    }

    #[test]
    fn speech_pcm_uses_openai_fixed_sample_contract() {
        assert_eq!(
            speech_pcm_duration(48_000).unwrap().as_nanoseconds(),
            1_000_000_000
        );
        assert_eq!(speech_pcm_duration(2).unwrap().as_nanoseconds(), 41_667);
    }

    fn one_second_wav() -> Bytes {
        const SAMPLE_RATE: u32 = 8_000;
        const SAMPLE_COUNT: u32 = SAMPLE_RATE;
        const DATA_BYTES: u32 = SAMPLE_COUNT * 2;
        let mut bytes = Vec::with_capacity((44 + DATA_BYTES) as usize);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + DATA_BYTES).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
        bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&DATA_BYTES.to_le_bytes());
        bytes.resize((44 + DATA_BYTES) as usize, 0);
        Bytes::from(bytes)
    }
}

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tool_derive::ToolParams;

use crate::tools::base::{
    PropertyInfo, PropertyType, ResolvedScope, SharedBucket, Tool, ToolContext, ToolError,
    ToolParams, ToolPermission, ToolSerializationError,
};
use crate::services::stt::{MAX_TASK_SAMPLES, WAIT_SAMPLES};
use crate::tools::storage::{check_file_scope, normalize};

pub struct TranscribeTool;

#[derive(Deserialize, ToolParams)]
struct TranscribeArgs {
    #[tool(
        description = "Absolute or relative path to the audio file to transcribe: a WAV file of 16-bit PCM, mono, 16 kHz."
    )]
    path: String,
    #[tool(
        description = "The id of an installed speech model, exactly as stt.list_models gives it (e.g. \"whisper-turbo\")."
    )]
    model: String,
    #[tool(
        description = "The language spoken in the file as a code (e.g. \"en\", \"ru\", \"de\"), or \"auto\" to let the model detect it."
    )]
    language: String,
}

#[derive(Serialize)]
struct TranscribeOut {
    text: String,
}

/// What a file too long to wait for answers with: the job that is reading it.
#[derive(Serialize)]
struct TranscribeJobOut {
    job_id: i64,
    /// Where the transcript is being written, one line per part of the recording.
    log_path: String,
    audio_minutes: u32,
    /// Repeated in the result itself, not just the tool description: a model reads what a call actually
    /// returned more reliably than a description it saw earlier.
    note: &'static str,
}

/// The recognizer takes 16 kHz audio only (`parse_wav` refuses anything else)
const SAMPLES_PER_MINUTE: usize = 16_000 * 60;

const JOB_NOTE: &str = "The file is long, so it is being transcribed as a background job and there is no text yet. \
    You'll get a message in this chat when it finishes; the transcript is written to log_path as it goes, one line per \
    part with its time range, and its last line starts with [done. Read it with os.job_output (the end) or \
    storage.read_file.";

#[async_trait]
impl Tool for TranscribeTool {
    fn function_name(&self) -> &str {
        "stt.transcribe"
    }

    fn description(&self) -> &str {
        "Transcribes speech from an audio file or a video's audio track into text: use this instead of installing whisper or any other speech package, nothing needs installing. It uses the speech models installed in Settings (Voice). Before calling it, call stt.list_models and take the id of a model marked installed; always pass both model and language, since a call with only a path is an error. The file must be a WAV of 16-bit PCM, mono, 16 kHz — anything else is an error; from a video or another audio format make one first with ffmpeg -i input -ac 1 -ar 16000 output.wav. A file longer than a minute and a half is transcribed as a background job (up to two hours of audio): the result is then a job id and the path of a log the transcript is written to as it goes, and a message arrives in the chat when it is done. Music with singing is not speech and usually comes out empty or wrong. This is for audio you found or were pointed to by path."
    }

    fn required_properties(&self) -> Vec<PropertyInfo> {
        TranscribeArgs::tool_properties()
    }

    // Reads a real file off disk — the same permission a plain storage.read_file on this
    // path would need, not a separate grant of its own.
    fn shared_buckets(&self) -> &'static [SharedBucket] {
        &[SharedBucket::StorageRead]
    }

    fn is_dangerous(
        &self,
        data: Value,
        scope: ResolvedScope,
    ) -> Result<ToolPermission, ToolSerializationError> {
        let args: TranscribeArgs = serde_json::from_value(data)?;
        Ok(check_file_scope(
            &args.path,
            SharedBucket::StorageRead,
            scope.shared.get(&SharedBucket::StorageRead),
        ))
    }

    async fn call_untyped(&self, data: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: TranscribeArgs = serde_json::from_value(data)?;
        let path = normalize(std::path::Path::new(&args.path));
        // "auto" is the empty language the service takes for automatic detection, as the HTTP route sends it.
        let language = if args.language.eq_ignore_ascii_case("auto") { "" } else { args.language.as_str() };
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| ToolError::FailedUnknown(format!("couldn't read '{}': {e}", path.display())))?;
        let samples = parse_wav(&bytes)?;
        // A long recording is held twice, as bytes and as samples, until this is dropped
        drop(bytes);
        if samples.len() > MAX_TASK_SAMPLES {
            return Err(ToolError::FailedUnknown("the audio file is longer than two hours; cut it into shorter files first".to_string()));
        }

        // The chosen model must exist and be installed: `resolve_model` would otherwise fall back to
        // another installed one and silently ignore the choice, so the tool checks first.
        let installed: Vec<String> = ctx.stt.status().models.into_iter().filter(|m| m.installed).map(|m| m.id).collect();
        if !installed.contains(&args.model) {
            return Err(ToolError::FailedUnknown(format!(
                "'{}' is not an installed speech model; the installed ones are: {}",
                args.model,
                if installed.is_empty() { "none (the owner can install one in Settings, Voice)".to_string() } else { installed.join(", ") }
            )));
        }
        let model = ctx
            .stt
            .resolve_model(Some(&args.model))
            .map_err(|e| ToolError::FailedUnknown(e.message.unwrap_or_default()))?;
        if samples.len() > WAIT_SAMPLES {
            let audio_minutes = (samples.len() / SAMPLES_PER_MINUTE) as u32;
            let stt = ctx.stt.clone();
            let language = language.to_string();
            let job = ctx
                .job_store
                .start_task(ctx.chat_id, &format!("Transcribing {} with {}", path.display(), model.label), move |log| async move {
                    stt.transcribe_to_log(model, &language, samples, &log).await
                })
                .await
                .map_err(|e| ToolError::FailedUnknown(e.to_string()))?;
            return Ok(serde_json::to_value(TranscribeJobOut {
                job_id: job.id,
                log_path: job.log_path.to_string_lossy().to_string(),
                audio_minutes,
                note: JOB_NOTE,
            })?);
        }
        let text = ctx
            .stt
            .transcribe(model, language, samples)
            .await
            .map_err(|e| ToolError::FailedUnknown(e.message.unwrap_or_default()))?;

        Ok(serde_json::to_value(TranscribeOut { text })?)
    }
}

/// Parses a WAV file into the 16-bit mono 16 kHz samples the recognizer takes. Only the plain
/// PCM layout is accepted (format tag 1, one channel, 16 bits, 16 kHz) — a resample isn't done
/// here, so anything else is a clear error rather than a guess.
fn parse_wav(bytes: &[u8]) -> Result<Vec<f32>, ToolError> {
    const TRUNCATED: &str = "the audio file is truncated";

    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(ToolError::FailedUnknown("the file is not a WAV file".to_string()));
    }

    let mut off = 12usize;
    let mut fmt_ok = false;
    let mut data: Option<&[u8]> = None;
    while off + 8 <= bytes.len() {
        let id = &bytes[off..off + 4];
        let size = u32::from_le_bytes([bytes[off + 4], bytes[off + 5], bytes[off + 6], bytes[off + 7]]) as usize;
        let body = off + 8;
        if body + size > bytes.len() {
            return Err(ToolError::FailedUnknown(TRUNCATED.to_string()));
        }
        let chunk = &bytes[body..body + size];
        match id {
            b"fmt " => {
                if chunk.len() < 16 {
                    return Err(ToolError::FailedUnknown(
                        "the audio file has a malformed fmt chunk".to_string(),
                    ));
                }
                let tag = u16::from_le_bytes([chunk[0], chunk[1]]);
                let channels = u16::from_le_bytes([chunk[2], chunk[3]]);
                let rate = u32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]);
                let bits = u16::from_le_bytes([chunk[14], chunk[15]]);
                if tag != 1 {
                    return Err(ToolError::FailedUnknown(
                        "the audio file is not plain PCM (only uncompressed WAV is supported)".to_string(),
                    ));
                }
                if channels != 1 || bits != 16 || rate != 16000 {
                    return Err(ToolError::FailedUnknown(format!(
                        "the audio file is {channels} channel, {bits}-bit, {rate} Hz: it must be mono, 16-bit, 16 kHz"
                    )));
                }
                fmt_ok = true;
            }
            b"data" => data = Some(chunk),
            _ => {} // LIST, fact, etc.
        }
        // Chunks are word-aligned: an odd-sized body is padded to the next word.
        off = body + size + (size % 2);
    }

    if !fmt_ok {
        return Err(ToolError::FailedUnknown("the audio file has no fmt chunk (not a valid WAV)".to_string()));
    }
    let data = data.ok_or_else(|| ToolError::FailedUnknown("the audio file has no data chunk".to_string()))?;
    Ok(crate::services::stt::pcm16_to_f32(data))
}

#[cfg(test)]
mod tests {
    use super::parse_wav;

    /// A minimal RIFF/WAVE file: the given 16-byte `fmt` chunk and `samples` of 16-bit little-endian audio.
    fn wav(fmt: &[u8; 16], samples: &[i16]) -> Vec<u8> {
        let mut out = Vec::new();
        let data_size = samples.len() * 2;
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&((36 + data_size) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(fmt);
        out.extend_from_slice(b"data");
        out.extend_from_slice(&(data_size as u32).to_le_bytes());
        for s in samples {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }

    /// `parse_wav`'s error type isn't `Debug`, so a plain `.unwrap()` won't compile — panic by hand instead.
    fn must_parse(bytes: &[u8]) -> Vec<f32> {
        match parse_wav(bytes) {
            Ok(v) => v,
            Err(_) => panic!("parse_wav failed where it should have succeeded"),
        }
    }

    fn pcm_mono_16k() -> [u8; 16] {
        let mut f = [0u8; 16];
        f[0..2].copy_from_slice(&1u16.to_le_bytes()); // tag = PCM
        f[2..4].copy_from_slice(&1u16.to_le_bytes()); // channels = mono
        f[4..8].copy_from_slice(&16000u32.to_le_bytes()); // sample rate
        f[8..12].copy_from_slice(&32000u32.to_le_bytes()); // byte rate
        f[12..14].copy_from_slice(&2u16.to_le_bytes()); // block align
        f[14..16].copy_from_slice(&16u16.to_le_bytes()); // bits per sample
        f
    }

    #[test]
    fn parses_valid_mono_16k() {
        let samples = [0i16, 32767, -32768, 100];
        let floats = must_parse(&wav(&pcm_mono_16k(), &samples));
        assert_eq!(floats.len(), 4);
        assert_eq!(floats[0], 0.0);
        assert!((floats[1] - 0.99997).abs() < 1e-4);
        assert_eq!(floats[2], -1.0);
        assert!((floats[3] - 100.0 / 32768.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_non_wav_and_bad_formats() {
        assert!(parse_wav(b"not a wav at all").is_err());
        // 44.1 kHz instead of 16 kHz
        let mut fmt = pcm_mono_16k();
        fmt[4..8].copy_from_slice(&44100u32.to_le_bytes());
        assert!(parse_wav(&wav(&fmt, &[0i16, 1])).is_err());
        // 32-bit instead of 16-bit
        let mut fmt = pcm_mono_16k();
        fmt[14..16].copy_from_slice(&32u16.to_le_bytes());
        assert!(parse_wav(&wav(&fmt, &[0i16, 1])).is_err());
    }
}

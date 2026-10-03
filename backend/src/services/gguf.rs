//! Reads just enough of a GGUF file's metadata header to tell what kind of file it is and
//! whether a vision projector belongs to a model. GGUF is `llama.cpp`'s model container: a
//! magic, a version, counts, then a run of typed key/value pairs — the tensors themselves
//! come after and are never touched here, so reading a 20 GB model costs a few megabytes.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::Path;

/// Ceilings on what a header may claim, so a corrupt or hostile file can't make this loop for
/// ages or allocate absurdly. Real models sit orders of magnitude below all of them.
const MAX_KV_PAIRS: u64 = 100_000;
const MAX_KEY_LEN: u64 = 1 << 16;
const MAX_WANTED_STRING_LEN: u64 = 1 << 16;
/// A chat template is a program, a few kilobytes for every model seen so far.
const MAX_TEMPLATE_LEN: u64 = 1 << 18;

/// What a GGUF file is, as far as importing it cares.
#[derive(Clone, Debug, PartialEq)]
pub enum GgufKind {
    /// A language model (any architecture that isn't `clip`).
    Model,
    /// A vision projector (`mmproj`): architecture `clip`.
    Projector,
}

#[derive(Clone, Debug)]
pub struct GgufInfo {
    pub kind: GgufKind,
    /// `general.name` — the human name the file's author gave the model. A projector made for
    /// a model usually carries the same one.
    pub name: Option<String>,
    /// A model's hidden size (`<arch>.embedding_length`): what a projector has to project *into*.
    pub embedding_length: Option<u64>,
    /// A projector's output size (`clip.vision.projection_dim`): what it projects into.
    pub projection_dim: Option<u64>,
    /// The context length the model was trained for (`<arch>.context_length`).
    pub trained_context: Option<u64>,
    /// How many layers the model has (`<arch>.block_count`).
    pub block_count: Option<u64>,
    /// How many MTP draft layers the file carries (`<arch>.nextn_predict_layers`): the draft head is
    /// stored with the model, and a file without it can't draft tokens with MTP.
    pub mtp_layers: Option<u64>,
    /// llama.cpp's quantization id (`general.file_type`), see `quantization_name`.
    pub file_type: Option<u64>,
    /// The model's own chat template (`tokenizer.chat_template`), the source of what `think` and tool
    /// calls look like for it.
    pub chat_template: Option<String>,
}

impl GgufInfo {
    /// Whether the file carries an MTP draft head.
    pub fn has_mtp(&self) -> bool {
        self.mtp_layers.is_some_and(|layers| layers > 0)
    }

    /// The quantization's usual name (`IQ3_S`, `Q4_K_M`), when the id is a known one.
    pub fn quantization(&self) -> Option<&'static str> {
        quantization_name(self.file_type?)
    }

    /// Whether `projector` can feed this model: its output size has to equal the model's hidden
    /// size. `None` when either file doesn't say, in which case nothing can be concluded.
    pub fn accepts(&self, projector: &GgufInfo) -> Option<bool> {
        Some(self.embedding_length? == projector.projection_dim?)
    }
}

// GGUF value type ids.
const T_U8: u32 = 0;
const T_I8: u32 = 1;
const T_U16: u32 = 2;
const T_I16: u32 = 3;
const T_U32: u32 = 4;
const T_I32: u32 = 5;
const T_F32: u32 = 6;
const T_BOOL: u32 = 7;
const T_STRING: u32 = 8;
const T_ARRAY: u32 = 9;
const T_U64: u32 = 10;
const T_I64: u32 = 11;
const T_F64: u32 = 12;

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

/// A string's bytes, or `None` after skipping past them when it's longer than `keep`.
fn read_string(r: &mut BufReader<File>, keep: u64) -> io::Result<Option<String>> {
    let len = read_u64(r)?;
    if len > keep {
        r.seek_relative(i64::try_from(len).map_err(|_| invalid("implausible string length"))?)?;
        return Ok(None);
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf)?;
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

/// Size in bytes of a fixed-width value type, or `None` for strings and arrays.
fn fixed_size(ty: u32) -> Option<i64> {
    match ty {
        T_U8 | T_I8 | T_BOOL => Some(1),
        T_U16 | T_I16 => Some(2),
        T_U32 | T_I32 | T_F32 => Some(4),
        T_U64 | T_I64 | T_F64 => Some(8),
        _ => None,
    }
}

fn skip_value(r: &mut BufReader<File>, ty: u32) -> io::Result<()> {
    if let Some(size) = fixed_size(ty) {
        return r.seek_relative(size);
    }
    match ty {
        T_STRING => {
            read_string(r, 0)?;
            Ok(())
        }
        T_ARRAY => {
            let element = read_u32(r)?;
            let count = read_u64(r)?;
            match fixed_size(element) {
                // A tokenizer's score/type arrays: one seek for the whole run.
                Some(size) => r.seek_relative(size.checked_mul(count as i64).ok_or_else(|| invalid("array too large"))?),
                None => {
                    for _ in 0..count {
                        skip_value(r, element)?;
                    }
                    Ok(())
                }
            }
        }
        _ => Err(invalid("unknown value type")),
    }
}

/// A numeric value as `u64`, for the few integer keys this cares about.
fn read_uint(r: &mut BufReader<File>, ty: u32) -> io::Result<Option<u64>> {
    Ok(match ty {
        T_U8 => Some(read_u8(r)? as u64),
        T_U16 => Some(read_u16(r)? as u64),
        T_U32 => Some(read_u32(r)? as u64),
        T_U64 => Some(read_u64(r)?),
        T_I32 => u64::try_from(read_u32(r)? as i32).ok(),
        T_I64 => u64::try_from(read_u64(r)? as i64).ok(),
        _ => {
            skip_value(r, ty)?;
            None
        }
    })
}

fn read_u8(r: &mut impl Read) -> io::Result<u8> {
    let mut b = [0u8; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_u16(r: &mut impl Read) -> io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}

/// Reads the metadata of the GGUF file at `path`. Errs `InvalidData` if it isn't one (wrong
/// magic, truncated header, nonsense counts) — including an empty file.
pub fn read_gguf_info(path: &Path) -> io::Result<GgufInfo> {
    // A big buffer lets `seek_relative` skip over most of the tokenizer arrays without
    // touching the disk again.
    let mut r = BufReader::with_capacity(8 << 20, File::open(path)?);

    let mut magic = [0u8; 4];
    r.read_exact(&mut magic).map_err(|_| invalid("not a GGUF file"))?;
    if &magic != b"GGUF" {
        return Err(invalid("not a GGUF file"));
    }
    let version = read_u32(&mut r)?;
    if !(2..=3).contains(&version) {
        return Err(invalid("unsupported GGUF version"));
    }
    let _tensor_count = read_u64(&mut r)?;
    let kv_count = read_u64(&mut r)?;
    if kv_count > MAX_KV_PAIRS {
        return Err(invalid("implausible metadata size"));
    }

    let mut architecture = None;
    let mut general_type = None;
    let mut name = None;
    let mut projection_dim = None;
    let mut embedding_lengths: HashMap<String, u64> = HashMap::new();
    let mut trained_contexts: HashMap<String, u64> = HashMap::new();
    let mut block_counts: HashMap<String, u64> = HashMap::new();
    let mut mtp_layers: HashMap<String, u64> = HashMap::new();
    let mut file_type = None;
    let mut chat_template = None;

    for _ in 0..kv_count {
        let key = read_string(&mut r, MAX_KEY_LEN)?.ok_or_else(|| invalid("implausible key length"))?;
        let ty = read_u32(&mut r)?;

        match key.as_str() {
            "general.architecture" if ty == T_STRING => architecture = read_string(&mut r, MAX_WANTED_STRING_LEN)?,
            "general.type" if ty == T_STRING => general_type = read_string(&mut r, MAX_WANTED_STRING_LEN)?,
            "general.name" if ty == T_STRING => name = read_string(&mut r, MAX_WANTED_STRING_LEN)?,
            "clip.vision.projection_dim" => projection_dim = read_uint(&mut r, ty)?,
            "general.file_type" => file_type = read_uint(&mut r, ty)?,
            "tokenizer.chat_template" if ty == T_STRING => chat_template = read_string(&mut r, MAX_TEMPLATE_LEN)?,
            k if k.ends_with(".embedding_length") => {
                if let Some(v) = read_uint(&mut r, ty)? {
                    embedding_lengths.insert(k.trim_end_matches(".embedding_length").to_string(), v);
                }
            }
            k if k.ends_with(".context_length") => {
                if let Some(v) = read_uint(&mut r, ty)? {
                    trained_contexts.insert(k.trim_end_matches(".context_length").to_string(), v);
                }
            }
            k if k.ends_with(".block_count") => {
                if let Some(v) = read_uint(&mut r, ty)? {
                    block_counts.insert(k.trim_end_matches(".block_count").to_string(), v);
                }
            }
            k if k.ends_with(".nextn_predict_layers") => {
                if let Some(v) = read_uint(&mut r, ty)? {
                    mtp_layers.insert(k.trim_end_matches(".nextn_predict_layers").to_string(), v);
                }
            }
            _ => skip_value(&mut r, ty)?,
        }
    }

    let architecture = architecture.ok_or_else(|| invalid("no general.architecture"))?;
    let kind = if architecture == "clip" || matches!(general_type.as_deref(), Some("clip-vision") | Some("mmproj")) {
        GgufKind::Projector
    } else {
        GgufKind::Model
    };
    let embedding_length = embedding_lengths.get(&architecture).copied();

    let (trained_context, block_count, mtp_layers) = (
        trained_contexts.get(&architecture).copied(),
        block_counts.get(&architecture).copied(),
        mtp_layers.get(&architecture).copied(),
    );

    Ok(GgufInfo { kind, name, embedding_length, projection_dim, trained_context, block_count, mtp_layers, file_type, chat_template })
}

/// llama.cpp's `llama_ftype` ids, by their usual quantization names. Only the ones in common use.
pub fn quantization_name(file_type: u64) -> Option<&'static str> {
    Some(match file_type {
        0 => "F32",
        1 => "F16",
        2 => "Q4_0",
        3 => "Q4_1",
        7 => "Q8_0",
        8 => "Q5_0",
        9 => "Q5_1",
        10 => "Q2_K",
        11 => "Q3_K_S",
        12 => "Q3_K_M",
        13 => "Q3_K_L",
        14 => "Q4_K_S",
        15 => "Q4_K_M",
        16 => "Q5_K_S",
        17 => "Q5_K_M",
        18 => "Q6_K",
        19 => "IQ2_XXS",
        20 => "IQ2_XS",
        21 => "Q2_K_S",
        22 => "IQ3_XS",
        23 => "IQ3_XXS",
        24 => "IQ1_S",
        25 => "IQ4_NL",
        26 => "IQ3_S",
        27 => "IQ3_M",
        28 => "IQ2_S",
        29 => "IQ2_M",
        30 => "IQ4_XS",
        31 => "IQ1_M",
        32 => "BF16",
        _ => return None,
    })
}

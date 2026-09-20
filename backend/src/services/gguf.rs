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
}

impl GgufInfo {
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

    for _ in 0..kv_count {
        let key = read_string(&mut r, MAX_KEY_LEN)?.ok_or_else(|| invalid("implausible key length"))?;
        let ty = read_u32(&mut r)?;

        match key.as_str() {
            "general.architecture" if ty == T_STRING => architecture = read_string(&mut r, MAX_WANTED_STRING_LEN)?,
            "general.type" if ty == T_STRING => general_type = read_string(&mut r, MAX_WANTED_STRING_LEN)?,
            "general.name" if ty == T_STRING => name = read_string(&mut r, MAX_WANTED_STRING_LEN)?,
            "clip.vision.projection_dim" => projection_dim = read_uint(&mut r, ty)?,
            k if k.ends_with(".embedding_length") => {
                if let Some(v) = read_uint(&mut r, ty)? {
                    embedding_lengths.insert(k.trim_end_matches(".embedding_length").to_string(), v);
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

    Ok(GgufInfo { kind, name, embedding_length, projection_dim })
}

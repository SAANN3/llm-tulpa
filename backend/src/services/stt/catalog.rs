//! The speech-to-text models offered for download: what they are, where their archive is and the sha256 it must have.
//! The archives are the ones sherpa-onnx publishes (`asr-models` release), unpacked into one folder per model.

/// How a model is run: which sherpa-onnx recognizer config it takes
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    /// NVIDIA NeMo transducer (Parakeet): encoder, decoder and joiner
    NemoTransducer,
    /// One CTC network that knows many languages (Omnilingual)
    OmnilingualCtc,
    /// OpenAI Whisper: encoder and decoder, told which language to expect
    Whisper,
}

pub struct ModelSpec {
    /// Names the model in the API, in the user's settings and as its folder
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// The languages it understands, as a person reads them
    pub languages: &'static str,
    pub family: Family,
    /// The archive's file name in the `asr-models` release
    pub archive: &'static str,
    pub download_bytes: u64,
    pub sha256: &'static str,
    /// What the loaded model takes of memory, roughly: said to the user before they install it
    pub memory_bytes: u64,
}

const GIB: u64 = 1 << 30;
const MIB: u64 = 1 << 20;

/// In the order they are offered; the first installed one is the default of a user who chose none
pub(super) const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "parakeet-tdt-0.6b-v3",
        label: "Parakeet TDT 0.6B v3",
        description: "The most accurate of the three for the languages it knows, and fast on a CPU. Puts in punctuation and capitals.",
        languages: "25 European languages: English, Russian, Ukrainian, German, French, Spanish, Italian, Portuguese, Polish, Dutch, Czech and more",
        family: Family::NemoTransducer,
        archive: "sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8.tar.bz2",
        download_bytes: 487_170_055,
        sha256: "5793d0fd397c5778d2cf2126994d58e9d56b1be7c04d13c7a15bb1b4eafb16bf",
        memory_bytes: GIB + 100 * MIB,
    },
    ModelSpec {
        id: "omnilingual-300m",
        label: "Omnilingual 300M",
        description: "For a language the others don't have. Smaller and less accurate, with no punctuation.",
        languages: "About 1600 languages",
        family: Family::OmnilingualCtc,
        archive: "sherpa-onnx-omnilingual-asr-1600-languages-300M-ctc-v2-int8-2026-02-05.tar.bz2",
        download_bytes: 292_313_120,
        sha256: "951b32409aade32bd525310bb39e9666773ba3fc611a39e817f620936d76c631",
        memory_bytes: 700 * MIB,
    },
    ModelSpec {
        id: "whisper-turbo",
        label: "Whisper large-v3 turbo",
        description: "Understands the most languages with good accuracy, but is the slowest: the words appear a little behind your speech. Tell it the language in Settings for the best result.",
        languages: "About 99 languages, including Chinese, Japanese, Korean, Arabic and Hindi",
        family: Family::Whisper,
        archive: "sherpa-onnx-whisper-turbo.tar.bz2",
        download_bytes: 563_790_207,
        sha256: "b11acbbcd660b44a8e0df33724feb5aaa709cf65668f2823d59f656312544f22",
        memory_bytes: 2 * GIB,
    },
];

impl ModelSpec {
    pub fn id(&self) -> &'static str {
        self.id
    }
}

pub(super) fn find(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_consistent() {
        let mut ids: Vec<_> = MODELS.iter().map(|m| m.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), MODELS.len(), "ids are unique");
        for m in MODELS {
            assert!(m.archive.ends_with(".tar.bz2"), "{}", m.id);
            assert_eq!(m.sha256.len(), 64, "{} has a sha256", m.id);
            assert!(m.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'), "{} is a safe folder name", m.id);
        }
    }
}

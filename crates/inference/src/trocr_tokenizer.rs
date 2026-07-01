use latexsnipper_foundation::{Result, SnipperError};
/// TrOCR tokenizer for formula recognition.
/// Loads HuggingFace tokenizer.json and decodes token IDs to text.
use std::collections::HashMap;
use std::path::Path;

const DECODER_START_ID: i64 = 2;
const EOS_ID: i64 = 2;
const MAX_TOKENS: usize = 512;

/// TrOCR tokenizer loaded from tokenizer.json.
pub struct TrOCRTokenizer {
    /// Map from token ID to token string.
    vocab: HashMap<i64, String>,
}

impl TrOCRTokenizer {
    /// Load tokenizer from tokenizer.json file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SnipperError::Model(format!("Failed to read tokenizer: {}", e)))?;

        let root: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| SnipperError::Model(format!("Failed to parse tokenizer JSON: {}", e)))?;

        let model = root
            .get("model")
            .ok_or_else(|| SnipperError::Model("No 'model' key in tokenizer".into()))?;

        let vocab_obj = model
            .get("vocab")
            .ok_or_else(|| SnipperError::Model("No 'vocab' key in model".into()))?;

        let mut vocab = HashMap::new();
        if let Some(obj) = vocab_obj.as_object() {
            for (key, value) in obj {
                if let Some(id) = value.as_i64() {
                    vocab.insert(id, key.clone());
                }
            }
        }

        log::info!("Loaded tokenizer with {} tokens", vocab.len());

        Ok(Self { vocab })
    }

    /// Decode a token ID to a string.
    pub fn decode_token(&self, id: i64) -> Option<&str> {
        self.vocab.get(&id).map(|s| s.as_str())
    }

    /// Decode a sequence of token IDs to text.
    pub fn decode(&self, token_ids: &[i64]) -> String {
        let mut result = String::new();
        for &id in token_ids {
            if id == EOS_ID {
                break;
            }
            if let Some(token) = self.decode_token(id) {
                // Skip special tokens
                if token == "<pad>"
                    || token == "<s>"
                    || token == "</s>"
                    || token == "<unk>"
                    || token == "<mask>"
                {
                    continue;
                }
                // BPE space prefix: Ā (U+0100) or Ġ (U+0120)
                if token.starts_with('\u{0100}') || token.starts_with('\u{0120}') {
                    result.push(' ');
                    result.extend(token.chars().skip(1));
                } else {
                    result.push_str(token);
                }
            }
        }
        result
    }

    /// Get the decoder start token ID.
    pub fn decoder_start_id() -> i64 {
        DECODER_START_ID
    }

    /// Get the EOS token ID.
    pub fn eos_id() -> i64 {
        EOS_ID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_load() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("../models/formula-rec/trocr-deit/tokenizer.json");

        if path.exists() {
            let tok = TrOCRTokenizer::load(&path).unwrap();
            assert!(tok.vocab.len() > 0);
            assert_eq!(tok.decode_token(0), Some("<pad>"));
            assert_eq!(tok.decode_token(5), Some("!"));
        }
    }

    #[test]
    fn test_bpe_space_prefix() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("../models/formula-rec/trocr-deit/tokenizer.json");

        if path.exists() {
            let tok = TrOCRTokenizer::load(&path).unwrap();
            // Check that Ġ prefix tokens exist
            // Token ID 270 should be "Ġ=" (space + equals)
            if let Some(token) = tok.decode_token(270) {
                assert!(
                    token.starts_with('\u{0120}'),
                    "Token 270 should start with Ġ"
                );
            }
        }
    }
}

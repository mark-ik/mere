// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Canonical wire codec for the lexical embedding resource.
//!
//! Hand-specified little-endian layout rather than a serde format, for two
//! reasons: the bytes are content-addressed, so their identity must not move
//! when a serialization library changes its mind; and the exactness receipt
//! wants a layout that can be reproduced from the spec text alone.
//!
//! ```text
//! batch   := b"mesh.lexical.batch/v1\0" dims:u32 count:u32 (len:u32 utf8)*
//! vectors := b"mesh.lexical.vectors/v1\0" dims:u32 count:u32 metric:u8 f32*
//! ```
//!
//! Everything here depends only on `esp` and `std`, so the wasm determinism
//! probe compiles this exact source for another target.

use esp::embed::{EmbeddingProvider, LexicalEmbeddingProvider, SimilarityMetric};

pub const BATCH_MAGIC: &[u8; 22] = b"mesh.lexical.batch/v1\0";
pub const VECTORS_MAGIC: &[u8; 24] = b"mesh.lexical.vectors/v1\0";

/// Widest embedding this resource will produce.
pub const MAX_DIMENSIONS: u32 = 4096;
/// Most texts one batch may carry.
pub const MAX_TEXTS: u32 = 1024;
/// Longest single text, in bytes.
pub const MAX_TEXT_BYTES: u32 = 64 * 1024;

/// A canonical batch of texts to embed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LexicalBatch {
    pub dimensions: u32,
    pub texts: Vec<String>,
}

/// The canonical result: one vector per input, in input order.
#[derive(Clone, Debug, PartialEq)]
pub struct LexicalVectors {
    pub dimensions: u32,
    pub metric: SimilarityMetric,
    pub vectors: Vec<Vec<f32>>,
}

/// Why canonical bytes were refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    #[error("not a {0} record")]
    WrongMagic(&'static str),
    #[error("record ends mid-field")]
    Truncated,
    #[error("record carries {0} trailing bytes")]
    Trailing(usize),
    #[error("dimensions must be 1..={MAX_DIMENSIONS}, found {0}")]
    Dimensions(u32),
    #[error("batch carries {0} texts (max {MAX_TEXTS})")]
    TooManyTexts(u32),
    #[error("text {index} is {bytes} bytes (max {MAX_TEXT_BYTES})")]
    TextTooLong { index: u32, bytes: u32 },
    #[error("text {0} is not valid UTF-8")]
    NotUtf8(u32),
    #[error("unknown similarity metric code {0}")]
    Metric(u8),
    #[error("embedding provider: {0}")]
    Provider(String),
}

impl LexicalBatch {
    pub fn new(dimensions: u32, texts: Vec<String>) -> Self {
        Self { dimensions, texts }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(BATCH_MAGIC.len() + 8 + self.texts.len() * 8);
        out.extend_from_slice(BATCH_MAGIC);
        out.extend_from_slice(&self.dimensions.to_le_bytes());
        out.extend_from_slice(&(self.texts.len() as u32).to_le_bytes());
        for text in &self.texts {
            out.extend_from_slice(&(text.len() as u32).to_le_bytes());
            out.extend_from_slice(text.as_bytes());
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut cursor = Cursor::new(bytes);
        cursor.magic(BATCH_MAGIC, "lexical batch")?;
        let dimensions = cursor.u32()?;
        if dimensions == 0 || dimensions > MAX_DIMENSIONS {
            return Err(CodecError::Dimensions(dimensions));
        }
        let count = cursor.u32()?;
        if count > MAX_TEXTS {
            return Err(CodecError::TooManyTexts(count));
        }
        let mut texts = Vec::with_capacity(count as usize);
        for index in 0..count {
            let len = cursor.u32()?;
            if len > MAX_TEXT_BYTES {
                return Err(CodecError::TextTooLong { index, bytes: len });
            }
            let raw = cursor.take(len as usize)?;
            texts.push(
                std::str::from_utf8(raw)
                    .map_err(|_| CodecError::NotUtf8(index))?
                    .to_string(),
            );
        }
        cursor.finish()?;
        Ok(Self { dimensions, texts })
    }
}

impl LexicalVectors {
    pub fn encode(&self) -> Vec<u8> {
        let count = self.vectors.len();
        let mut out =
            Vec::with_capacity(VECTORS_MAGIC.len() + 9 + count * self.dimensions as usize * 4);
        out.extend_from_slice(VECTORS_MAGIC);
        out.extend_from_slice(&self.dimensions.to_le_bytes());
        out.extend_from_slice(&(count as u32).to_le_bytes());
        out.push(metric_code(self.metric));
        for vector in &self.vectors {
            for value in vector {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut cursor = Cursor::new(bytes);
        cursor.magic(VECTORS_MAGIC, "lexical vectors")?;
        let dimensions = cursor.u32()?;
        if dimensions == 0 || dimensions > MAX_DIMENSIONS {
            return Err(CodecError::Dimensions(dimensions));
        }
        let count = cursor.u32()?;
        if count > MAX_TEXTS {
            return Err(CodecError::TooManyTexts(count));
        }
        let metric = metric_from_code(cursor.u8()?)?;
        let mut vectors = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let mut vector = Vec::with_capacity(dimensions as usize);
            for _ in 0..dimensions {
                let raw: [u8; 4] = cursor.take(4)?.try_into().expect("take(4) yields 4 bytes");
                vector.push(f32::from_le_bytes(raw));
            }
            vectors.push(vector);
        }
        cursor.finish()?;
        Ok(Self {
            dimensions,
            metric,
            vectors,
        })
    }
}

/// Embed a canonical batch. The whole compute half of `esp.embed.lexical/v1`,
/// isolated from the mesh so another target can run exactly this.
pub fn embed_batch(batch: &LexicalBatch) -> Result<LexicalVectors, CodecError> {
    let provider = LexicalEmbeddingProvider::new(batch.dimensions as usize)
        .map_err(|err| CodecError::Provider(err.to_string()))?;
    let texts: Vec<&str> = batch.texts.iter().map(String::as_str).collect();
    let vectors = provider
        .embed(&texts)
        .map_err(|err| CodecError::Provider(err.to_string()))?;
    Ok(LexicalVectors {
        dimensions: batch.dimensions,
        metric: provider.metric(),
        vectors,
    })
}

/// Canonical output bytes for a canonical input — the single function the
/// determinism receipt compares across targets.
pub fn run_canonical(input: &[u8]) -> Result<Vec<u8>, CodecError> {
    Ok(embed_batch(&LexicalBatch::decode(input)?)?.encode())
}

fn metric_code(metric: SimilarityMetric) -> u8 {
    match metric {
        SimilarityMetric::Cosine => 0,
        SimilarityMetric::Euclidean => 1,
        SimilarityMetric::DotProduct => 2,
    }
}

fn metric_from_code(code: u8) -> Result<SimilarityMetric, CodecError> {
    match code {
        0 => Ok(SimilarityMetric::Cosine),
        1 => Ok(SimilarityMetric::Euclidean),
        2 => Ok(SimilarityMetric::DotProduct),
        other => Err(CodecError::Metric(other)),
    }
}

/// A bounds-checked forward reader. Every read either advances or errors, so a
/// truncated record can never be read as a short one.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], CodecError> {
        let end = self.at.checked_add(len).ok_or(CodecError::Truncated)?;
        let slice = self.bytes.get(self.at..end).ok_or(CodecError::Truncated)?;
        self.at = end;
        Ok(slice)
    }

    fn magic(&mut self, expected: &[u8], what: &'static str) -> Result<(), CodecError> {
        if self.take(expected.len())? != expected {
            return Err(CodecError::WrongMagic(what));
        }
        Ok(())
    }

    fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        let raw: [u8; 4] = self.take(4)?.try_into().expect("take(4) yields 4 bytes");
        Ok(u32::from_le_bytes(raw))
    }

    fn finish(self) -> Result<(), CodecError> {
        let left = self.bytes.len() - self.at;
        if left > 0 {
            return Err(CodecError::Trailing(left));
        }
        Ok(())
    }
}

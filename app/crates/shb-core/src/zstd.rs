//! zstd, the one thing every packed file in this game has in common.
//!
//! `structured-zstd` is pure Rust (no cmake, no C compiler on the Windows toolchain used here); the output frames are
//! the standard format, and the level only changes how hard the encoder tries — the game reads any of them.

use crate::error::{Error, Result};
use std::io::Read;

pub fn decompress(data: &[u8]) -> Result<Vec<u8>> {
    let mut source = data;
    let mut decoder = structured_zstd::decoding::StreamingDecoder::new(&mut source);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| Error::Zstd(format!("{e}")))?;
    Ok(out)
}

/// Compress a whole buffer as a single frame.
pub fn compress(data: &[u8], level: i32) -> Result<Vec<u8>> {
    use structured_zstd::encoding::{CompressionLevel, FrameCompressor};
    let mut compressor: FrameCompressor = FrameCompressor::new(CompressionLevel::from_level(level));
    Ok(compressor.compress_independent_frame(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let data = b"the same bytes, whatever the level".repeat(500);
        for level in [3, 6, 19] {
            let packed = compress(&data, level).unwrap();
            assert_eq!(decompress(&packed).unwrap(), data, "level {level}");
        }
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(matches!(decompress(b"not zstd"), Err(Error::Zstd(_))));
    }
}

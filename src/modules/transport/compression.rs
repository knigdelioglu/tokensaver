use std::fmt;
use std::io::{self, Cursor, Read, Write};

use flate2::read::{GzDecoder, ZlibDecoder};
use flate2::write::{GzEncoder, ZlibEncoder};
use flate2::Compression;

pub(crate) const MAX_DECODED_BODY_BYTES: usize = 256 * 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct EncodingChain(Vec<ContentEncoding>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentEncoding {
    Gzip,
    XGzip,
    Deflate,
    Brotli,
    Zstd,
}

#[derive(Debug)]
pub(crate) enum CompressionError {
    UnsupportedEncoding(String),
    DecodedBodyTooLarge,
    Io(io::Error),
}

impl fmt::Display for CompressionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedEncoding(value) => {
                write!(formatter, "unsupported Content-Encoding: {value}")
            }
            Self::DecodedBodyTooLarge => write!(
                formatter,
                "decoded request body exceeds {MAX_DECODED_BODY_BYTES} bytes"
            ),
            Self::Io(error) => write!(formatter, "request body compression failed: {error}"),
        }
    }
}

impl std::error::Error for CompressionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CompressionError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl EncodingChain {
    pub(crate) fn parse(value: Option<&str>) -> Result<Self, CompressionError> {
        let mut encodings = Vec::new();
        for raw in value.unwrap_or_default().split(',') {
            let encoding = raw.trim().to_ascii_lowercase();
            if encoding.is_empty() || encoding == "identity" {
                continue;
            }
            let parsed = match encoding.as_str() {
                "gzip" => ContentEncoding::Gzip,
                "x-gzip" => ContentEncoding::XGzip,
                "deflate" => ContentEncoding::Deflate,
                "br" => ContentEncoding::Brotli,
                "zstd" => ContentEncoding::Zstd,
                _ => return Err(CompressionError::UnsupportedEncoding(encoding)),
            };
            encodings.push(parsed);
        }
        Ok(Self(encodings))
    }

    pub(crate) fn is_identity(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn decode(&self, encoded: &[u8]) -> Result<Vec<u8>, CompressionError> {
        let mut body = encoded.to_vec();
        for encoding in self.0.iter().rev() {
            body = decode_one(*encoding, &body)?;
        }
        Ok(body)
    }

    pub(crate) fn encode(&self, decoded: &[u8]) -> Result<Vec<u8>, CompressionError> {
        let mut body = decoded.to_vec();
        for encoding in &self.0 {
            body = encode_one(*encoding, &body)?;
        }
        Ok(body)
    }
}

fn decode_one(encoding: ContentEncoding, input: &[u8]) -> Result<Vec<u8>, CompressionError> {
    match encoding {
        ContentEncoding::Gzip | ContentEncoding::XGzip => {
            read_limited(GzDecoder::new(Cursor::new(input)))
        }
        ContentEncoding::Deflate => read_limited(ZlibDecoder::new(Cursor::new(input))),
        ContentEncoding::Brotli => {
            read_limited(brotli::Decompressor::new(Cursor::new(input), 32 * 1024))
        }
        ContentEncoding::Zstd => {
            let decoder = zstd::stream::read::Decoder::new(Cursor::new(input))?;
            read_limited(decoder)
        }
    }
}

fn encode_one(encoding: ContentEncoding, input: &[u8]) -> Result<Vec<u8>, CompressionError> {
    match encoding {
        ContentEncoding::Gzip | ContentEncoding::XGzip => {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(input)?;
            Ok(encoder.finish()?)
        }
        ContentEncoding::Deflate => {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(input)?;
            Ok(encoder.finish()?)
        }
        ContentEncoding::Brotli => {
            let mut output = Vec::new();
            {
                let mut encoder = brotli::CompressorWriter::new(&mut output, 32 * 1024, 5, 22);
                encoder.write_all(input)?;
            }
            Ok(output)
        }
        ContentEncoding::Zstd => Ok(zstd::stream::encode_all(Cursor::new(input), 3)?),
    }
}

fn read_limited(reader: impl Read) -> Result<Vec<u8>, CompressionError> {
    let limit = u64::try_from(MAX_DECODED_BODY_BYTES)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut limited = reader.take(limit);
    let mut output = Vec::new();
    limited.read_to_end(&mut output)?;
    if output.len() > MAX_DECODED_BODY_BYTES {
        return Err(CompressionError::DecodedBodyTooLarge);
    }
    Ok(output)
}

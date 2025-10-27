use bytes::BytesMut;
use std::io::{self, Write};
use tokio_util::codec::{Decoder, Encoder};

use crate::{encode_into_writer, process_byte, DecoderState, Result, SlipError};

/// SLIP codec implementing [`tokio_util::codec::Decoder`] and [`Encoder`].
#[derive(Default)]
pub struct SlipCodec {
    state: DecoderState,
    buffer: Vec<u8>,
}

impl SlipCodec {
    /// Construct a new SLIP codec.
    pub fn new() -> Self {
        Self::default()
    }

    /// Encode a byte slice without allocating.
    pub fn encode_slice(&mut self, item: &[u8], dst: &mut BytesMut) -> Result<()> {
        let mut writer = BytesMutWriter(dst);
        encode_into_writer(item.iter().copied(), &mut writer)
    }
}

impl Encoder<Vec<u8>> for SlipCodec {
    type Error = SlipError;

    fn encode(&mut self, item: Vec<u8>, dst: &mut BytesMut) -> Result<()> {
        let mut writer = BytesMutWriter(dst);
        encode_into_writer(item, &mut writer)
    }
}

impl Decoder for SlipCodec {
    type Item = Vec<u8>;
    type Error = SlipError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        while !src.is_empty() {
            let byte = src.split_to(1)[0];
            let completed = process_byte(&mut self.state, byte, |value| self.buffer.push(value))?;
            if completed {
                return Ok(Some(std::mem::take(&mut self.buffer)));
            }
        }
        Ok(None)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
        if let Some(frame) = self.decode(src)? {
            return Ok(Some(frame));
        }
        if self.state.last_was_esc {
            return Err(SlipError::IncompleteEscape);
        }
        if !self.buffer.is_empty() {
            return Err(SlipError::UnexpectedEndOfFrame);
        }
        Ok(None)
    }
}

struct BytesMutWriter<'a>(&'a mut BytesMut);

impl<'a> Write for BytesMutWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_vec() {
        let mut codec = SlipCodec::new();
        let mut dst = BytesMut::new();
        codec.encode(b"abc".to_vec(), &mut dst).unwrap();
        assert_eq!(&dst[..], crate::encode_frame(b"abc"));
    }

    #[test]
    fn encode_slice() {
        let mut codec = SlipCodec::new();
        let mut dst = BytesMut::new();
        codec.encode_slice(b"data", &mut dst).unwrap();
        assert_eq!(&dst[..], crate::encode_frame(b"data"));
    }

    #[test]
    fn decode_multiple_frames() {
        let mut codec = SlipCodec::new();
        let frames = [
            crate::encode_frame(b"one"),
            crate::encode_frame(&[crate::END]),
        ]
        .concat();
        let mut src = BytesMut::from(&frames[..]);
        let first = codec.decode(&mut src).unwrap().unwrap();
        assert_eq!(first, b"one");
        let second = codec.decode(&mut src).unwrap().unwrap();
        assert_eq!(second, vec![crate::END]);
        assert!(codec.decode(&mut src).unwrap().is_none());
    }

    #[test]
    fn decode_incomplete_eof_errors() {
        let mut frame = crate::encode_frame(b"broken");
        frame.pop();
        let mut codec = SlipCodec::new();
        let mut src = BytesMut::from(&frame[..]);
        assert!(codec.decode(&mut src).unwrap().is_none());
        let err = codec.decode_eof(&mut src).unwrap_err();
        assert!(matches!(err, SlipError::UnexpectedEndOfFrame));
    }

    #[test]
    fn decode_esc_pending_eof_errors() {
        let mut codec = SlipCodec::new();
        let mut src = BytesMut::from(&[crate::ESC][..]);
        assert!(codec.decode(&mut src).unwrap().is_none());
        let err = codec.decode_eof(&mut src).unwrap_err();
        assert!(matches!(err, SlipError::IncompleteEscape));
    }
}

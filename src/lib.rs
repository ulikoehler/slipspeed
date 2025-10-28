#![doc = include_str!("../README.md")]

use std::error::Error;
use std::fmt;
use std::io::{self, Read, Write};
use memchr::{memchr2, memchr2_iter};

#[cfg(feature = "async-codec")]
pub mod async_codec;
#[cfg(feature = "tokio-codec")]
pub mod tokio_codec;

/// SLIP END byte (0xC0).
pub const END: u8 = 0xC0;
/// SLIP ESC byte (0xDB).
pub const ESC: u8 = 0xDB;
/// SLIP ESC END byte (0xDC).
pub const ESC_END: u8 = 0xDC;
/// SLIP ESC ESC byte (0xDD).
pub const ESC_ESC: u8 = 0xDD;

/// Convenient result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, SlipError>;

/// Captures decoded bytes that were buffered when a stream ended without a
/// terminating [`END`] byte.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FrameRemainder {
    /// Decoded payload bytes collected before the unexpected end of stream.
    pub decoded: Vec<u8>,
    /// `true` if the decoder ended while waiting for the second byte of an escape sequence.
    pub escape_pending: bool,
}

impl FrameRemainder {
    /// Returns the number of decoded bytes that were buffered.
    pub fn len(&self) -> usize {
        self.decoded.len()
    }

    /// Returns `true` when there is no buffered payload and no pending escape sequence.
    pub fn is_empty(&self) -> bool {
        self.decoded.is_empty() && !self.escape_pending
    }
}

/// Error type for SLIP encoding and decoding operations.
#[derive(Debug)]
#[non_exhaustive]
pub enum SlipError {
    /// Wrapper around [`std::io::Error`] originating from the underlying reader or writer.
    Io(io::Error),
    /// Encountered bytes that were not terminated by an [`END`] delimiter.
    UnexpectedEndOfFrame,
    /// Encountered an [`ESC`] byte at the end of a stream without a following escape code.
    IncompleteEscape,
    /// Encountered an invalid escape sequence while decoding.
    InvalidEscape(u8),
    /// No complete SLIP frame was present in the input while one was expected.
    MissingFrame,
    /// More frames than expected were present in the input.
    MultipleFrames(usize),
}

impl fmt::Display for SlipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SlipError::Io(err) => write!(f, "I/O error: {err}"),
            SlipError::UnexpectedEndOfFrame => write!(f, "encountered unexpected end of frame"),
            SlipError::IncompleteEscape => write!(f, "encountered incomplete escape sequence"),
            SlipError::InvalidEscape(code) => {
                write!(f, "encountered invalid escape sequence 0x{code:02X}")
            }
            SlipError::MissingFrame => write!(f, "no complete SLIP frame found in input"),
            SlipError::MultipleFrames(count) => {
                write!(f, "expected a single frame but found {count}")
            }
        }
    }
}

impl Error for SlipError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SlipError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for SlipError {
    fn from(value: io::Error) -> Self {
        SlipError::Io(value)
    }
}

/// Encode arbitrary bytes as a SLIP frame and return the encoded data as a newly allocated [`Vec`].
///
/// The returned frame always ends with the [`END`] delimiter. See `examples/basic.rs`
/// for an end-to-end demonstration.
pub fn encode_frame(data: &[u8]) -> Vec<u8> {
    // Fast path for slices: pre-size and scan using memchr2.
    let mut out = Vec::with_capacity(encoded_len_bytes(data));
    let mut start = 0usize;
    for pos in memchr2_iter(END, ESC, data) {
        if pos > start {
            out.extend_from_slice(&data[start..pos]);
        }
        match data[pos] {
            END => out.extend_from_slice(&[ESC, ESC_END]),
            ESC => out.extend_from_slice(&[ESC, ESC_ESC]),
            _ => unreachable!(),
        }
        start = pos + 1;
    }
    if start < data.len() {
        out.extend_from_slice(&data[start..]);
    }
    out.push(END);
    out
}

/// Encode an arbitrary iterator of bytes as a SLIP frame and return the encoded data.
///
/// This helper is generic over any iterator to make it easy to encode common Rust collections.
pub fn encode_iter<I>(input: I) -> Vec<u8>
where
    I: IntoIterator<Item = u8>,
{
    let mut out = Vec::new();
    encode_into_writer(input, &mut out).expect("writing to Vec<u8> cannot fail");
    out
}

/// Encode bytes as SLIP and write the result directly into the provided writer.
///
/// The writer receives the escaped payload followed by the trailing [`END`] delimiter.
/// Refer to `examples/basic.rs` for a runnable usage sample.
pub fn encode_into_writer<I, W>(input: I, writer: &mut W) -> Result<()>
where
    I: IntoIterator<Item = u8>,
    W: Write,
{
    for byte in input {
        match byte {
            END => writer.write_all(&[ESC, ESC_END])?,
            ESC => writer.write_all(&[ESC, ESC_ESC])?,
            value => writer.write_all(&[value])?,
        }
    }
    writer.write_all(&[END])?;
    Ok(())
}

/// Decode all SLIP frames contained in the provided byte slice.
///
/// The function returns a vector containing one decoded frame per [`END`] delimiter.
/// Frames are returned in the order they appear in the input.
/// A complete example is available in `examples/basic.rs`.
pub fn decode_frames(bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
    let (frames, remainder) = decode_frames_with_remainder(bytes)?;
    if remainder.escape_pending {
        return Err(SlipError::IncompleteEscape);
    }
    if !remainder.decoded.is_empty() {
        return Err(SlipError::UnexpectedEndOfFrame);
    }
    Ok(frames)
}

/// Decode all SLIP frames produced by the given iterator over bytes.
pub fn decode_frames_iter<I>(input: I) -> Result<Vec<Vec<u8>>>
where
    I: IntoIterator<Item = u8>,
{
    let (frames, remainder) = decode_frames_iter_with_remainder(input)?;
    if remainder.escape_pending {
        return Err(SlipError::IncompleteEscape);
    }
    if !remainder.decoded.is_empty() {
        return Err(SlipError::UnexpectedEndOfFrame);
    }
    Ok(frames)
}

/// Decode SLIP frames and also return any buffered remainder when the input ends without a trailing [`END`].
///
/// ```
/// use slipspeed::{decode_frames_with_remainder, encode_frame};
///
/// let mut truncated = encode_frame(b"hi");
/// truncated.pop();
/// let (frames, remainder) = decode_frames_with_remainder(&truncated).unwrap();
/// assert!(frames.is_empty());
/// assert_eq!(remainder.decoded, b"hi");
/// assert!(!remainder.escape_pending);
/// ```
pub fn decode_frames_with_remainder(bytes: &[u8]) -> Result<(Vec<Vec<u8>>, FrameRemainder)> {
    let mut frames: Vec<Vec<u8>> = Vec::new();
    let mut buffer: Vec<u8> = Vec::new();
    let mut i = 0usize;
    let mut escape_pending = false;

    while i < bytes.len() {
        if escape_pending {
            let code = bytes[i];
            match code {
                ESC_END => buffer.push(END),
                ESC_ESC => buffer.push(ESC),
                invalid => return Err(SlipError::InvalidEscape(invalid)),
            }
            escape_pending = false;
            i += 1;
            continue;
        }

        match memchr2(END, ESC, &bytes[i..]) {
            Some(rel) => {
                let pos = i + rel;
                if pos > i {
                    buffer.extend_from_slice(&bytes[i..pos]);
                }
                match bytes[pos] {
                    END => {
                        frames.push(std::mem::take(&mut buffer));
                    }
                    ESC => {
                        escape_pending = true;
                    }
                    _ => unreachable!(),
                }
                i = pos + 1;
            }
            None => {
                // No more specials: copy remainder and finish
                buffer.extend_from_slice(&bytes[i..]);
                i = bytes.len();
            }
        }
    }

    Ok((
        frames,
        FrameRemainder {
            decoded: buffer,
            escape_pending,
        },
    ))
}

/// Iterator variant of [`decode_frames_with_remainder`].
pub fn decode_frames_iter_with_remainder<I>(input: I) -> Result<(Vec<Vec<u8>>, FrameRemainder)>
where
    I: IntoIterator<Item = u8>,
{
    let mut frames = Vec::new();
    let mut buffer = Vec::new();
    let mut state = DecoderState::default();

    for byte in input {
        let completed = process_byte(&mut state, byte, |value| buffer.push(value))?;
        if completed {
            frames.push(std::mem::take(&mut buffer));
        }
    }

    Ok((
        frames,
        FrameRemainder {
            decoded: buffer,
            escape_pending: state.last_was_esc,
        },
    ))
}

/// Compute the encoded length (including the trailing [`END`] delimiter) without allocating.
///
/// ```
/// use slipstream::{encoded_len, END, ESC};
///
/// assert_eq!(encoded_len([END, ESC, 0x01]), 6);
/// ```
pub fn encoded_len<I>(input: I) -> usize
where
    I: IntoIterator<Item = u8>,
{
    let mut len = 1; // Account for the final END delimiter.
    for byte in input {
        len += match byte {
            END | ESC => 2,
            _ => 1,
        };
    }
    len
}

/// Optimized encoded length for byte slices.
fn encoded_len_bytes(bytes: &[u8]) -> usize {
    // Each END/ESC expands to two bytes; others stay as one. Add 1 for trailing END.
    let mut count = 0usize;
    for _ in memchr2_iter(END, ESC, bytes) {
        count += 1;
    }
    bytes.len() + count + 1
}

/// Determine the decoded length of each SLIP frame in the provided input without materialising the payloads.
///
/// ```
/// use slipstream::{decoded_lengths, encode_frame};
///
/// let encoded = [encode_frame(b"hi"), encode_frame(&[])].concat();
/// assert_eq!(decoded_lengths(&encoded).unwrap(), vec![2, 0]);
/// ```
pub fn decoded_lengths(bytes: &[u8]) -> Result<Vec<usize>> {
    let mut lengths: Vec<usize> = Vec::new();
    let mut current = 0usize;
    let mut i = 0usize;
    let mut escape_pending = false;

    while i < bytes.len() {
        if escape_pending {
            let code = bytes[i];
            match code {
                ESC_END | ESC_ESC => {
                    current += 1;
                }
                invalid => return Err(SlipError::InvalidEscape(invalid)),
            }
            escape_pending = false;
            i += 1;
            continue;
        }

        match memchr2(END, ESC, &bytes[i..]) {
            Some(rel) => {
                let pos = i + rel;
                // bytes between i..pos are payload
                current += pos - i;
                match bytes[pos] {
                    END => {
                        lengths.push(current);
                        current = 0;
                    }
                    ESC => {
                        escape_pending = true;
                    }
                    _ => unreachable!(),
                }
                i = pos + 1;
            }
            None => {
                // No more specials: remaining are payload
                current += bytes.len() - i;
                i = bytes.len();
            }
        }
    }

    if escape_pending {
        return Err(SlipError::IncompleteEscape);
    }
    if current != 0 {
        return Err(SlipError::UnexpectedEndOfFrame);
    }
    Ok(lengths)
}

/// Iterator variant of [`decoded_lengths`].
pub fn decoded_lengths_iter<I>(input: I) -> Result<Vec<usize>>
where
    I: IntoIterator<Item = u8>,
{
    let mut lengths = Vec::new();
    let mut current = 0usize;
    let mut state = DecoderState::default();

    for byte in input {
        let completed = process_byte(&mut state, byte, |_| current += 1)?;
        if completed {
            lengths.push(current);
            current = 0;
        }
    }

    if state.last_was_esc {
        return Err(SlipError::IncompleteEscape);
    }

    if current != 0 {
        return Err(SlipError::UnexpectedEndOfFrame);
    }

    Ok(lengths)
}

/// Decode a single SLIP frame from the provided bytes.
///
/// # Errors
///
/// * [`SlipError::MissingFrame`] if no complete frame was found.
/// * [`SlipError::MultipleFrames`] if more than one frame was present.
pub fn decode_frame(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut frames = decode_frames(bytes)?;
    match frames.len() {
        0 => Err(SlipError::MissingFrame),
        1 => Ok(frames.remove(0)),
        count => Err(SlipError::MultipleFrames(count)),
    }
}

/// Writer wrapper that encodes outgoing frames as SLIP before forwarding them to the underlying writer.
///
/// The wrapper does not buffer beyond the escaping that SLIP requires. Each call to [`write_frame`](SlipWriter::write_frame)
/// appends a single SLIP frame to the wrapped writer. See `examples/stream.rs` for a runnable demonstration.
pub struct SlipWriter<W> {
    inner: W,
}

impl<W> SlipWriter<W> {
    /// Construct a new SLIP writer around the provided sink.
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    /// Retrieve an immutable reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Retrieve a mutable reference to the underlying writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// Consume the wrapper and return the inner writer.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

impl<W: Write> SlipWriter<W> {
    /// Encode the provided payload as a SLIP frame and write it to the underlying sink.
    pub fn write_frame(&mut self, payload: &[u8]) -> Result<()> {
        // Use the optimized slice-based encoder and write once to reduce syscall overhead.
        let frame = encode_frame(payload);
        self.inner.write_all(&frame).map_err(SlipError::from)
    }

    /// Encode any iterator of bytes as a SLIP frame and write it to the underlying sink.
    pub fn write_frame_iter<I>(&mut self, payload: I) -> Result<()>
    where
        I: IntoIterator<Item = u8>,
    {
        encode_into_writer(payload, &mut self.inner)
    }

    /// Flush the underlying writer.
    pub fn flush(&mut self) -> Result<()> {
        self.inner.flush().map_err(SlipError::from)
    }
}

/// Reader wrapper that decodes SLIP frames from an underlying byte stream.
///
/// A full streaming example is provided in `examples/stream.rs`. Use
/// [`SlipReader::take_remainder`] to inspect buffered data when a stream ends
/// mid-frame.
pub struct SlipReader<R> {
    inner: R,
    state: DecoderState,
    pending: Vec<u8>,
}

impl<R> SlipReader<R> {
    /// Construct a new `SlipReader` around the provided source.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            state: DecoderState::default(),
            pending: Vec::new(),
        }
    }

    /// Borrow the underlying reader.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Borrow the underlying reader mutably.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Consume the wrapper and return the inner reader.
    pub fn into_inner(self) -> R {
        self.inner
    }

    /// Consume the wrapper and return both the inner reader and any buffered remainder.
    pub fn into_inner_with_remainder(self) -> (R, FrameRemainder) {
        (
            self.inner,
            FrameRemainder {
                decoded: self.pending,
                escape_pending: self.state.last_was_esc,
            },
        )
    }
}

impl<R: Read> SlipReader<R> {
    /// Read the next SLIP frame into the supplied buffer.
    ///
    /// On success the buffer is populated with the decoded payload and the function returns the frame length.
    /// When the end of the underlying reader is reached without another complete frame, `Ok(None)` is returned.
    pub fn read_frame_into(&mut self, buffer: &mut Vec<u8>) -> Result<Option<usize>> {
        buffer.clear();

        loop {
            let mut byte = [0u8; 1];
            match self.inner.read(&mut byte) {
                Ok(0) => {
                    if self.state.last_was_esc {
                        return Err(SlipError::IncompleteEscape);
                    }
                    if !self.pending.is_empty() {
                        return Err(SlipError::UnexpectedEndOfFrame);
                    }
                    return Ok(None);
                }
                Ok(_) => {
                    let completed =
                        process_byte(&mut self.state, byte[0], |value| self.pending.push(value))?;
                    if completed {
                        buffer.extend_from_slice(&self.pending);
                        let len = buffer.len();
                        self.pending.clear();
                        return Ok(Some(len));
                    }
                }
                Err(err) => return Err(SlipError::Io(err)),
            }
        }
    }

    /// Read the next SLIP frame and return it as a freshly allocated [`Vec`].
    pub fn read_frame(&mut self) -> Result<Option<Vec<u8>>> {
        let mut frame = Vec::new();
        match self.read_frame_into(&mut frame)? {
            Some(_) => Ok(Some(frame)),
            None => Ok(None),
        }
    }

    /// Read the next SLIP frame and return only its decoded length.
    ///
    /// ```
    /// use slipstream::{SlipReader, encode_frame, Result};
    /// use std::io::Cursor;
    ///
    /// # fn main() -> Result<()> {
    /// let encoded = [encode_frame(b"foo"), encode_frame(&[1])].concat();
    /// let mut reader = SlipReader::new(Cursor::new(encoded));
    /// assert_eq!(reader.read_frame_length()?, Some(3));
    /// assert_eq!(reader.read_frame_length()?, Some(1));
    /// assert!(reader.read_frame_length()?.is_none());
    /// # Ok(())
    /// # }
    /// ```
    pub fn read_frame_length(&mut self) -> Result<Option<usize>> {
        let mut length = 0usize;

        loop {
            let mut byte = [0u8; 1];
            match self.inner.read(&mut byte) {
                Ok(0) => {
                    if self.state.last_was_esc {
                        return Err(SlipError::IncompleteEscape);
                    }
                    if !self.pending.is_empty() {
                        return Err(SlipError::UnexpectedEndOfFrame);
                    }
                    return Ok(None);
                }
                Ok(_) => {
                    let completed = process_byte(&mut self.state, byte[0], |value| {
                        self.pending.push(value);
                        length += 1;
                    })?;

                    if completed {
                        self.pending.clear();
                        return Ok(Some(length));
                    }
                }
                Err(err) => return Err(SlipError::Io(err)),
            }
        }
    }

    /// Take ownership of any pending decoded bytes accumulated for the current, incomplete frame.
    ///
    /// ```
    /// use slipstream::{SlipReader, encode_frame, SlipError, Result};
    /// use std::io::Cursor;
    ///
    /// # fn main() -> Result<()> {
    /// let mut encoded = encode_frame(b"data");
    /// encoded.pop(); // remove END terminator
    /// let mut reader = SlipReader::new(Cursor::new(encoded));
    /// let mut frame = Vec::new();
    /// assert!(matches!(
    ///     reader.read_frame_into(&mut frame),
    ///     Err(SlipError::UnexpectedEndOfFrame)
    /// ));
    /// let remainder = reader.take_remainder();
    /// assert_eq!(remainder.decoded, b"data");
    /// assert!(!remainder.escape_pending);
    /// # Ok(())
    /// # }
    /// ```
    pub fn take_remainder(&mut self) -> FrameRemainder {
        let remainder = FrameRemainder {
            decoded: std::mem::take(&mut self.pending),
            escape_pending: self.state.last_was_esc,
        };
        self.state.last_was_esc = false;
        remainder
    }

    /// Check if an incomplete frame is currently buffered.
    pub fn has_remainder(&self) -> bool {
        !self.pending.is_empty() || self.state.last_was_esc
    }
}

#[derive(Default)]
pub(crate) struct DecoderState {
    pub(crate) last_was_esc: bool,
}

fn process_byte<F>(state: &mut DecoderState, byte: u8, mut on_byte: F) -> Result<bool>
where
    F: FnMut(u8),
{
    if state.last_was_esc {
        state.last_was_esc = false;
        match byte {
            ESC_END => on_byte(END),
            ESC_ESC => on_byte(ESC),
            invalid => return Err(SlipError::InvalidEscape(invalid)),
        }
        return Ok(false);
    }

    match byte {
        END => {
            state.last_was_esc = false;
            Ok(true)
        }
        ESC => {
            state.last_was_esc = true;
            Ok(false)
        }
        value => {
            on_byte(value);
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn encode_simple() {
        let encoded = encode_frame(b"abc");
        assert_eq!(encoded, vec![b'a', b'b', b'c', END]);
    }

    #[test]
    fn encode_escapes() {
        let encoded = encode_frame(&[END, ESC, 0x01]);
        assert_eq!(encoded, vec![ESC, ESC_END, ESC, ESC_ESC, 0x01, END]);
    }

    #[test]
    fn decode_single_frame() {
        let frame = encode_frame(b"payload");
        let decoded = decode_frame(&frame).unwrap();
        assert_eq!(decoded, b"payload");
    }

    #[test]
    fn decode_multiple_frames() {
        let encoded = [encode_frame(b"one"), encode_frame(&[END])].concat();
        let frames = decode_frames(&encoded).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0], b"one");
        assert_eq!(frames[1], vec![END]);
    }

    #[test]
    fn reader_writer_roundtrip() {
        let mut writer = SlipWriter::new(Vec::new());
        writer.write_frame(b"first").unwrap();
        writer.write_frame(&[END]).unwrap();
        let encoded = writer.into_inner();

        let mut reader = SlipReader::new(Cursor::new(encoded));
        let mut frame = Vec::new();
        assert_eq!(reader.read_frame_into(&mut frame).unwrap(), Some(5));
        assert_eq!(frame, b"first");
        assert_eq!(reader.read_frame_into(&mut frame).unwrap(), Some(1));
        assert_eq!(frame, vec![END]);
        assert!(reader.read_frame_into(&mut frame).unwrap().is_none());
    }

    #[test]
    fn decode_invalid_escape() {
        let err = decode_frames(&[ESC, 0x01, END]).unwrap_err();
        assert!(matches!(err, SlipError::InvalidEscape(0x01)));
    }

    #[test]
    fn reader_incomplete_escape() {
        let data = vec![ESC];
        let mut reader = SlipReader::new(Cursor::new(data));
        let mut frame = Vec::new();
        let err = reader.read_frame_into(&mut frame).unwrap_err();
        assert!(matches!(err, SlipError::IncompleteEscape));
    }

    #[test]
    fn decode_frames_remainder_incomplete() {
        let (frames, remainder) = decode_frames_with_remainder(&[0x01, 0x02]).unwrap();
        assert!(frames.is_empty());
        assert_eq!(remainder.decoded, vec![0x01, 0x02]);
        assert!(!remainder.escape_pending);
    }

    #[test]
    fn decode_frames_remainder_escape_pending() {
        let input = [b'X', ESC];
        let (frames, remainder) = decode_frames_with_remainder(&input).unwrap();
        assert!(frames.is_empty());
        assert_eq!(remainder.decoded, vec![b'X']);
        assert!(remainder.escape_pending);
    }

    #[test]
    fn decoded_lengths_multiple_frames() {
        let encoded = [
            encode_frame(b"foo"),
            encode_frame(&[]),
            encode_frame(&[END]),
        ]
        .concat();
        let lengths = decoded_lengths(&encoded).unwrap();
        assert_eq!(lengths, vec![3, 0, 1]);
    }

    #[test]
    fn decoded_lengths_incomplete_frame_error() {
        let mut encoded = encode_frame(b"broken");
        encoded.pop(); // drop END terminator
        let err = decoded_lengths(&encoded).unwrap_err();
        assert!(matches!(err, SlipError::UnexpectedEndOfFrame));
    }

    #[test]
    fn encoded_len_counts_escapes() {
        let len = encoded_len([END, ESC, 0x01]);
        assert_eq!(len, 6);
    }

    #[test]
    fn reader_take_remainder_after_eof() {
        let mut encoded = encode_frame(b"chunk");
        encoded.pop();
        let mut reader = SlipReader::new(Cursor::new(encoded));
        let mut frame = Vec::new();
        let err = reader.read_frame_into(&mut frame).unwrap_err();
        assert!(matches!(err, SlipError::UnexpectedEndOfFrame));
        assert!(frame.is_empty());
        assert!(reader.has_remainder());
        let remainder = reader.take_remainder();
        assert_eq!(remainder.decoded, b"chunk");
        assert!(!remainder.escape_pending);
        assert!(!reader.has_remainder());
    }

    #[test]
    fn reader_frame_length_only() {
        let encoded = [
            encode_frame(b"first"),
            encode_frame(&[END]),
            encode_frame(b""),
        ]
        .concat();
        let mut reader = SlipReader::new(Cursor::new(encoded));
        assert_eq!(reader.read_frame_length().unwrap(), Some(5));
        assert_eq!(reader.read_frame_length().unwrap(), Some(1));
        assert_eq!(reader.read_frame_length().unwrap(), Some(0));
        assert!(reader.read_frame_length().unwrap().is_none());
    }

    #[test]
    fn reader_frame_length_incomplete() {
        let mut encoded = encode_frame(b"oops");
        encoded.pop();
        let mut reader = SlipReader::new(Cursor::new(encoded));
        let err = reader.read_frame_length().unwrap_err();
        assert!(matches!(err, SlipError::UnexpectedEndOfFrame));
        let remainder = reader.take_remainder();
        assert_eq!(remainder.decoded, b"oops");
        assert!(!remainder.escape_pending);
    }
}

/// HTTP message body parsing
///
/// See https://tools.ietf.org/html/rfc7230#section-3.3.3

use std::io;

use tokio_core::io::{Codec, EasyBuf};

use super::Chunk;

/// Body decoding based on a Content-Length header
pub struct Length {
    length: usize,
    remaining: usize,
}

impl Length {
    pub fn new(length: usize) -> Length {
        Length {
            length: length,
            remaining: length,
        }
    }

    pub fn remaining(&self) -> usize {
        self.remaining
    }
}

impl Codec for Length {
    type In = Chunk;
    type Out = Chunk;

    fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Self::In>> {
        if buf.len() == 0 {
            return Ok(None);
        }

        if self.remaining <= 0 {
            return Err(io::Error::new(io::ErrorKind::Other, "No more body bytes remaining."));
        }

        // TODO make sure that content length is not > usize::MAX. does parse do this?
        let len = ::std::cmp::min(self.length, buf.len());
        self.remaining -= len;
        debug!("Content length remaining {}", self.remaining);

        let body = buf.drain_to(len);

        Ok(
            Some(
                Chunk(Vec::from(body.as_ref()))
            )
        )
    }

    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> io::Result<()> {
        buf.extend_from_slice(msg.0.as_ref());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tokio_core::io::{Codec, EasyBuf};
    use super::*;

    fn mock_buf(len: usize) -> EasyBuf {
        let mut buf = EasyBuf::with_capacity(len);
        if len > 0 {
            extend_mock_buf(&mut buf, len);
        }
        assert_eq!(len, buf.len());
        buf
    }

    fn extend_mock_buf(buf: &mut EasyBuf, len: usize) {
        let mut data = (0u8..len as u8).map(|_| 0).collect::<Vec<u8>>();
        buf.get_mut().append(&mut data);
    }

    #[test]
    fn test_decode_buf_len_zero() {
        let mut buf = EasyBuf::with_capacity(0);
        assert_eq!(0, buf.len());
        let mut codec = Length::new(1);
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn test_decode_buf_len_equals_content_length() {
        let mut buf = mock_buf(64);
        let mut codec = Length::new(64);
        assert_eq!(64, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(0, buf.len());
    }

    #[test]
    fn test_decode_buf_len_greater_than_content_length() {
        let mut buf = mock_buf(65);
        let mut codec = Length::new(64);
        assert_eq!(64, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(1, buf.len());

        assert!(codec.decode(&mut buf).is_err());
    }

    #[test]
    fn test_decode_buf_len_less_than_content_length() {
        let mut buf = mock_buf(20);
        let mut codec = Length::new(64);
        assert_eq!(20, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(44, codec.remaining());
        assert_eq!(0, buf.len());

        extend_mock_buf(&mut buf, 40);
        assert_eq!(40, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(4, codec.remaining());
        assert_eq!(0, buf.len());

        extend_mock_buf(&mut buf, 4);
        assert_eq!(4, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(0, codec.remaining());
        assert_eq!(0, buf.len());
    }
}

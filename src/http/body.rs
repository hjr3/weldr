/// HTTP message body parsing
///
/// See https://tools.ietf.org/html/rfc7230#section-3.3.3

use std::io;

use tokio_core::io::EasyBuf;

use httparse;

use super::Chunk;

pub enum BodyCodec {
    Length(Length),
    Chunked(Chunked),
    UntilClose(UntilClose),
}

impl BodyCodec {
    pub fn remaining(&self) -> bool {
        match *self {
            BodyCodec::Length(ref l) => l.remaining(),
            BodyCodec::Chunked(ref c) => c.remaining(),
            BodyCodec::UntilClose(ref u) => u.remaining(),
        }
    }

    pub fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Chunk>> {
        match *self {
            BodyCodec::Length(ref mut l) => l.decode(buf),
            BodyCodec::Chunked(ref mut c) => c.decode(buf),
            BodyCodec::UntilClose(ref mut u) => u.decode(buf),
        }
    }
}

/// Body decoding based on a Content-Length header
#[derive(Debug)]
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

    pub fn remaining(&self) -> bool {
        self.remaining > 0
    }

    pub fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Chunk>> {
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
}

#[derive(Eq, PartialEq, Debug)]
pub enum ChunkedState {
    Header,
    Body(u64),
    Trailer,
    End,
}

/// Chunked Transfer Coding
///
/// See https://tools.ietf.org/html/rfc7230#section-4
pub struct Chunked {
    state: ChunkedState,
}

impl Chunked {
    pub fn new() -> Chunked {
        Chunked {
            state: ChunkedState::Header,
        }
    }

    pub fn remaining(&self) -> bool {
        self.state != ChunkedState::End
    }

    pub fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Chunk>> {
        trace!("chunked decode");
        let mut body: Option<Chunk> = None;
        loop {
            match self.state {
                ChunkedState::Header => {
                    trace!("chunk state header");
                    match httparse::parse_chunk_size(buf.as_ref()) {
                        Ok(httparse::Status::Complete((i, len))) => {
                            debug!("Found chunk size of {}", len);

                            let header = buf.drain_to(i);

                            if body.is_some() {
                                let mut chunk = body.take().expect("Chunk is None");
                                chunk.0.extend_from_slice(header.as_ref());
                                body = Some(chunk);
                            } else {
                                body = Some(Chunk(Vec::from(header.as_ref())));
                            }

                            if len > 0 {
                                self.state = ChunkedState::Body(len);
                            } else {
                                debug!("Found last chunk");
                                if buf.as_ref().starts_with(b"\r\n") {
                                    self.state = ChunkedState::End;
                                } else {
                                    self.state = ChunkedState::Trailer;
                                }
                            }
                        }
                        Ok(httparse::Status::Partial) => return Ok(body),
                        Err(e) => {
                            debug!("Invalid chunk size: {:?}", e);
                            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid chunk size"));
                        }
                    }
                }
                ChunkedState::Body(len) => {
                    trace!("chunk state body");

                    if buf.len() == 0 {
                        return Ok(body);
                    }

                    // ensure that the buffer contains the crlf too
                    // TODO check for overflow
                    let len: usize = len as usize + 2;

                    if len > buf.len() {
                        debug!("partial body chunk + crlf len = {}, buf len = {}", len, buf.len());
                        let l = buf.len();
                        let b = buf.drain_to(l);

                        if body.is_some() {
                            let mut chunk = body.take().expect("Chunk is None");
                            chunk.0.extend_from_slice(b.as_ref());
                            body = Some(chunk);
                        } else {
                            body = Some(Chunk(Vec::from(b.as_ref())));
                        }

                        let new_len: u64 = (len - l - 2) as u64;
                        self.state = ChunkedState::Body(new_len);
                    } else {
                        let b = buf.drain_to(len);

                        if body.is_some() {
                            let mut chunk = body.take().expect("Chunk is None");
                            chunk.0.extend_from_slice(b.as_ref());
                            body = Some(chunk);
                        } else {
                            body = Some(Chunk(Vec::from(b.as_ref())));
                        }

                        self.state = ChunkedState::Header;
                    }
                }
                ChunkedState::Trailer => {
                    trace!("chunk state trailer");
                    let mut headers = [httparse::EMPTY_HEADER; 16];
                    match httparse::parse_headers(buf.as_ref(), &mut headers) {
                        Ok(httparse::Status::Complete((i, _))) => {
                            let h = buf.drain_to(i);

                            if body.is_some() {
                                let mut chunk = body.take().expect("Chunk is None");
                                chunk.0.extend_from_slice(h.as_ref());
                                body = Some(chunk);
                            } else {
                                body = Some(Chunk(Vec::from(h.as_ref())));
                            }

                            self.state = ChunkedState::End;
                        }
                        Ok(httparse::Status::Partial) => return Ok(body),
                        Err(e) => {
                            debug!("Invalid header format: {:?}", e);
                            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid header format"));
                        }
                    }
                }
                ChunkedState::End => {
                    trace!("chunk state end");
                    if buf.len() < 2 {
                        return Ok(body);
                    }

                    let h = buf.drain_to(2);
                    if body.is_some() {
                        let mut chunk = body.take().expect("Chunk is None");
                        chunk.0.extend_from_slice(h.as_ref());
                        body = Some(chunk);
                    } else {
                        body = Some(Chunk(Vec::from(h.as_ref())));
                    }
                    break;
                }
            }
        }

        Ok(body)
    }
}

pub struct UntilClose {
    is_closed: bool,
}

impl UntilClose {

    pub fn new() -> UntilClose {
        UntilClose {
            is_closed: false,
        }
    }

    pub fn remaining(&self) -> bool {
        self.is_closed == false
    }

    pub fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Chunk>> {
        if buf.len() == 0 {
            self.is_closed = true;
            return Ok(None);
        }

        let len = buf.len();
        trace!("UntilClose decode {} bytes", len);


        let body = buf.drain_to(len);

        Ok(
            Some(
                Chunk(Vec::from(body.as_ref()))
            )
        )
    }
}

#[cfg(test)]
mod tests {
    use tokio_core::io::EasyBuf;
    use super::*;

    fn mock_buf(data: &[u8]) -> EasyBuf {
        let mut buf = EasyBuf::with_capacity(data.len());
        buf.get_mut().extend_from_slice(data);
        buf
    }

    fn extend_mock_buf(buf: &mut EasyBuf, len: usize) {
        let mut data = (0u8..len as u8).map(|_| 0).collect::<Vec<u8>>();
        buf.get_mut().append(&mut data);
    }

    #[test]
    fn test_decode_length_buf_len_zero() {
        let mut buf = mock_buf(&[]);
        assert_eq!(0, buf.len());
        let mut codec = Length::new(1);
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn test_decode_length_buf_len_equals_content_length() {
        let mut buf = mock_buf(&[0u8; 64]);
        let mut codec = Length::new(64);
        assert_eq!(64, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(0, buf.len());
    }

    #[test]
    fn test_decode_length_buf_len_greater_than_content_length() {
        let mut buf = mock_buf(&[0u8; 65]);
        let mut codec = Length::new(64);
        assert_eq!(64, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(1, buf.len());

        assert!(codec.decode(&mut buf).is_err());
    }

    #[test]
    fn test_decode_length_buf_len_less_than_content_length() {
        let mut buf = mock_buf(&[0u8; 20]);
        let mut codec = Length::new(64);
        assert_eq!(20, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(44, codec.remaining);
        assert_eq!(0, buf.len());

        extend_mock_buf(&mut buf, 40);
        assert_eq!(40, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(4, codec.remaining);
        assert_eq!(0, buf.len());

        extend_mock_buf(&mut buf, 4);
        assert_eq!(4, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(0, codec.remaining);
        assert_eq!(0, buf.len());
    }

    #[test]
    fn test_decode_chunked_buf_len_zero() {
        let mut buf = mock_buf(&[]);
        assert_eq!(0, buf.len());
        let mut codec = Chunked::new();
        assert!(codec.decode(&mut buf).unwrap().is_none());
    }

    #[test]
    fn test_decode_chunked() {
        let body =
            b"7\r\n\
              Mozilla\r\n\
              9\r\n\
              Developer\r\n\
              7\r\n\
              Network\r\n\
              0\r\n\
              \r\n";

        let mut buf = mock_buf(body.as_ref());

        let mut codec = Chunked::new();
        assert_eq!(body.len(), codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(0, buf.len());
        assert_eq!(ChunkedState::End, codec.state);
    }

    #[test]
    fn test_decode_chunked_buf_len_greater_than_chunk() {
        let body =
            b"7\r\n\
              Mozilla\r\n\
              0\r\n\
              \r\nx";

        let mut buf = mock_buf(body.as_ref());

        let mut codec = Chunked::new();
        assert_eq!(body.len() - 1, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(1, buf.len());
        assert_eq!(ChunkedState::End, codec.state);
    }

    #[test]
    fn test_decode_chunked_buf_len_less_than_chunk() {
        let body =
            b"7\r\n\
              Mozi";

        let mut buf = mock_buf(body.as_ref());

        let mut codec = Chunked::new();
        assert_eq!(7, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(0, buf.len());
        assert_eq!(ChunkedState::Body(3), codec.state);

        let body = b"lla\r\n";
        buf.get_mut().extend_from_slice(body.as_ref());

        assert_eq!(5, codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(0, buf.len());
        assert_eq!(ChunkedState::Header, codec.state);
    }

    #[test]
    fn test_decode_until_close_buf_len_zero() {
        let mut buf = mock_buf(&[]);
        assert_eq!(0, buf.len());
        let mut codec = UntilClose::new();
        assert!(codec.decode(&mut buf).unwrap().is_none());
        assert_eq!(false, codec.remaining());
    }

    #[test]
    fn test_decode_until_close_buf_len_greater_than_zero() {
        let mut buf = mock_buf(&[0u8; 64]);
        assert_eq!(64, buf.len());
        let mut codec = UntilClose::new();
        assert_eq!(buf.len(), codec.decode(&mut buf).unwrap().unwrap().0.len());
        assert_eq!(true, codec.remaining());
        assert!(codec.decode(&mut buf).unwrap().is_none());
        assert_eq!(false, codec.remaining());
    }
}

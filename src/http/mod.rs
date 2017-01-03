pub mod request;
pub mod response;
pub mod body;

use std::ascii::AsciiExt;
use std::{slice, str};

use tokio_core::io::EasyBuf;

/// HTTP protocol version
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Version {
    /// `HTTP/0.9`
    Http09,
    /// `HTTP/1.0`
    Http10,
    /// `HTTP/1.1`
    Http11,
    /// `HTTP/2`
    Http2,
}

/// A piece of a message body.
#[derive(Debug, Eq, PartialEq)]
pub struct Chunk(pub Vec<u8>);

pub type Slice = (usize, usize);

pub struct Headers<'r> {
    headers: slice::Iter<'r, (Slice, Slice)>,
    data: &'r EasyBuf,
}

impl<'r> Headers<'r> {
    pub fn content_length(self) -> Option<usize> {
        self
            .rev()
            .find(|h| h.0.to_ascii_lowercase().as_str() == "content-length")
            .and_then(|h| {
                let v = ::std::str::from_utf8(&h.1).unwrap();
                v.parse::<usize>().ok()
            })
    }

    pub fn transfer_encoding_chunked(mut self) -> bool {
        match self
            .find(|h| h.0.to_ascii_lowercase().as_str() == "transfer-encoding") {

            Some(h) => {
                let v = ::std::str::from_utf8(&h.1).unwrap();
                v.to_ascii_lowercase() == "chunked"
            }
            None => {
                 false
            }
        }
    }
}

impl<'r> Iterator for Headers<'r> {
    type Item = (&'r str, &'r [u8]);

    fn next(&mut self) -> Option<(&'r str, &'r [u8])> {
        self.headers.next().map(|&(ref a, ref b)| {
            let a = &self.data.as_slice()[a.0..a.1];
            let b = &self.data.as_slice()[b.0..b.1];
            (str::from_utf8(a).unwrap(), b)
        })
    }
}

impl<'r> DoubleEndedIterator for Headers<'r> {
    fn next_back(&mut self) -> Option<(&'r str, &'r [u8])> {
        self.headers.next_back().map(|&(ref a, ref b)| {
            let a = &self.data.as_slice()[a.0..a.1];
            let b = &self.data.as_slice()[b.0..b.1];
            (str::from_utf8(a).unwrap(), b)
        })
    }
}

#[cfg(test)]
mod tests {
    use tokio_core::io::EasyBuf;

    use super::*;

    #[test]
    fn test_duplicate_content_length_header() {
        let head = b"Content-Length: 10\r\n\
            Content-Length: 16\r\n";

        let mut buf = EasyBuf::new();
        buf.get_mut().extend_from_slice(head);

        let slices = vec![
            ((0, 14), (16, 18)),
            ((20, 34), (36, 38)),
        ];

        let headers = Headers {
            headers: slices.iter(),
            data: &buf,
        };

        assert_eq!(16, headers.content_length().unwrap());
    }
}

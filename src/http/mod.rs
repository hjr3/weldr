pub mod parser;

use std::ascii::AsciiExt;
use std::fmt;
use std::io;

use tokio_proto::Error as ProtoError;

/// HTTP protocol version
#[derive(Debug, Eq, PartialEq)]
pub enum Version {
    /// `HTTP/0.9`
    Http09,
    /// `HTTP/1.0`
    Http10,
    /// `HTTP/1.1`
    Http11,
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Version::Http09 => write!(f, "HTTP/0.9"),
            Version::Http10 => write!(f, "HTTP/1.0"),
            Version::Http11 => write!(f, "HTTP/1.1"),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.value)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct RequestHead {
    pub method: String,
    pub uri: String,
    pub version: Version,
    pub headers: Vec<Header>,
}

#[derive(Debug, Eq, PartialEq)]
pub struct ResponseHead {
    pub version: Version,
    pub status: u16,
    pub reason: String,
    pub headers: Vec<Header>,
}

impl fmt::Display for ResponseHead {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        try!(write!(f, "{} {} {}\r\n", self.version, self.status, self.reason));

        for header in &self.headers {
            try!(write!(f, "{}\r\n", header));
        }

        write!(f, "\r\n")
    }
}

impl ResponseHead {
    pub fn content_length(&self) -> Option<usize> {
        self.headers
            .iter()
            .find(|h| h.name.to_ascii_lowercase().as_str() == "content-length")
            .map(|h| h.value.parse::<usize>().ok())
            .and_then(|len| len)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Response {
    pub head: ResponseHead,
    pub body: Vec<Chunk>,
}

/// A piece of a message body.
#[derive(Debug, Eq, PartialEq)]
pub struct Chunk(pub Vec<u8>);

#[derive(Debug)]
pub enum Error {
    Unknown,
    Invalid,
    Io(io::Error),
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<ProtoError<Error>> for Error {
    fn from(err: ProtoError<Error>) -> Error {
        match err {
            ProtoError::Transport(e) => e,
            ProtoError::Io(e) => Error::Io(e),
        }
    }
}

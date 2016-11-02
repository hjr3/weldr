pub mod parser;

use std::io;
use tokio_proto::Error as ProtoError;

#[derive(Debug, Eq, PartialEq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Eq, PartialEq)]
pub struct RequestHead {
    pub method: String,
    pub uri: String,
    pub version: String,
    pub headers: Vec<Header>,
}

#[derive(Debug)]
pub struct ResponseHead {
    pub version: String,
    pub status: String,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Response(pub Vec<u8>);

/// A piece of a message body.
#[derive(Debug, Eq, PartialEq)]
pub struct Chunk(Vec<u8>);

#[derive(Debug)]
pub enum Error {
    Unknown,
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

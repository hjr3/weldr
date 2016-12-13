pub mod request;
pub mod response;
pub mod body;

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

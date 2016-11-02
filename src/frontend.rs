use std::io;

use futures::{Poll, Async};
use tokio_proto::pipeline::Frame;

use bytes::{Buf, BufMut};
use bytes::ByteBuf;

use http;

use framed::{Parse, Serialize};

pub struct HttpParser {}

impl Parse for HttpParser {
    type Out = Frame<http::RequestHead, http::Chunk, http::Error>;

    fn parse(&mut self, buf: &mut ByteBuf) -> Poll<Self::Out, io::Error> {
        if buf.len() == 0 {
            return Ok(
                Async::Ready(
                    Frame::Done
                )
            );
        }

        trace!("Attempting to parse bytes into HTTP Request");

        let request = http::parser::parse_request_head(buf.bytes());

        debug!("Parser created: {:?}", request);

        buf.clear();

        Ok(
            Async::Ready(
                Frame::Message{
                    message: request,
                    body: false,
                }
            )
        )
    }
}

pub struct HttpSerializer {}

impl Serialize for HttpSerializer {

    type In = Frame<http::Response, http::Chunk, http::Error>;

    /// Serializes a frame into the buffer provided.
    ///
    /// This method will serialize `msg` into the byte buffer provided by `buf`.
    /// The `buf` provided is an internal buffer of the `ProxyFramed` instance and
    /// will be written out when possible.
    fn serialize(&mut self, msg: Self::In, buf: &mut ByteBuf) {
        trace!("Serializing message frame: {:?}", msg);

        let m = match msg {
            Frame::Message {
                ref message,
                body: _,
            } => {
                message.0.as_slice()
            }
            Frame::Error { error } => {
                error!("Upstream error: {:?}", error);
                b"HTTP/1.1 502 Bad Gateway\r\n\
                  Content-Length: 0\r\n\
                  \r\n"
            }
            _ => unimplemented!(),
        };

        trace!("Trying to write {} bytes", m.len());
        buf.copy_from_slice(&m[..]);
        trace!("Copied {} bytes", m.len());

    }
}

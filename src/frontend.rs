use std::io;

use futures::{Poll, Async};
use tokio_core::io::frame::{EasyBuf, Framed, Codec};
use tokio_proto::pipeline::Frame;

use bytes::{Buf, BufMut};
use bytes::ByteBuf;

use http;

pub struct HttpCodec {}

impl HttpCodec {
    fn new() -> HttpCodec {
        HttpCodec {}
    }
}

impl Codec for HttpCodec {
    type In = Frame<http::Response, http::Chunk, http::Error>;
    type Out = Frame<http::RequestHead, http::Chunk, http::Error>;

    fn decode(&mut self, buf: &mut EasyBuf) -> Result<Option<Self::In>, io::Error> {
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

    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) {
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
        buf.extend_from_slice(&m[..]);
        trace!("Copied {} bytes", m.len());
    }
}

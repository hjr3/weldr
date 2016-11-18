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
    type In = Frame<http::RequestHead, http::Chunk, http::Error>;
    type Out = Frame<http::Response, http::Chunk, http::Error>;

    fn decode(&mut self, buf: &mut EasyBuf) -> Result<Option<Self::In>, io::Error> {
        if buf.len() == 0 {
            return Ok(
                Async::Ready(
                    Frame::Done
                )
            );
        }

        trace!("Attempting to parse bytes into HTTP Request");

        let response = http::Response(Vec::from(buf.bytes()));

        debug!("Parser created: {:?}", response);

        buf.clear();

        return Ok(
            Async::Ready(
                Frame::Message{
                    message: response,
                    body: false,
                }
            )
        );
    }

    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) {
        trace!("Serializing message frame: {:?}", msg);

        match msg {
            Frame::Message { message, body: _ } => {
                let input = format!(
                    "{} {} HTTP/1.1\r\n\
                     Host: www.example.com\r\n\
                     Accept: */*\r\n\
                     \r\n",
                    message.method, message.uri);

                trace!("Computed message to send to backend: {}", input);
                trace!("Trying to write {} bytes", input.len());
                buf.copy_from_slice(input.as_bytes());
                trace!("Copied {} bytes", input.len());
            },
            Frame::Body { chunk} => {
                error!("Serializing body is not implemented: {:?}", chunk);
                ()
            },
            Frame::Error { error } => {
                error!("Dealing with error is not implemented: {:?}", error);
                ()
            },
            Frame::Done => (),
        }
    }
}

use std::io;

use futures::{Poll, Async};
use tokio_proto::pipeline::Frame;

use bytes::{ByteBuf, BufMut};

use http;

use framed::{Parse, Serialize};

pub struct HttpParser {}

impl Parse for HttpParser {
    type Out = Frame<http::Response, http::Chunk, http::Error>;

    fn parse(&mut self, buf: &mut ByteBuf) -> Poll<Self::Out, io::Error> {
        if buf.len() == 0 {
            return Ok(
                Async::Ready(
                    Frame::Done
                )
            );
        }

        trace!("Attempting to parse bytes into HTTP Request");

        let mut parser = http::parser::ResponseParser::new();
        let response = match parser.parse_response(buf) {
            Ok(Some(response)) => response,
            Ok(None) => panic!("Not enough bytes to parse response"),
            Err(e) => panic!("Error parsing response: {:?}", e),
        };

        debug!("Parser created: {:?}", response);

        return Ok(
            Async::Ready(
                Frame::Message{
                    message: response,
                    body: false,
                }
            )
        );
    }
}

pub struct HttpSerializer {}

impl Serialize for HttpSerializer {

    type In = Frame<http::RequestHead, http::Chunk, http::Error>;

    /// Serializes a frame into the buffer provided.
    ///
    /// This method will serialize `msg` into the byte buffer provided by `buf`.
    /// The `buf` provided is an internal buffer of the `ProxyFramed` instance and
    /// will be written out when possible.
    fn serialize(&mut self, msg: Self::In, buf: &mut ByteBuf) {
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

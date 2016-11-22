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

    type In = Frame<http::Request, http::Chunk, http::Error>;

    /// Serializes a frame into the buffer provided.
    ///
    /// This method will serialize `msg` into the byte buffer provided by `buf`.
    /// The `buf` provided is an internal buffer of the `ProxyFramed` instance and
    /// will be written out when possible.
    fn serialize(&mut self, msg: Self::In, buf: &mut ByteBuf) {
        trace!("Serializing message frame: {:?}", msg);

        match msg {
            Frame::Message { message, body: _ } => {
                let request = message;
                let head = format!("{}", request.head);
                trace!("Computed message to send to backend:\r\n{}", head);

                let head = head.into_bytes();
                trace!("Trying to write {} bytes from request head", head.len());
                buf.copy_from_slice(&head[..]);
                trace!("Copied {} bytes from request head", head.len());

                if !request.body.is_empty() {
                    match request.head.content_length() {
                        Some(_) => {
                            if let Some(chunk) = request.body.first() {
                                trace!("Trying to write {} bytes from request chunk", chunk.0.len());
                                buf.copy_from_slice(&chunk.0[..]);
                                trace!("Copied {} bytes from request chunk", chunk.0.len());
                            }
                        }
                        None => {
                            panic!("Transfer encoding chunked not implemented for request body");
                        }
                    }
                }
            }
            _ => unimplemented!(),
        }
    }
}

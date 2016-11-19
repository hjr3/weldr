use std::io;

use futures::{Poll, Async};
use tokio_proto::pipeline::Frame;

use bytes::BufMut;
use bytes::ByteBuf;

use http;

use framed::{Parse, Serialize};

pub struct HttpParser {}

impl Parse for HttpParser {
    type Out = Frame<http::Request, http::Chunk, http::Error>;

    fn parse(&mut self, buf: &mut ByteBuf) -> Poll<Self::Out, io::Error> {
        if buf.len() == 0 {
            return Ok(
                Async::Ready(
                    Frame::Done
                )
            );
        }

        trace!("Attempting to parse bytes into HTTP Request");

        let mut parser = http::parser::RequestParser::new();
        let request = match parser.parse_request(buf) {
            Ok(Some(request)) => request,
            Ok(None) => panic!("Not enough bytes to parse request"),
            Err(e) => panic!("Error parsing request: {:?}", e),
        };

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

        match msg {
            Frame::Message {
                ref message,
                body: _,
            } => {
                let response = message;
                let head = format!("{}", response.head).into_bytes();

                trace!("Trying to write {} bytes from response head", head.len());
                buf.copy_from_slice(&head[..]);
                trace!("Copied {} bytes from response head", head.len());

                match response.head.content_length() {
                    Some(_) => {
                        if let Some(chunk) = response.body.first() {
                            trace!("Trying to write {} bytes from response chunk", chunk.0.len());
                            buf.copy_from_slice(&chunk.0[..]);
                            trace!("Copied {} bytes from response chunk", chunk.0.len());
                        }
                    }
                    None => {
                        panic!("Transfer encoding chunked not implemented");
                    }
                }
            }
            Frame::Error { error } => {
                error!("Upstream error: {:?}", error);
                let e = b"HTTP/1.1 502 Bad Gateway\r\n\
                  Content-Length: 0\r\n\
                  \r\n";

                trace!("Trying to write {} bytes from response head", e.len());
                buf.copy_from_slice(&e[..]);
                trace!("Copied {} bytes from response head", e.len());

            }
            _ => unimplemented!(),
        }
    }
}

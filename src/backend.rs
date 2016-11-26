use std::io;

use futures::{Poll, Async};
use tokio_proto::pipeline::Frame;

use bytes::{ByteBuf, BufMut};

use http;

use framed::{Parse, Serialize};

pub struct HttpParser {
    parser: http::parser::ResponseParser,
}

impl HttpParser {
    pub fn new() -> HttpParser {
        HttpParser {
            parser: http::parser::ResponseParser::new(),
        }
    }
}

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

        if self.parser.is_streaming() {
            trace!("Extracting bytes from streaming response body");
            return match self.parser.parse_body(buf) {
                Ok(Some(body)) =>  {
                    Ok(
                        Async::Ready(
                            Frame::Body {
                                chunk: Some(body),
                            }
                        )
                    )
                }
                Ok(None) => {
                    debug!("No bytes in response");

                    Ok(
                        Async::Ready(
                            Frame::Body {
                                chunk: None,
                            }
                        )
                    )
                },
                Err(e) => {
                    error!("Tried to parse out body when the buffer was empty");
                    Ok(
                        Async::Ready(
                            Frame::Error {
                                error: e,
                            }
                        )
                    )
                }
            }
        }

        trace!("Attempting to parse bytes into HTTP Response");

        let response = match self.parser.parse_response(buf) {
            Ok(Some(response)) => response,
            Ok(None) => {
                debug!("Not enough bytes to parse response");
                return Ok(Async::NotReady);
            },
            Err(e) => {
                return Ok(
                    Async::Ready(
                        Frame::Error {
                            error: e,
                        }
                    )
                );
            }
        };

        debug!("Parser created: {:?}", response);

        return Ok(
            Async::Ready(
                Frame::Message{
                    message: response,
                    body: self.parser.is_streaming(),
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

use std::io;

use tokio_core::io::{Io, Codec, EasyBuf};
use tokio_proto::streaming::pipeline::{ClientProto, Frame};

use framed;
use http::Chunk;
use http::request::{self, Request};
use http::response::{self, Response};
use http::body;

pub struct Backend;

impl<T: Io + 'static> ClientProto<T> for Backend {
    type Request = Request;
    type RequestBody = Chunk;
    type Response = Response;
    type ResponseBody = Chunk;
    type Error = io::Error;
    type Transport = framed::Framed<T, HttpCodec>;
    type BindTransport = io::Result<framed::Framed<T, HttpCodec>>;

    fn bind_transport(&self, io: T) -> Self::BindTransport {
        Ok(framed::framed(io, HttpCodec::new()))
    }
}

enum BodyCodec {
    Length(body::Length),
    Chunked(body::Chunked),
}

pub struct HttpCodec {
    body_codec: Option<BodyCodec>
}

impl HttpCodec {
    pub fn new() -> HttpCodec {
        HttpCodec {
            body_codec: None,
        }
    }
}

impl Codec for HttpCodec {
    type In = Frame<Response, Chunk, io::Error>;
    type Out = Frame<Request, Chunk, io::Error>;

    fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Self::In>> {
        trace!("decode");
        //debug!("Raw response {:?}", buf.as_ref());
        //debug!("String response {:?}", unsafe {::std::str::from_utf8_unchecked(buf.as_ref())});
        if buf.len() == 0 {
            return Ok(None);
        }

        if let Some(ref mut codec) = self.body_codec {
            match *codec {
                BodyCodec::Length(ref mut codec) => {
                    if codec.remaining() != 0 {
                        return match try!(codec.decode(buf)) {
                            None => {
                                // TODO should this be an error?
                                debug!("Empty buffer?");
                                Ok(None)
                            }
                            Some(chunk) => {
                                Ok(
                                    Some(
                                        Frame::Body { chunk: Some(chunk) }
                                    )
                                )
                            }
                        };
                    }
                },
                BodyCodec::Chunked(ref mut codec) => {
                    return match try!(codec.decode(buf)) {
                        None => {
                            // TODO should this be an error?
                            debug!("Empty buffer?");
                            Ok(None)
                        }
                        Some(chunk) => {
                            Ok(
                                Some(
                                    Frame::Body { chunk: Some(chunk) }
                                )
                            )
                        }
                    };
                }
            }
        }

        match try!(response::decode(buf)) {
            None => {
                debug!("Partial response");
                Ok(None)
            }
            Some(mut response) => {
                if let Some(content_length) = response.content_length() {
                    let mut codec = body::Length::new(content_length);

                    match try!(codec.decode(buf)) {
                        None => {
                            debug!("Body with content length of {} but no more bytes in buffer", content_length);
                        }
                        Some(chunk) => {
                            response.append_data(chunk.0.as_ref());
                        }
                    }

                    if codec.remaining() > 0 {
                        self.body_codec = Some(BodyCodec::Length(codec));
                    }
                } else if response.is_chunked() {
                    self.body_codec = Some(BodyCodec::Chunked(body::Chunked{}));
                }

                Ok(
                    Some(
                        Frame::Message { message: response, body: self.body_codec.is_some() }
                    )
                )
            }
        }
    }

    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> io::Result<()> {
        trace!("encode");
        debug!("Request {:?}", msg);

        match msg {
            Frame::Message { message, body: _ } => {
                request::encode(message, buf);
            }
            Frame::Body { chunk } => {
                if let Some(mut chunk) = chunk {
                    buf.append(&mut chunk.0);
                }
            }
            Frame::Error { error } => {
                error!("Upstream error: {:?}", error);
                return Err(error);
            }
        }

        Ok(())
    }
}

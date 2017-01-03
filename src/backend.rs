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

pub struct HttpCodec {
    body_codec: Option<body::BodyCodec>
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
            if codec.remaining() == false {
                return Ok(
                    Some(
                        Frame::Body { chunk: None }
                    )
                );
            }

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
            }
        }

        match try!(response::decode(buf)) {
            None => {
                debug!("Partial response");
                Ok(None)
            }
            Some(mut response) => {
                let mut codec = if let Some(content_length) = response.content_length() {
                    info!("Found body with content length of {}", content_length);
                    body::BodyCodec::Length(body::Length::new(content_length))
                } else if response.transfer_encoding_chunked() {
                    info!("Found body with chunked transfer");
                    body::BodyCodec::Chunked(body::Chunked::new())
                } else {
                    info!("Found body with no content length or chunked transfer specified");
                    body::BodyCodec::UntilClose(body::UntilClose::new())
                };

                match try!(codec.decode(buf)) {
                    None => {
                        debug!("Decoding body but no more bytes in buffer");
                    }
                    Some(chunk) => {
                        response.append_data(chunk.0.as_ref());
                    }
                }

                if codec.remaining() {
                    self.body_codec = Some(codec);
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

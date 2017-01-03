use std::io;

use tokio_core::io::{Io, Codec, EasyBuf};
use tokio_core::reactor::Handle;
use tokio_proto::streaming::pipeline::{Frame, ServerProto};

use framed;
use http::Chunk;
use http::request::{self, Request};
use http::response::{self, Response};
use http::body;

pub struct Frontend {
    pub handle: Handle,
}

impl<T: Io + 'static> ServerProto<T> for Frontend {
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
    type In = Frame<Request, Chunk, io::Error>;
    type Out = Frame<Response, Chunk, io::Error>;

    fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Self::In>> {
        trace!("decode");
        debug!("Raw request {:?}", buf.as_ref());

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
            };
        }

        match try!(request::decode(buf)) {
            None => {
                debug!("Partial request");
                Ok(None)
            }
            Some(mut request) => {
                if let Some(content_length) = request.content_length() {
                    info!("Found body with content length of {}", content_length);
                    let mut codec = body::BodyCodec::Length(body::Length::new(content_length));

                    match try!(codec.decode(buf)) {
                        None => {
                            debug!("Decoding body but no more bytes in buffer");
                        }
                        Some(chunk) => {
                            request.append_data(chunk.0.as_ref());
                        }
                    }

                    if codec.remaining() {
                        self.body_codec = Some(codec);
                    }
                } else if request.transfer_encoding_chunked() {
                    info!("Found body with chunked transfer");
                    let mut codec = body::BodyCodec::Chunked(body::Chunked::new());

                    match try!(codec.decode(buf)) {
                        None => {
                            debug!("Decoding body but no more bytes in buffer");
                        }
                        Some(chunk) => {
                            request.append_data(chunk.0.as_ref());
                        }
                    }

                    if codec.remaining() {
                        self.body_codec = Some(codec);
                    }
                } else {
                    if buf.len() != 0 {
                        error!("No valid headers for request body");
                        let msg = format!("Found request body with no content length or chunked transfer specified: {:?}", request);
                        return Err(io::Error::new(io::ErrorKind::Other, msg));
                    }
                }

                Ok(
                    Some(
                        Frame::Message { message: request, body: self.body_codec.is_some() }
                    )
                )
            }
        }
    }

    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> io::Result<()> {
        trace!("encode");
        match msg {
            Frame::Message { message, body: _ } => {
                response::encode(message, buf);
            }
            Frame::Body { chunk } => {
                if let Some(mut chunk) = chunk {
                    buf.append(&mut chunk.0);
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
        }

        Ok(())
    }
}

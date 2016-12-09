use std::io;

use tokio_core::io::{Io, Codec, EasyBuf};
use tokio_proto::streaming::pipeline::{ClientProto, Frame};

use framed;
use request::{self, Request};
use response::{self, Response};

pub struct Backend;

impl<T: Io + 'static> ClientProto<T> for Backend {
    type Request = Request;
    type RequestBody = Vec<u8>;
    type Response = Response;
    type ResponseBody = Vec<u8>;
    type Error = io::Error;
    type Transport = framed::Framed<T, HttpCodec>;
    type BindTransport = io::Result<framed::Framed<T, HttpCodec>>;

    fn bind_transport(&self, io: T) -> Self::BindTransport {
        Ok(framed::framed(io, HttpCodec::new()))
    }
}

pub struct HttpCodec {
    content_length_remaining: Option<usize>,
}

impl HttpCodec {
    pub fn new() -> HttpCodec {
        HttpCodec {
            content_length_remaining: None,
        }
    }
}

impl Codec for HttpCodec {
    type In = Frame<Response, Vec<u8>, io::Error>;
    type Out = Frame<Request, Vec<u8>, io::Error>;

    fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Self::In>> {
        trace!("decode");
        //debug!("Raw response {:?}", buf.as_ref());
        //debug!("String response {:?}", unsafe {::std::str::from_utf8_unchecked(buf.as_ref())});
        if buf.len() == 0 {
            return Ok(None);
        }

        if let Some(content_length) = self.content_length_remaining {
            debug!("Found content length remaining: {:?} bytes", self.content_length_remaining);
            debug!("Buffer length remaining: {:?} bytes", buf.len());
            let len = ::std::cmp::min(content_length, buf.len());
            let raw = buf.drain_to(len);
            let body = Vec::from(raw.as_ref());

            if len == content_length {
                self.content_length_remaining = None;
            } else {
                self.content_length_remaining = Some(content_length - len);
            }

            debug!("Content length remaining: {:?} bytes", self.content_length_remaining);

            return Ok(
                Some(
                    Frame::Body { chunk: Some(body) }
                )
            );
        }

        match try!(response::decode(buf)) {
            None => {
                debug!("Partial response");
                Ok(None)
            }
            Some(response) => {
                self.content_length_remaining = response.content_length_remaining;
                debug!("Content length of {:?} remaining", self.content_length_remaining);

                Ok(
                    Some(
                        Frame::Message { message: response, body: self.content_length_remaining.is_some() }
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
                    buf.append(&mut chunk);
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

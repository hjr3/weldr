use std::io;

use tokio_core::io::{Io, Codec, EasyBuf};
use tokio_core::reactor::Handle;
use tokio_proto::streaming::pipeline::{Frame, ServerProto};

use framed;
use request::{self, Request};
use response::{self, Response};

pub struct Frontend {
    pub handle: Handle,
}

impl<T: Io + 'static> ServerProto<T> for Frontend {
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
}

impl HttpCodec {
    pub fn new() -> HttpCodec {
        HttpCodec {
        }
    }
}

impl Codec for HttpCodec {
    type In = Frame<Request, Vec<u8>, io::Error>;
    type Out = Frame<Response, Vec<u8>, io::Error>;

    fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Self::In>> {
        trace!("decode");
        debug!("Raw request {:?}", buf.as_ref());

        match try!(request::decode(buf)) {
            None => {
                debug!("Partial request");
                Ok(None)
            }
            Some(request) => {
                // TODO handle streaming body
                Ok(
                    Some(
                        Frame::Message { message: request, body: false }
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
                    buf.append(&mut chunk);
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

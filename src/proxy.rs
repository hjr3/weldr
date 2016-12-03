use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::result;

use futures::{Future, Async, Poll};
use futures::stream::Stream;
use tokio_core::io::FramedIo;
use tokio_core::net::{TcpStream, TcpListener};
use tokio_core::reactor::Core;
//use tokio_proto::{Message, Body};
use tokio_proto::pipeline::Frame;

use http;
use framed;
use backend;
use frontend;
use pool::Pool;

type Frontend = framed::ProxyFramed<TcpStream, frontend::HttpParser, frontend::HttpSerializer>;
type Backend = framed::ProxyFramed<TcpStream, backend::HttpParser, backend::HttpSerializer>;

#[must_use = "Must use Proxy"]
pub struct Proxy {
    frontend: Frontend,
    backend: Backend,
    f_out_frames: VecDeque<Frame<http::Request, http::Chunk, http::Error>>,
    b_out_frames: VecDeque<Frame<http::Response, http::Chunk, http::Error>>,
    f_run: bool,
    b_run: bool,
}

impl Proxy {
    pub fn new(frontend: Frontend, backend: Backend) -> Proxy {
        Proxy {
            frontend: frontend,
            backend: backend,
            f_out_frames: VecDeque::with_capacity(32),
            b_out_frames: VecDeque::with_capacity(32),
            f_run: true,
            b_run: true,
        }
    }

    fn is_done(&self) -> bool {
        !self.f_run && !self.b_run && self.f_out_frames.is_empty() && self.b_out_frames.is_empty()
    }

    fn process_f_out_frame(&mut self, frame: Frame<http::Request, http::Chunk, http::Error>) -> io::Result<()> {
        match frame {
            Frame::Message { message, body } => {
                if body {
                    trace!("frontend read out message with body");

                    self.f_out_frames.push_back(Frame::Message { message: message, body: body });
                } else {
                    trace!("frontend read out message");

                    self.f_out_frames.push_back(Frame::Message { message: message, body: body });
                }
            }
            Frame::Body { chunk } => {
                match chunk {
                    Some(chunk) => {
                        trace!("frontend read out body chunk");
                        self.f_out_frames.push_back(Frame::Body { chunk: Some(chunk) });
                    }
                    None => {
                        trace!("frontend read out body EOF");
                    }
                }
            }
            Frame::Done => {
                trace!("frontend read Frame::Done");
                // At this point, we just return. This works
                // because tick() will be called again and go
                // through the read-cycle again.
                self.f_run = false;
            }
            Frame::Error { .. } => {
                // At this point, the transport is toast, there
                // isn't much else that we can do. Killing the task
                // will cause all in-flight requests to abort, but
                // they can't be written to the transport anyway...
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "An error occurred."));
            }
        }

        Ok(())
    }

    fn process_b_out_frame(&mut self, frame: Frame<http::Response, http::Chunk, http::Error>) -> io::Result<()> {
        match frame {
            Frame::Message { message, body } => {
                if body {
                    trace!("backend read out message with body");

                    self.b_out_frames.push_back(Frame::Message { message: message, body: body });
                } else {
                    trace!("backend read out message");

                    self.b_out_frames.push_back(Frame::Message { message: message, body: body });
                }
            }
            Frame::Body { chunk } => {
                match chunk {
                    Some(chunk) => {
                        trace!("backend read out body chunk");
                        self.b_out_frames.push_back(Frame::Body { chunk: Some(chunk) });
                    }
                    None => {
                        trace!("backend read out body EOF");
                    }
                }
            }
            Frame::Done => {
                trace!("backend read Frame::Done");
                // At this point, we just return. This works
                // because tick() will be called again and go
                // through the read-cycle again.
                self.b_run = false;
            }
            Frame::Error { .. } => {
                // At this point, the transport is toast, there
                // isn't much else that we can do. Killing the task
                // will cause all in-flight requests to abort, but
                // they can't be written to the transport anyway...
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "An error occurred."));
            }
        }

        Ok(())
    }
}

impl Future for Proxy {
    type Item = ();
    type Error = io::Error;

    // Tick the proxy state machine
    fn poll(&mut self) -> Poll<(), io::Error> {
        trace!("Proxy::tick");

        debug!("Flush any buffered writes first");
        try!(self.frontend.flush());
        try!(self.backend.flush());

        debug!("Reading unparsed request data from the frontend");
        while self.f_run {
            if let Async::Ready(frame) = try!(self.frontend.read()) {
                try!(self.process_f_out_frame(frame));
            } else {
                break;
            }
        }

        debug!("Writing parsed head and/or bodies to the backend");
        loop {
            // TODO fix this to use a sink so that we don't lose a message if there is back
            // pressure. I think poll_write() should protect against this though.
            if let Some(message) = self.f_out_frames.pop_front() {
                try!(self.backend.write(message));
            } else {
                break;
            }
        }

        debug!("Reading unparsed request data from the backend");
        while self.b_run {
            if let Async::Ready(frame) = try!(self.backend.read()) {
                try!(self.process_b_out_frame(frame));
            } else {
                break;
            }
        }

        debug!("Writing {} parsed response head and/or bodies to the frontend", self.b_out_frames.len());
        loop {
            if let Some(message) = self.b_out_frames.pop_front() {
                try_ready!(self.frontend.write(message));
            } else {
                break;
            }
        }

        debug!("Flushing buffered writes");
        try!(self.frontend.flush());
        try!(self.backend.flush());

        // Clean shutdown of the proxy server can happen when
        //
        // 1. The server is done running, this is signaled by ProxyFramed::read()
        //    returning Frame::Done for both the frontend and backend.
        //
        // 2. ProxyFramed is done writing all data to the socket, this is
        //    signaled by ProxyFramed::flush() returning Ok(Some(())) for both the frontend and the
        //    backend.
        //
        // 3. There are no further request messages to the backend and no further response messages
        //    to write to the frontend.
        //
        // It is necessary to perfom these three checks in order to handle the
        // case where the frontend shuts down half the socket or the backend shuts down half the
        // socket.
        //
        if self.is_done() {
            debug!("Proxying request/response done");
            return Ok(().into())
        }

        // Tick again later
        Ok(Async::NotReady)
    }
}

pub fn listen(addr: SocketAddr, pool: Pool) -> result::Result<(), http::Error> {

    let mut lp = Core::new().unwrap();
    let handle = lp.handle();

    let listener = TcpListener::bind(&addr, &handle.clone()).expect("Failed to bind to socket");
    info!("Listening on: {}", addr);

    let f = listener.incoming().for_each(|(fsock, addr)| {
        debug!("Incoming connection on {}", addr);

        let addr = pool.get().expect("Failed to get address from pool");
        debug!("Preparing backend request to {:?}", addr);

        let proxy = TcpStream::connect(&addr, &handle.clone()).and_then(|bsock| {
            let frontend = framed::ProxyFramed::new(fsock, frontend::HttpParser {}, frontend::HttpSerializer {});
            let backend = framed::ProxyFramed::new(bsock, backend::HttpParser::new(), backend::HttpSerializer {});

            Proxy::new(frontend,backend)
        }).map(|()| {
            debug!("Finished proxying");
            ()
        }).map_err(|e| {
            error!("Error occurred during proxy: {:?}", e);
            ()
        });

        handle.spawn(proxy);

        Ok(())
    });

    lp.run(f).expect("Unexpected error while proxying connection");

    Ok(())
}

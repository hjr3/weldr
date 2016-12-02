use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::result;

use futures::{Future, Async, Poll};
use futures::stream::Stream;
use tokio_core::io::FramedIo;
use tokio_core::net::{TcpStream, TcpListener};
use tokio_core::reactor::Core;
use tokio_proto::{Message, Body};

use http;
use framed;
use backend;
use frontend;
use pool::Pool;

pub struct Proxy<F, B> where F: FramedIo, B: FramedIo {
    frontend: F, // framed::ProxyFramed<TcpStream, frontend::HttpParser, frontend::HttpSerializer>,
    backend: B, // framed::ProxyFramed<TcpStream, backend::HttpParser, backend::HttpSerializer>,
    f_out_frames: VecDeque<F::Out>,
}

impl<F, B> Proxy<F, B> where F: FramedIo, B: FramedIo {
    pub fn new(frontend: F, backend: B) -> Proxy<F, B> {
        Proxy {
            frontend: frontend,
            backend: backend,
            f_out_frames: VecDeque::with_capacity(32),
        }
    }
}

impl<F, B> Future for Proxy<F, B> where F: FramedIo, B: FramedIo {
    type Item = ();
    type Error = io::Error;

    // Tick the proxy state machine
    fn poll(&mut self) -> Poll<(), io::Error> {
        trace!("Proxy::tick");

        // Always flush first
        try!(self.frontend.flush());
        try!(self.backend.flush());

        // Read unparsed request data from the frontend
        loop {
            if let Async::Ready(frame) = try!(self.frontend.read()) {
                self.f_out_frames.push_back(frame);
            } else {
                break;
            }
        }

        // Write parsed head and/or bodies to the backend
        loop {
            if let Some(message) = self.f_out_frames.front_mut() {
                try!(self.backend.write(message));
            } else {
                break;
            }

            self.f_out_frames.pop_front();
        }
//
//        // Read unparsed response data from the backend
//        loop {
//            try_ready!(self.backend.read());
//        }
//
//        // Write parsed response head and/or bodies to the backend
//        loop {
//            if let Some(message) = self.backend.messages.front_mut() {
//                try_ready!(self.backend.write(message);
//            } else {
//                break;
//            }
//
//            self.backend.messages.pop_front();
//        }
//
//        // Try flushing buffered writes
//        try!(self.frontend.flush());
//        try!(self.backend.flush());
//
//        // Clean shutdown of the proxy server can happen when
//        //
//        // 1. The server is done running, this is signaled by ProxyFramed::read()
//        //    returning Frame::Done for both the frontend and backend.
//        //
//        // 2. ProxyFramed is done writing all data to the socket, this is
//        //    signaled by ProxyFramed::flush() returning Ok(Some(())) for both the frontend and the
//        //    backend.
//        //
//        // 3. There are no further request messages to the backend and no further response messages
//        //    to write to the frontend.
//        //
//        // It is necessary to perfom these three checks in order to handle the
//        // case where the frontend shuts down half the socket or the backend shuts down half the
//        // socket.
//        //
//        if self.is_done() {
//            return Ok(().into())
//        }

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

        let proxy = TcpStream::connect(&addr, &handle.clone()).map(|bsock| {
            let frontend = framed::ProxyFramed::new(fsock, frontend::HttpParser {}, frontend::HttpSerializer {});
            let backend = framed::ProxyFramed::new(bsock, backend::HttpParser::new(), backend::HttpSerializer {});

            Proxy::new(frontend,backend)
        }).map(|_| {
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

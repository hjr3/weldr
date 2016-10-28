use std::io;

use futures::{Future, Poll, Async};
use tokio_core::net::TcpStream;
use tokio_proto::{TryRead, TryWrite};
use bytes::Buf;
use bytes::buf::RingBuf;

use http_parser::{self, HttpState, RequestState, ResponseState};

#[derive(Debug)]
enum ConnectionState {
    ClientReading,
    ClientWriting,
    ServerReading,
    ServerWriting,
}

#[must_use = "Must use Pipe"]
pub struct Pipe {
    client: TcpStream,
    server: TcpStream,
    state: ConnectionState,

    /// A buffer to keep track of the data betewen the client and server
    buf: RingBuf,
}

impl Pipe {
    pub fn new(client: TcpStream, server: TcpStream) -> Pipe {
        Pipe {
            client: client,
            server: server,
            state: ConnectionState::ClientReading,
            buf: RingBuf::with_capacity(1024),
        }
    }
}

impl Future for Pipe {
    type Item = ();
    type Error = io::Error;

    // assuming that we are http 1.1, which says a request/response must complete before
    // the next request/response can be served. this is a naive approach and will be
    // changed once http2 support is put in.
    fn poll(&mut self) -> Poll<(), io::Error> {
        trace!("Polling...");

        loop {

            match self.state {
                ConnectionState::ClientReading => {
                    trace!("Reading from {}", self.client.local_addr().unwrap());

                    let bytes = try_ready!(self.client.try_read_buf(&mut self.buf));
                    trace!("Read {} bytes from {}", bytes, self.client.local_addr().unwrap());

                    let state = http_parser::parse_request_until_stop(
                        HttpState::new(),
                        "",
                        self.buf.bytes());

                    debug!("Parse request state is {:?}", state);

                    match state.request {
                        Some(RequestState::Request(_,_,_)) => {
                            self.state = ConnectionState::ServerWriting;
                            trace!("State switched to {:?}", self.state);
                        }
                        Some(RequestState::Error(e)) => {
                            error!("Error when parsing request: {:?}", e);
                            break;
                        }
                        _ => {
                            trace!("Not enough progress parsing request. Remaining at {:?}", self.state);
                        }
                    }
                }

                ConnectionState::ServerWriting => {
                    trace!("Writing to {}", self.server.peer_addr().unwrap());

                    if self.buf.bytes().len() == 0 {
                        self.state = ConnectionState::ServerReading;
                        trace!("Buffer is empty. State switched to {:?}", self.state);
                    } else {
                        let bytes = try_ready!(self.server.try_write_buf(&mut self.buf));
                        trace!("Wrote {} bytes to {}", bytes, self.server.peer_addr().unwrap());
                    }
                }

                ConnectionState::ServerReading => {
                    trace!("Reading from {}", self.server.peer_addr().unwrap());

                    let bytes = try_ready!(self.server.try_read_buf(&mut self.buf));
                    trace!("Read {} bytes from {}", bytes, self.server.peer_addr().unwrap());

                    let state = http_parser::parse_response_until_stop(
                        HttpState::new(),
                        "",
                        self.buf.bytes());

                    debug!("Parse response state is {:?}", state);

                    match state.response {
                        Some(ResponseState::ResponseWithBody(_,_,_)) => {
                            self.state = ConnectionState::ClientWriting;
                            trace!("State switched to {:?}", self.state);
                        }
                        Some(ResponseState::Error(e)) => {
                            error!("Error when parsing response: {:?}", e);
                            break;
                        }
                        _ => {
                            trace!("Not enough progress parsing response. Remaining at {:?}", self.state);
                        }
                    }
                }

                ConnectionState::ClientWriting => {
                    trace!("Writing to {}", self.client.local_addr().unwrap());

                    if self.buf.bytes().len() == 0 {
                        self.state = ConnectionState::ClientReading;
                        trace!("Buffer is empty. State switched to {:?}", self.state);
                        trace!("Request/Response is finished. Bailing out of loop.");
                        break;
                    } else {
                        let bytes = try_ready!(self.client.try_write_buf(&mut self.buf));
                        trace!("Wrote {} bytes to {}", bytes, self.client.local_addr().unwrap());
                    }
                }
            }
        }

        Ok(Async::Ready(()))
    }
}

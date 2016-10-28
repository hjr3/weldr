use std::io;

use futures::{Future, Poll, Async};
use tokio_core::net::TcpStream;
use tokio_proto::{TryRead, TryWrite};
use bytes::buf::RingBuf;

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

    fn poll(&mut self) -> Poll<(), io::Error> {
        trace!("Polling...");

        loop {

            match self.state {
                ConnectionState::ClientReading => {
                    trace!("Reading from {}", self.client.local_addr().unwrap());

                    let bytes = try_ready!(self.client.try_read_buf(&mut self.buf));
                    trace!("Read {} bytes from {}", bytes, self.client.local_addr().unwrap());

                    if bytes == 0 {
                        self.state = ConnectionState::ServerWriting;
                        trace!("State switched to {:?}", self.state);
                    }
                }

                ConnectionState::ServerWriting => {
                    trace!("Writing to {}", self.server.peer_addr().unwrap());

                    let bytes = try_ready!(self.server.try_write_buf(&mut self.buf));
                    trace!("Wrote {} bytes to {}", bytes, self.server.peer_addr().unwrap());

                    if bytes == 0 {
                        self.state = ConnectionState::ServerReading;
                        trace!("State switched to {:?}", self.state);
                    }
                }

                ConnectionState::ServerReading => {
                    trace!("Reading from {}", self.server.peer_addr().unwrap());

                    let bytes = try_ready!(self.server.try_read_buf(&mut self.buf));
                    trace!("Read {} bytes from {}", bytes, self.server.peer_addr().unwrap());

                    if bytes == 0 {
                        self.state = ConnectionState::ClientWriting;
                        trace!("State switched to {:?}", self.state);
                    }
                }

                ConnectionState::ClientWriting => {
                    trace!("Writing to {}", self.client.local_addr().unwrap());

                    let bytes = try_ready!(self.client.try_write_buf(&mut self.buf));
                    trace!("Wrote {} bytes to {}", bytes, self.client.local_addr().unwrap());

                    if bytes == 0 {
                        self.state = ConnectionState::ClientReading;
                        trace!("State switched to {:?}", self.state);
                        trace!("Bailing out of loop");
                        break;
                    }
                }
            }
        }

        Ok(Async::Ready(()))
    }
}

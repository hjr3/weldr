use std::io;
use std::net::SocketAddr;

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
    client_addr: SocketAddr,
    server_addr: SocketAddr,
    client: TcpStream,
    server: TcpStream,
    state: ConnectionState,

    /// A buffer to keep track of the data betewen the client and server
    buf: RingBuf,
}

impl Pipe {
    pub fn new(client_addr: SocketAddr, client: TcpStream, server_addr: SocketAddr, server: TcpStream) -> Pipe {
        Pipe {
            client_addr: client_addr,
            server_addr: server_addr,
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
                    trace!("Reading from {}", self.client_addr);

                    let bytes = try_ready!(self.client.try_read_buf(&mut self.buf));
                    trace!("Read {} bytes from {}", bytes, self.client_addr);

                    if bytes == 0 {
                        self.state = ConnectionState::ServerWriting;
                        trace!("State switched to {:?}", self.state);
                    }
                }

                ConnectionState::ServerWriting => {
                    trace!("Writing to {}", self.server_addr);

                    let bytes = try_ready!(self.server.try_write_buf(&mut self.buf));
                    trace!("Wrote {} bytes to {}", bytes, self.server_addr);

                    if bytes == 0 {
                        self.state = ConnectionState::ServerReading;
                        trace!("State switched to {:?}", self.state);
                    }
                }

                ConnectionState::ServerReading => {
                    trace!("Reading from {}", self.server_addr);

                    let bytes = try_ready!(self.server.try_read_buf(&mut self.buf));
                    trace!("Read {} bytes from {}", bytes, self.server_addr);

                    if bytes == 0 {
                        self.state = ConnectionState::ClientWriting;
                        trace!("State switched to {:?}", self.state);
                    }
                }

                ConnectionState::ClientWriting => {
                    trace!("Writing to {}", self.client_addr);

                    let bytes = try_ready!(self.client.try_write_buf(&mut self.buf));
                    trace!("Wrote {} bytes to {}", bytes, self.client_addr);

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

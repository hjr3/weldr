use std::io::{self, Read, Write};
use std::net::SocketAddr;

use futures::{Future, Poll};
use tokio_core::net::TcpStream;

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

    /// The buffer from the client to send to the server
    send_buf: Vec<u8>,

    /// The buffer from the server to send to the client
    recv_buf: Vec<u8>,
}

impl Pipe {
    pub fn new(client_addr: SocketAddr, client: TcpStream, server_addr: SocketAddr, server: TcpStream) -> Pipe {
        Pipe {
            client_addr: client_addr,
            server_addr: server_addr,
            client: client,
            server: server,
            state: ConnectionState::ClientReading,
            send_buf: Vec::new(),
            recv_buf: Vec::new(),
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
                    loop {
                        trace!("Reading from {}", self.client_addr);

                        // TODO should really be a VecDequeue in case we read more than once
                        let bytes = try_nb!(self.client.read_to_end(&mut self.send_buf));
                        trace!("Read {} bytes from {}", bytes, self.client_addr);

                        if bytes == 0 {
                            self.state = ConnectionState::ServerWriting;
                            trace!("State switched to {:?}", self.state);
                            break;
                        }
                    }
                }

                ConnectionState::ServerWriting => {
                    trace!("Writing to {}", self.server_addr);
                    try_nb!(self.server.write_all(&mut self.send_buf));
                    trace!("Wrote {} bytes to {}", self.send_buf.len(), self.server_addr);

                    self.server.shutdown(::std::net::Shutdown::Write).expect("Failed to shutdown writes for server socket");
                    self.state = ConnectionState::ServerReading;
                    trace!("State switched to {:?}", self.state);
                }

                ConnectionState::ServerReading => {
                    loop {
                        trace!("Reading from {}", self.server_addr);

                        let bytes = try_nb!(self.server.read_to_end(&mut self.recv_buf));
                        trace!("Read {} bytes from {}", bytes, self.server_addr);

                        if bytes == 0 {
                            self.state = ConnectionState::ClientWriting;
                            trace!("State switched to {:?}", self.state);
                            break;
                        }
                    }
                }

                ConnectionState::ClientWriting => {
                    trace!("Writing to {}", self.client_addr);
                    try_nb!(self.client.write_all(&mut self.recv_buf));
                    trace!("Wrote {} bytes to {}", self.recv_buf.len(), self.client_addr);

                    self.state = ConnectionState::ClientReading;
                    trace!("State switched to {:?}", self.state);
                }
            }
        }
    }
}

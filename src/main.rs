#[macro_use]
extern crate mioco;
extern crate env_logger;

#[macro_use]
extern crate log;

use std::str::FromStr;
use std::io::{self, Read, Write};
use std::net::SocketAddr;
use mioco::tcp::TcpListener;
use mioco::tcp::TcpStream;

#[derive(Clone)]
struct Backend {
    servers: Vec<SocketAddr>
}

impl Backend {
    fn forward(&self, send_buf: &[u8], recv_buf: &mut [u8]) -> io::Result<usize> {
        let server = self.servers.first().unwrap();

        trace!("Connecting to backend server on {}", server);
        let mut stream = TcpStream::connect(server).unwrap();

        trace!("Writing {} bytes to backend server", send_buf.len());
        let bytes_sent = try!(stream.write(send_buf));
        trace!("Wrote {} bytes to backend server", bytes_sent);

        let size = try!(stream.read(recv_buf));
        trace!("Read {} bytes from backend server", size);

        Ok(size)
    }
}

fn main() {
    env_logger::init().unwrap();

    let addr = FromStr::from_str("127.0.0.1:5555").unwrap();
    let listener = TcpListener::bind(&addr).unwrap();

    mioco::start(move ||{

        info!("Starting alacrity server on {}", addr);

        for _ in 0..mioco::thread_num() {
            let listener = try!(listener.try_clone());
            mioco::spawn(move || {

                loop {
                    let mut conn = try!(listener.accept());
                    trace!("Accepted conncection");

                    mioco::spawn(move || {
                        let mut buf = [0u8; 1024 * 16];
                        loop {
                            let size = try!(conn.read(&mut buf));
                            trace!("Read {} bytes from socket", size);
                            if size == 0 {/* eof */ break; }

                            let mut conn = try!(conn.try_clone());

                            mioco::spawn(move || {
                                let mut recv_buf = [0u8; 1024 * 16];
                                let servers = vec!(FromStr::from_str("127.0.0.1:8000").unwrap());
                                let backend = Backend { servers: servers };
                                let recv_size = match backend.forward(&buf[0..size], &mut recv_buf) {
                                    Ok(size) => size,
                                    Err(e) => {
                                        error!("Failed to foward to backend: {:?}", e);
                                        return Ok(());
                                    },
                                };
                                try!(conn.write_all(&mut recv_buf[0..recv_size]));

                                Ok(())
                            });
                        }

                        Ok(())
                    });
                }
            })
        }

        Ok(())
    });
}

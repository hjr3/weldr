#[macro_use] extern crate log;
extern crate env_logger;
extern crate futures;
extern crate tokio_core;

use std::env;
use std::io::{Read, Write};
use std::net::SocketAddr;

use futures::Future;
use futures::stream::Stream;
use tokio_core::reactor::Core;
use tokio_core::net::{TcpListener, TcpStream};

struct Pipe {
    client: TcpStream,
    server: TcpStream,
    buf: Vec<u8>,
}

fn main() {
    env_logger::init().unwrap();

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    let backend = env::args().nth(2).unwrap_or("127.0.0.1:12345".to_string());
    let backend = backend.parse::<SocketAddr>().unwrap();

    // Create the event loop that will drive this server
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();
    let h2 = handle.clone();

    let s = TcpListener::bind(&addr, &handle.clone());

    // Create a TCP listener which will listen for incoming connections
    let listener = lp.run(futures::done(s)).unwrap();

    info!("Listening on: {}", addr);

    let pipe = listener.incoming().map(move |(socket, addr)| {
        debug!("Incoming connection on {}", addr);

        // FIXME move this into the spawn so it is not blocking the main thread?
        let connected = TcpStream::connect(&backend, &h2);

        connected.and_then(move |server| {
            Ok (
                Pipe {
                    client: socket,
                    server: server,
                    buf: Vec::new(),
                }
            )
        }).boxed()
    });

    // Ideal: read(client, ...).and_then(write_all(server, ...).and_then(read(server, ...).and_then(write_all(client, ...)

    let server = pipe.for_each(|pipe| {
        trace!("Connecting pipe");

        let done = pipe.and_then(|mut pipe| {
            let bytes = pipe.client.read_to_end(&mut pipe.buf).expect("Failed to read from client");
            debug!("Read {} bytes from client {}", bytes, &pipe.client.peer_addr().unwrap());

            futures::done(Ok(pipe))
        }).and_then(|mut pipe| {
            pipe.server.write_all(&pipe.buf).expect("Failed to write to server");
            debug!("Wrote {} bytes to server {}", &pipe.buf.len(), &pipe.server.peer_addr().unwrap());

            futures::done(Ok(pipe))
        }).and_then(|mut pipe| {
            pipe.buf.clear();

            let bytes = pipe.server.read_to_end(&mut pipe.buf).expect("Failed to read from server");
            println!("Read {} bytes from server {}", bytes, &pipe.server.peer_addr().unwrap());

            futures::done(Ok(pipe))
        }).and_then(|mut pipe| {
            pipe.client.write_all(&pipe.buf).expect("Failed to write to client");
            debug!("Wrote {} bytes to client {}", &pipe.buf.len(), &pipe.client.peer_addr().unwrap());

            futures::finished(())
        }).map_err(|e| { // spawn expects an error type of () and we are passing through io::Error
            error!("Error trying proxy - {}", e);
            ()
        });

        handle.spawn(done);

        Ok(())
    });

    lp.run(server).unwrap();
}

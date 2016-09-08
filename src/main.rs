extern crate env_logger;
extern crate futures;
extern crate tokio_core;

use std::env;
use std::io::{Read, Write};
use std::net::SocketAddr;

use futures::Future;
use futures::stream::Stream;
use tokio_core::{Loop, TcpStream};

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
    let mut lp = Loop::new().unwrap();
    let handle = lp.handle();

    // Create a TCP listener which will listen for incoming connections
    let listener = lp.run(handle.clone().tcp_listen(&addr)).unwrap();

    let pin = lp.pin();

    println!("Listening on: {}", addr);

    let pipe = listener.incoming().map(move |(socket, addr)| {
        println!("Incoming connection on {}", addr);

        let handle = handle.clone();
        let connected = handle.tcp_connect(&backend);

        connected.and_then(move |server| {
            Ok (
                Pipe {
                    client: socket,
                    server: server,
                    buf: Vec::new(),
                }
            )
        })
    });

    //let pipe = pipe.and_then(|mut pipe| {
    //    let bytes = pipe.client.read_to_end(&mut pipe.buf).expect("Failed to read from client");

    //    println!("Read {} bytes", bytes);
    //    pipe
    //});

    let server = pipe.for_each(|pipe| {
        println!("Connecting pipe");
        pin.spawn(pipe.and_then(|mut pipe| {
            println!("About to read");

            let bytes = pipe.client.read_to_end(&mut pipe.buf).expect("Failed to read from client");

            println!("Read {} bytes", bytes);

            futures::finished(())
        }).map_err(|e| { // spawn expects an error type of () and we are passing through io::Error
            println!("Error when reading from client - {}", e);
            ()
        }));

        Ok(())
    });

    lp.run(server).unwrap();
}

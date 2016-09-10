#[macro_use] extern crate log;
extern crate env_logger;
#[macro_use] extern crate futures;
#[macro_use] extern crate tokio_core;

// pub mod for now until the entire API is used internally
pub mod pool;
mod pipe;

use std::env;
use std::net::SocketAddr;

use futures::{Future};
use futures::stream::Stream;
use tokio_core::reactor::Core;
use tokio_core::net::{TcpListener, TcpStream};
use pool::Pool;

fn main() {
    env_logger::init().unwrap();

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    let backend = env::args().nth(2).unwrap_or("127.0.0.1:12345".to_string());
    let mut pool = Pool::new(vec![backend]).unwrap();

    // Create the event loop that will drive this server
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();
    let h2 = handle.clone();

    let s = TcpListener::bind(&addr, &handle.clone());

    // Create a TCP listener which will listen for incoming connections
    let listener = lp.run(futures::done(s)).unwrap();

    info!("Listening on: {}", addr);

    let proxy = listener.incoming().for_each(|(sock, addr)| {
        debug!("Incoming connection on {}", addr);

        let backend = pool.get().unwrap();

        // TODO turn this into a pool managed by raft
        let pipe = TcpStream::connect(&backend, &h2).and_then(move |server| {

            pipe::Pipe::new(
                addr,
                sock,
                backend,
                server
            )

        }).and_then(|_| {
            debug!("Finished proxying");
            futures::finished(())
        }).map_err(|e| {
            error!("Error trying proxy - {}", e);
            ()
        });

        // spawn expects Item=Async(()), Error=()
        handle.spawn(pipe);

        Ok(())
    });

    lp.run(proxy).unwrap();
}

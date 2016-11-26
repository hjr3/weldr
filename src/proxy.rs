use std::net::SocketAddr;
use std::result;

use futures::{Future, Async};
use futures::stream::Stream;
use tokio_service::Service;
use tokio_proto::{pipeline, Message, Body};
use tokio_core::reactor::{Core, Handle};
use tokio_core::net::{TcpStream, TcpListener};

use http;
use framed;
use backend;
use frontend;
use pool::Pool;

pub struct Proxy {
    handle: Handle,
    pool: Pool,
}

impl Service for Proxy {
    type Request = Message<http::Request, Body<http::Chunk, http::Error>>;
    type Response = Message<http::Response, Body<http::Chunk, http::Error>>;
    type Error = http::Error;
    type Future = Box<Future<Item=Self::Response, Error=Self::Error> + Send + 'static>;

    fn call(&self, message: Self::Request) -> Self::Future {

        let addr = self.pool.get().expect("Failed to get address from pool");

        debug!("Starting backend request to {:?}", addr);

        // This is a future to a framed transport. The call to pipline::connect below expects a
        // future to a socket.
        let framed = TcpStream::connect(&addr, &self.handle.clone()).map(|sock| {
            framed::ProxyFramed::new(sock, backend::HttpParser::new(), backend::HttpSerializer {})
        });

        let pipeline = pipeline::connect(framed, &self.handle.clone());
        pipeline.call(message).boxed()
    }

    fn poll_ready(&self) -> Async<()> {
        Async::Ready(())
    }
}

pub fn listen(addr: SocketAddr, pool: Pool) -> result::Result<(), http::Error> {

    let mut lp = Core::new().unwrap();
    let handle = lp.handle();

    let listener = TcpListener::bind(&addr, &handle.clone()).expect("Failed to bind to socket");
    info!("Listening on: {}", addr);

    let f = listener.incoming().for_each(|(sock, addr)| {
        debug!("Incoming connection on {}", addr);

        let service = Proxy{
            handle: handle.clone(),
            pool: pool.clone(),
        };
        let framed = framed::ProxyFramed::new(sock, frontend::HttpParser {}, frontend::HttpSerializer {});
        let pipeline = pipeline::Server::new(service, framed).map(|_| ()).map_err(|e| {
            error!("Pipeline error occurred: {:?}", e);
            ()
        });

        handle.spawn(pipeline);

        Ok(())
    });

    lp.run(f).expect("Unexpected error while proxying connection");

    Ok(())
}

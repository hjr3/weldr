use std::net::SocketAddr;
use std::result;

use futures::{Future, Async};
use futures::stream::Stream;
use tokio_service::Service;
use tokio_proto::{pipeline, Message, Body};
use tokio_core::reactor::Core;
use tokio_core::net::{TcpStream, TcpListener};
use tokio_proto::client::Client;

use http;
use framed;
use backend;
use frontend;
use pool::Pool;

pub struct Proxy {
    backend: Client<http::Request, http::Response, Body<http::Chunk, http::Error>, Body<http::Chunk, http::Error>, http::Error>
}

impl Service for Proxy {
    type Request = Message<http::Request, Body<http::Chunk, http::Error>>;
    type Response = Message<http::Response, Body<http::Chunk, http::Error>>;
    type Error = http::Error;
    type Future = Box<Future<Item=Self::Response, Error=Self::Error> + Send + 'static>;

    fn call(&self, message: Self::Request) -> Self::Future {
        self.backend.call(message).boxed()
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

        let addr = pool.get().expect("Failed to get address from pool");
        debug!("Preparing backend request to {:?}", addr);

        // This is a future to a framed transport. The call to pipline::connect below expects a
        // future to a socket.
        let framed = TcpStream::connect(&addr, &handle.clone()).map(|sock| {
            framed::ProxyFramed::new(sock, backend::HttpParser::new(), backend::HttpSerializer {})
        });

        let backend = pipeline::connect(framed, &handle.clone());

        let service = Proxy{
            backend: backend,
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

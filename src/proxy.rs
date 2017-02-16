use std::io;
use std::net::SocketAddr;
use std::result;
use std::str;
use std::thread;

use futures::{Future, Stream};
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;
use hyper::{self, Headers, Client, HttpVersion};
use hyper::client::{self, HttpConnector};
use hyper::client::Service;
use hyper::header;
use hyper::server::{self, Http};

use pool::{HttpPool, Pool};

struct Proxy {
    client: Client<HttpConnector>,
    pool: Pool,
}

impl Service for Proxy {
    type Request = server::Request;
    type Response = server::Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item=server::Response, Error=Self::Error>>;

    fn call(&self, req: server::Request) -> Self::Future {
        let http_pool = HttpPool::new(self.client.clone(), self.pool.clone());
        let backend = http_pool.call(req).and_then(|res| {
            ::futures::finished(res)
        }).map_err(|e| {
            error!("Error connecting to backend: {:?}", e);
            e
        });

        Box::new(backend)
    }
}

pub fn listen(addr: SocketAddr, pool: Pool) -> result::Result<thread::JoinHandle<()>, io::Error> {
    let handle = thread::spawn(move || {

        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let listener = TcpListener::bind(&addr, &handle).unwrap();
        info!("Listening on http://{}", &addr);

        let handle2 = handle.clone();
        let work = listener.incoming().for_each(move |(socket, addr)| {
            let client = Client::new(&handle2.clone());
            let service = Proxy {
                client: client,
                pool: pool.clone(),
            };

            let http = Http::new();
            http.bind_connection(&handle2, socket, addr, service);
            Ok(())
        });

        core.run(work).unwrap();
    });


    Ok(handle)
}

use std::io;
use std::net::SocketAddr;
use std::result;
use std::thread;

use futures::Future;
use hyper::{self, Client};
use hyper::server::{Server, Service, Request, Response, Listening};
use hyper::client;
use tokio_core::reactor::Handle;
use tokio_service::NewService;

use pool::Pool;

struct Proxy {
    handle: Handle,
    pool: Pool,
}

impl Service for Proxy {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item=Response, Error=Self::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let addr = self.pool.get().expect("Failed to get address from pool");
        let url = format!("http://{}{}", addr, req.path().unwrap());
        debug!("Preparing backend request to {:?}", url);

        let mut client_req = client::Request::new(req.method().clone(), url.parse().unwrap());

        // TOOD put this in a separate function. it should filter out headers we should not pass
        // we should also add headers like via
        {
            let mut client_headers = client_req.headers_mut();
            client_headers.extend(req.headers().iter());
        }

        // TODO store client in Proxy object
        let client = Client::new(&self.handle.clone());
        let backend = client.call(client_req).and_then(|res| {
            debug!("Response: {}", res.status());
            debug!("Headers: \n{}", res.headers());

            // transform client::Response into server::Response
            let mut server_response = Response::new().with_status(*res.status());

            {
                let mut server_headers = server_response.headers_mut();
                server_headers.extend(res.headers().iter());
            }

            server_response.set_body(res.body());

            ::futures::finished(server_response)
        }).map_err(|e| {
            error!("Error connecting to backend: {:?}", e);
            e
        });

        Box::new(backend)
    }
}

struct NewProxyService {
    handle: Handle,
    pool: Pool,
}

impl NewService for NewProxyService {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Instance = Proxy;

    fn new_service(&self) -> io::Result<Self::Instance> {
        Ok(Proxy {
            handle: self.handle.clone(),
            pool: self.pool.clone(),
        })
    }
}

pub fn listen(addr: SocketAddr, pool: Pool) -> result::Result<(thread::JoinHandle<()>, Listening), io::Error> {
    let (tx, rx) = ::std::sync::mpsc::channel();

    let handle = thread::spawn(move || {
        let (listening, server) = Server::standalone(|tokio_handle| {
            let new_service = NewProxyService {
                handle: tokio_handle.clone(),
                pool: pool.clone(),
            };

            Server::http(&addr, tokio_handle)?
                .handle(new_service, tokio_handle)
        }).unwrap();

        info!("Listening on http://{}", listening);

        tx.send(listening).unwrap();
        server.run();
    });

    Ok((handle, rx.recv().unwrap()))
}

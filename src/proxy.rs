use std::io;
use std::net::SocketAddr;
use std::result;

use futures::{Future, Stream};
use tokio_service::Service;
use tokio_core::net::{TcpListener};
use tokio_core::reactor::{Core};
use tokio_proto::{BindServer, TcpClient};
use tokio_proto::util::client_proxy::{ClientProxy, Response};

use backend;
use frontend;
use pool::Pool;

pub struct Proxy<R, S> {
    backend: ClientProxy<R, S, io::Error>,
}

impl<R, S> Service for Proxy<R, S> {
    type Request = R;
    type Response = S;
    type Error = io::Error;
    type Future = Response<S, io::Error>;

    fn call(&self, message: Self::Request) -> Self::Future {
        trace!("frontend service");
        self.backend.call(message)
    }
}

impl<R, S> Proxy<R, S> {
    pub fn new(backend: ClientProxy<R, S, io::Error>) -> Proxy<R, S> {
        Proxy {
            backend: backend,
        }
    }
}

pub fn listen(addr: SocketAddr, pool: Pool) -> result::Result<(), io::Error> {

    let mut lp = Core::new().unwrap();
    let handle = lp.handle();

    let listener = TcpListener::bind(&addr, &handle.clone()).expect("Failed to bind to socket");
    info!("Listening on: {}", addr);

    let f = listener.incoming().for_each(|(sock, addr)| {
        debug!("Incoming connection on {}", addr);

        let addr = pool.get().expect("Failed to get address from pool");
        debug!("Preparing backend request to {:?}", addr);

        // We attach the handle to Frontend to force the compiler to acknowledge that `backend`
        // will not outlive the handle.
        let frontend = frontend::Frontend { handle: handle.clone() };
        let client = TcpClient::new(backend::Backend);
        let f = client.connect(&addr, &handle.clone()).map(move |backend| {
            let service = Proxy::new(backend);
            frontend.bind_server(&frontend.handle, sock, service);
        }).map_err(|e| {
            error!("Error connecting to backend: {:?}", e);
            ()
        });

        handle.spawn(f);

        Ok(())
    });

    lp.run(f).expect("Unexpected error while proxying connection");

    Ok(())
}

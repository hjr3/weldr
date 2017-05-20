use std::net::SocketAddr;
use std::rc::Rc;
use std::cell::RefCell;
use std::str::FromStr;

use weldr_capnp::{publisher, subscriber, add_backend_server_request};

use futures::Future;

use capnp_rpc::{RpcSystem, twoparty, rpc_twoparty_capnp};
use capnp::capability::{Response, Promise};
use capnp::serialize;
use capnp::message::ReaderOptions;

use hyper::Uri;

use tokio_io::AsyncRead;
use tokio_core::reactor::Handle;
use tokio_core::net::TcpStream;

use server::Server;
use pool::Pool;

struct SubscriberImpl {
    pool: Pool,
}

impl SubscriberImpl {
    pub fn new(pool: Pool) -> SubscriberImpl {
        SubscriberImpl {
            pool: pool,
        }
    }
}

impl subscriber::Server<::capnp::data::Owned> for SubscriberImpl {
    fn push_message(&mut self,
                    params: subscriber::PushMessageParams<::capnp::data::Owned>,
                    _results: subscriber::PushMessageResults<::capnp::data::Owned>)
        -> Promise<(), ::capnp::Error>
        {
            let mut buf = pry!(pry!(params.get()).get_message());
            info!("raw message from publisher: {:?}", buf);

            let reader = serialize::read_message(&mut buf, ReaderOptions::new()).unwrap();
            let message = reader.get_root::<add_backend_server_request::Reader>().unwrap();
            let url = message.get_url().unwrap();
            info!("url from manager: {:?}", url);

            let backend = Uri::from_str(url).expect("Failed to parse server uri");
            let backend = Server::new(backend, true);
            self.pool.add(backend);

            Promise::ok(())
        }
}

pub struct S {
    pub response: Option<Response<publisher::subscribe_results::Owned<::capnp::data::Owned>>>,
}

pub fn subscribe(addr: SocketAddr, handle: Handle, pool: Pool) -> Rc<RefCell<S>> {
    let handle1 = handle.clone();

    let s = S { response: None };
    let s = Rc::new(RefCell::new(s));
    let s1 = s.clone();

    let request = TcpStream::connect(&addr, &handle).map_err(|e| e.into()).and_then(move |stream| {
        stream.set_nodelay(true).unwrap();
        let (reader, writer) = stream.split();

        let rpc_network =
            Box::new(twoparty::VatNetwork::new(reader, writer,
                                               rpc_twoparty_capnp::Side::Client,
                                               Default::default()));

        let mut rpc_system = RpcSystem::new(rpc_network, None);
        let publisher: publisher::Client<::capnp::data::Owned> =
            rpc_system.bootstrap(rpc_twoparty_capnp::Side::Server);

        let sub = subscriber::ToClient::new(SubscriberImpl::new(pool)).from_server::<::capnp_rpc::Server>();

        let mut request = publisher.subscribe_request();
        request.get().set_subscriber(sub);
        handle1.spawn(rpc_system.map_err(|e| {
            error!("Subscribe RPC System error {:?}", e);
        }));

        request.send().promise
    }).map_err(|e| {
        error!("Subscribe request error {:?}", e);
    }).and_then(move |r| {
        info!("Got a subscribe response");
        s1.borrow_mut().response = Some(r);
        ::futures::finished(())
    });

    handle.spawn(request);

    s
}

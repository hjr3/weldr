#[macro_use] extern crate futures;
#[macro_use] extern crate tokio_core;
#[macro_use] extern crate tokio_proto;
extern crate tokio_service;
extern crate tokio_timer;
extern crate bytes;
#[macro_use] extern crate nom;
#[macro_use] extern crate log;
extern crate env_logger;

// pub mod for now until the entire API is used internally
pub mod pool;
pub mod http;
mod framed;
mod proxy;
mod backend;
mod frontend;

use std::env;
use std::net::SocketAddr;

use pool::Pool;

fn main() {
    env_logger::init().unwrap();

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    let backend = env::args().nth(2).unwrap_or("127.0.0.1:12345".to_string());
    let pool = Pool::new(vec![backend]).unwrap();

    proxy::listen(addr, pool.clone()).expect("Failed to start server")
}

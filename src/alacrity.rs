extern crate env_logger;
extern crate alacrity;

use std::env;
use std::net::SocketAddr;

use alacrity::pool::Pool;

fn main() {
    env_logger::init().unwrap();

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    let backend = env::args().nth(2).unwrap_or("127.0.0.1:12345".to_string());
    let pool = Pool::new(vec![backend]).unwrap();

    alacrity::proxy::listen(addr, pool.clone()).expect("Failed to start server")
}

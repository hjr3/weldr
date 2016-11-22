extern crate env_logger;
extern crate alacrity;

use std::env;
use std::net::SocketAddr;
use std::thread;

use alacrity::pool::Pool;
use alacrity::mgmt;

fn main() {
    env_logger::init().unwrap();

    let addr = env::args().nth(1).unwrap_or("127.0.0.1:8080".to_string());
    let addr = addr.parse::<SocketAddr>().unwrap();

    let backend = env::args().nth(2).unwrap_or("127.0.0.1:12345".to_string());
    let backend = backend.parse::<SocketAddr>().unwrap();
    let pool = Pool::with_servers(vec![backend]);

    let admin_ip = env::args().nth(3).unwrap_or("127.0.0.1:8687".to_string());
    let admin_addr = admin_ip.parse::<SocketAddr>().unwrap();
    let p = pool.clone();
    let _ = thread::Builder::new().name("management".to_string()).spawn(move || {
        mgmt::listen(admin_addr, p);
    }).expect("Failed to create proxy thread");

    alacrity::proxy::listen(addr, pool.clone()).expect("Failed to start server");
}

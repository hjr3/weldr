#[macro_use]
extern crate log;
extern crate env_logger;
extern crate hyper;
extern crate weldr;
extern crate clap;
extern crate tokio_core;

use std::net::SocketAddr;

use clap::{Arg, App, SubCommand};

use tokio_core::reactor::Core;

use weldr::pool::Pool;
use weldr::config::Config;
use weldr::mgmt::{worker, manager};
use weldr::mgmt::health::BackendHealth;

fn main() {
    env_logger::init().expect("Failed to start logger");

    let matches = App::new("weldr")
        .arg(
            Arg::with_name("admin-ip")
                .long("admin-ip")
                .value_name("admin-ip")
                .takes_value(true)
                .help(
                    "admin ip and port used to issue commands to cluster. default: 0.0.0.0:8687",
                ),
        )
        .arg(
            Arg::with_name("ip")
                .long("ip")
                .value_name("ip")
                .takes_value(true)
                .help("listening ip and port for cluster. default: 0.0.0.0:8080"),
        )
        .subcommand(
            SubCommand::with_name("worker").about("start a worker").arg(
                Arg::with_name("id")
                    .long("id")
                    .value_name("id")
                    .takes_value(true)
                    .help("worker id assigned by the manager"),
            ),
        )
        .get_matches();


    let core = Core::new().unwrap();
    let handle = core.handle();

    // TODO make this dynamic and pass it down to workers
    let internal_addr = "127.0.0.1:4000";
    let internal_addr = internal_addr
        .parse::<SocketAddr>()
        .expect("Failed to parse addr");

    let ip = matches.value_of("worker").unwrap_or("0.0.0.0:8080");
    let ip = ip.parse::<SocketAddr>().unwrap();

    let pool = Pool::default();

    if let Some(matches) = matches.subcommand_matches("worker") {
        let id = matches.value_of("id").unwrap();
        debug!("Spawned worker {}", id);
        let _result = worker::subscribe(internal_addr, handle, pool.clone());

        weldr::proxy::run(ip, pool, core).expect("Failed to start server");
    } else {
        let conf = Config::default();
        let mut manager = manager::Manager::new();
        manager.listen(internal_addr, handle.clone());
        manager.start_workers(5).expect("Failed to start manager");

        let health = BackendHealth::new();

        let admin_ip = matches.value_of("worker").unwrap_or("0.0.0.0:8687");
        let admin_ip = admin_ip.parse::<SocketAddr>().unwrap();
        weldr::mgmt::run(admin_ip, pool, core, manager.clone(), &conf, health.clone())
            .expect("Failed to start server");
    }
}

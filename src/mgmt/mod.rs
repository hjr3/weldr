use std::io;
use std::net::SocketAddr;

use futures::Stream;
use tokio_core::reactor::{Core, Handle};
use tokio_core::net::{TcpStream, TcpListener};
use tokio_timer::Timer;
use hyper::server::Http;

use pool::Pool;
use self::api::Mgmt;
use self::manager::Manager;
use self::health::BackendHealth;
use config::Config;

pub mod api;
pub mod health;
pub mod manager;
pub mod worker;

/// Run manager server and start health check timer
pub fn run(sock: SocketAddr,
           pool: Pool,
           mut core: Core,
           manager: Manager,
           config: &Config,
           health: BackendHealth)
           -> io::Result<()> {
    let handle = core.handle();
    let listener = TcpListener::bind(&sock, &handle)?;
    let timer = Timer::default();
    let health_timer = timer
        .interval(config.health_check.interval)
        .map(|_| None)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e));

    let admin_addr = listener.local_addr()?;
    let listener = listener
        .incoming()
        .map(|stream| Some(stream))
        .select(health_timer);
    let srv = listener.for_each(move |stream| {

        // first stream is the management ip
        // second stream is health interval
        match stream {
            Some((socket, addr)) => {
                mgmt(socket, addr, pool.clone(), &handle, manager.clone());
            }
            None => {
                info!("health check");
                health::run(pool.clone(),
                            &handle,
                            &config,
                            manager.clone(),
                            health.clone());
            }
        }

        Ok(())
    });

    info!("Listening on http://{}", &admin_addr);
    core.run(srv)
}

fn mgmt(socket: TcpStream, addr: SocketAddr, pool: Pool, handle: &Handle, manager: Manager) {
    let service = Mgmt::new(pool, handle.clone(), manager);
    let http = Http::new();
    http.bind_connection(&handle, socket, addr, service);
}

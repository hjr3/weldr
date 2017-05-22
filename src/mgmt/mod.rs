use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use futures::Stream;
use futures::stream::MergedItem;
use tokio_core::reactor::{Core, Handle};
use tokio_core::net::{TcpStream, TcpListener};
use tokio_timer::Timer;
use hyper::server:: Http;

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
pub fn run(sock: SocketAddr, pool: Pool, mut core: Core, manager: Manager, conf: &Config, health: BackendHealth) -> io::Result<()> {
    let handle = core.handle();
    let listener = TcpListener::bind(&sock, &handle)?;
    let timer = Timer::default();
    let health_timer = timer.interval(Duration::from_secs(conf.health_check.interval)).map_err(|e| {
        io::Error::new(io::ErrorKind::Other, e)
    });

    let admin_addr = listener.local_addr()?;
    let listener = listener.incoming().merge(health_timer);
    let srv = listener.for_each(move |stream| {

        // first stream is the management ip
        // second stream is health interval
        match stream {
            MergedItem::First((socket, addr)) => {
                mgmt(socket, addr, pool.clone(), &handle, manager.clone());
            }
            MergedItem::Second(()) => {
                health::run(pool.clone(), &handle, &conf, manager.clone(), health.clone());
            }
            MergedItem::Both((socket, addr), ()) => {
                mgmt(socket, addr, pool.clone(), &handle, manager.clone());
                info!("health check");
                health::run(pool.clone(), &handle, &conf, manager.clone(), health.clone());
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

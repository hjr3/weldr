use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use futures::{future, Future, Stream};
use futures::stream::MergedItem;
use tokio_core::reactor::{Core, Handle};
use tokio_core::net::{TcpStream, TcpListener};
use tokio_timer::Timer;
use hyper;
use hyper::server:: Http;

use pool::Pool;
use self::api::Mgmt;
use self::manager::Manager;
use config::Config;

pub mod api;
pub mod health;
pub mod manager;
pub mod worker;

/// Run server with default Core
pub fn run(admin_addr: SocketAddr, pool: Pool, core: Core, manager: Manager, conf: &Config) -> io::Result<()> {
    let handle = core.handle();

    let admin_listener = TcpListener::bind(&admin_addr, &handle)?;
    run_with(core, admin_listener, pool, future::empty(), manager, conf)
}

/// Run server with specified Core, TcpListener, Pool
///
/// This is useful for integration testing where the port is set to 0 and the test code needs to
/// determine the local addr.
pub fn run_with<F>(mut core: Core, listener: TcpListener, pool: Pool, shutdown_signal: F, manager: Manager, conf: &Config) -> io::Result<()>
    where F: Future<Item = (), Error = hyper::Error>,
{
    let handle = core.handle();

    let timer = Timer::default();

    // FIXME configure health check timer
    let health_timer = timer.interval(Duration::from_secs(conf.health.timeout)).map_err(|e| {
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
                health::run(pool.clone(), &handle, &conf);
            }
            MergedItem::Both((socket, addr), ()) => {
                mgmt(socket, addr, pool.clone(), &handle, manager.clone());
                info!("health check");
                health::run(pool.clone(), &handle, &conf);
            }
        }

        Ok(())
    });

    info!("Listening on http://{}", &admin_addr);
    match core.run(shutdown_signal.select(srv.map_err(|e| e.into()))) {
        Ok(((), _incoming)) => Ok(()),
        Err((e, _other)) => return Err(io::Error::new(io::ErrorKind::Other, e)),
    }
}

fn mgmt(socket: TcpStream, addr: SocketAddr, pool: Pool, handle: &Handle, manager: Manager) {
    let service = Mgmt::new(pool, handle.clone(), manager);
    let http = Http::new();
    http.bind_connection(&handle, socket, addr, service);
}

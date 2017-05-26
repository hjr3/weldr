use std::str::FromStr;
use std::cell::RefCell;
use std::rc::Rc;
use std::collections::HashMap;

use futures::Future;
use tokio_core::reactor::Handle;
use hyper::{Client, Uri};
use hyper_tls::HttpsConnector;

use pool::{Pool, Backend};
use config::Config;
use mgmt::Manager;

#[derive(Clone, Debug)]
pub struct BackendHealth {
    inner: Rc<RefCell<Inner>>,
}

impl BackendHealth {
    pub fn new() -> BackendHealth {
        BackendHealth {
            inner: Rc::new(RefCell::new(Inner {
                pending_active: HashMap::new(),
                pending_down: HashMap::new(),
            }))
        }
    }
}

#[derive(Debug)]
struct Inner {
    pending_active: HashMap<Backend, u64>,
    pending_down: HashMap<Backend, u64>,
}

pub fn run(pool: Pool, handle: &Handle, conf: &Config, manager: Manager, health: BackendHealth) {
    let client = Client::configure()
        .connector(HttpsConnector::new(4, &handle))
        .build(&handle);

    let backends = pool.all();
    let handle1 = handle.clone();
    for backend in backends {
        let manager = manager.clone();
        let handle1 = handle1.clone();
        let server = backend.server();
        let health = health.clone();
        let url = format!("{}{}", server.url(), &conf.health_check.uri_path);
        let url = match Uri::from_str(&url) {
            Ok(url) => url,
            Err(e) => {
                error!("Invalid health check url: {:?}",e);
                if backend.is_active() {
                    let ref mut pending_down = health.inner.borrow_mut().pending_down;
                    let failures = pending_down.entry(backend.clone()).or_insert(0);
                    *failures += 1;

                    if *failures >= conf.health_check.failures {
                        info!("Disabling {:?} in pool", backend);
                        backend.mark_down();
                        let uri = backend.server().url();
                        manager.publish_server_state_down(&uri, handle.clone());
                    }
                }
                continue;
            }
        };

        let allowed_failures = conf.health_check.failures;
        debug!("Health check {:?}", url);
        let req = client.get(url).then(move |res| {
            match res {
                Ok(res) => {
                    debug!("Response: {}", res.status());
                    debug!("Headers: \n{}", res.headers());

                    if res.status().is_success() {
                        if backend.is_down() {
                            let ref mut pending_down = health.inner.borrow_mut().pending_down;
                            let failures = pending_down.entry(backend.clone()).or_insert(0);
                            *failures += 1;

                            if *failures >= allowed_failures {
                                info!("Enabling {:?} in pool", backend);
                                backend.mark_active();
                                let uri = backend.server().url();
                                manager.publish_server_state_active(&uri, handle1.clone());
                            }
                        }
                    } else {
                        if backend.is_active() {
                            let ref mut pending_down = health.inner.borrow_mut().pending_down;
                            let failures = pending_down.entry(backend.clone()).or_insert(0);
                            *failures += 1;

                            if *failures >= allowed_failures {
                                info!("Disabling {:?} in pool", backend);
                                backend.mark_down();
                                let uri = backend.server().url();
                                manager.publish_server_state_down(&uri, handle1.clone());
                            }
                        }
                    }
                    ::futures::finished(())
                }
                Err(e) => {
                    error!("Error connecting to backend: {:?}", e);
                    let ref mut pending_down = health.inner.borrow_mut().pending_down;
                    let failures = pending_down.entry(backend.clone()).or_insert(0);
                    *failures += 1;

                    if *failures >= allowed_failures {
                        info!("Disabling {:?} in pool", backend);
                        backend.mark_down();
                        let uri = backend.server().url();
                        manager.publish_server_state_down(&uri, handle1.clone());
                    }
                    ::futures::finished(())
                }
            }
        });

        handle.spawn(req);
    }
}

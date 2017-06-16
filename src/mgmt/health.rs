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

#[derive(Debug, Clone, Copy)]
enum HealthState {
    Passing(u64),
    Failing(u64),
}

#[derive(Clone, Debug)]
pub struct BackendHealth {
    inner: Rc<RefCell<Inner>>,
}

#[derive(Debug)]
struct Inner {
    health_state: HashMap<Backend, HealthState>,
}

impl BackendHealth {
    pub fn new() -> BackendHealth {
        BackendHealth { inner: Rc::new(RefCell::new(Inner { health_state: HashMap::new() })) }
    }

    pub fn should_mark_active(&self, backend: Backend, required_passes: u64) -> bool {
        let ref mut health_state = self.inner.borrow_mut().health_state;
        let state = health_state
            .entry(backend.clone())
            .or_insert(HealthState::Passing(0));

        // rules
        // - if active and passing, then do nothing
        // - if active and failing, then we had a passed health check so reset
        // - if down and passing, then check if we can change state or update consec passes
        // - if down and failing, then increment concsec passing

        if backend.is_active() {
            if let &mut HealthState::Failing(_) = state {
                *state = HealthState::Passing(0);
            }
        } else {
            match state {
                &mut HealthState::Passing(mut consec_passes) => {
                    consec_passes += 1;
                    if consec_passes >= required_passes {
                        *state = HealthState::Passing(0);
                        return true;
                    } else {
                        *state = HealthState::Passing(consec_passes);
                    }
                }
                &mut HealthState::Failing(_) => {
                    *state = HealthState::Passing(1);
                }
            }
        }

        false
    }

    pub fn should_mark_down(&self, backend: Backend, required_failures: u64) -> bool {
        let ref mut health_state = self.inner.borrow_mut().health_state;
        let state = health_state
            .entry(backend.clone())
            .or_insert(HealthState::Passing(0));

        // rules
        // - if down and failing, then do nothing
        // - if down and passing, then we had a failed health check so reset
        // - if active and failing, then check if we can change state or update consec failures
        // - if active and passing, then increment consec failing

        if backend.is_down() {
            if let &mut HealthState::Passing(_) = state {
                *state = HealthState::Failing(0);
            }
        } else {
            match state {
                &mut HealthState::Failing(mut consec_failures) => {
                    consec_failures += 1;
                    if consec_failures >= required_failures {
                        *state = HealthState::Failing(0);
                        return true;
                    } else {
                        *state = HealthState::Failing(consec_failures);
                    }
                }
                &mut HealthState::Passing(_) => {
                    *state = HealthState::Failing(1);
                }
            }
        }

        false
    }
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
                error!("Invalid health check url: {:?}", e);
                if backend.is_active() {
                    info!("Disabling {:?} in pool", backend);
                    backend.mark_down();
                    let uri = backend.server().url();
                    manager.publish_server_state_down(&uri, handle1.clone());
                }
                continue;
            }
        };

        let allowed_failures = conf.health_check.failures;
        let allowed_successes = conf.health_check.passes;
        debug!("Health check {:?}", url);
        let req = client.get(url).then(move |res| match res {
            Ok(res) => {
                debug!("Response: {}", res.status());
                debug!("Headers: \n{}", res.headers());

                if res.status().is_success() {
                    if health.should_mark_active(backend.clone(), allowed_successes) {
                        info!("Enabling {:?} in pool", backend);
                        backend.mark_active();
                        let uri = backend.server().url();
                        manager.publish_server_state_active(&uri, handle1.clone());
                    }
                } else {
                    if health.should_mark_down(backend.clone(), allowed_failures) {
                        info!("Disabling {:?} in pool", backend);
                        backend.mark_down();
                        let uri = backend.server().url();
                        manager.publish_server_state_down(&uri, handle1.clone());
                    }
                }
                ::futures::finished(())
            }
            Err(e) => {
                error!("Error connecting to backend: {:?}", e);
                if health.should_mark_down(backend.clone(), allowed_failures) {
                    info!("Disabling {:?} in pool", backend);
                    backend.mark_down();
                    let uri = backend.server().url();
                    manager.publish_server_state_down(&uri, handle1.clone());
                }
                ::futures::finished(())
            }
        });

        handle.spawn(req);
    }
}

#[cfg(test)]
mod tests {
    use super::BackendHealth;
    use pool::Backend;
    use server::Server;
    use std::str::FromStr;

    fn backend() -> Backend {
        Backend::new(Server::new(
            FromStr::from_str(
                "http://127.0.0.1:6000
                ",
            ).unwrap(),
            false,
        ))
    }

    #[test]
    fn test_should_mark_active() {
        let backend = backend();
        let passes = 2;

        let health = BackendHealth::new();
        backend.mark_down();
        assert_eq!(false, health.should_mark_active(backend.clone(), passes));
        assert_eq!(true, health.should_mark_active(backend.clone(), passes));
    }

    #[test]
    fn test_should_mark_down() {
        let backend = backend();
        let failures = 2;

        let health = BackendHealth::new();
        assert_eq!(false, health.should_mark_down(backend.clone(), failures));
        assert_eq!(true, health.should_mark_down(backend.clone(), failures));
    }

    #[test]
    fn test_backend_flapping() {
        let backend = backend();
        let passes = 3;
        let failures = 2;

        let health = BackendHealth::new();
        assert_eq!(false, health.should_mark_down(backend.clone(), failures));
        assert_eq!(false, health.should_mark_active(backend.clone(), passes));
        // should not be marked down here because the failures need to be consecutive
        assert_eq!(false, health.should_mark_down(backend.clone(), failures));
        assert_eq!(true, health.should_mark_down(backend.clone(), failures));
    }
}

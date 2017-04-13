use std::io;
use std::rc::Rc;
use std::cell::RefCell;

use futures::Future;

use hyper::{self, server};

use server::Server;
use stats::Stats;

/// A round-robin pool for servers
///
/// A simple pool that stores socket addresses and, for now, clones them out.
///
/// Inspired by https://github.com/NicolasLM/nucleon/blob/master/src/backend.rs
#[derive(Clone, Debug, Default)]
pub struct Pool {
    inner: Rc<RefCell<InnerPool>>,
}

impl Pool {
    /// Send a request to the pool
    ///
    /// The pool may be exhausted of eligible addresses to connect to and will return an error.
    pub fn request<F>(&self, f: F) -> Box<Future<Item=server::Response, Error=hyper::Error>>
        where F: FnOnce(&Server) -> Box<Future<Item=server::Response, Error=hyper::Error>>
    {
        match self.inner.borrow_mut().get() {
            Some(backend) => {
                let backend1 = backend.clone();
                Box::new(f(&backend.borrow().server).then(move |res| {
                    match res {
                        Ok(res) => {
                            if res.status().is_server_error() {
                                backend1.borrow_mut().stats_mut().inc_failure();
                            } else {
                                backend1.borrow_mut().stats_mut().inc_success();
                            }
                            ::futures::finished(res)
                        }
                        Err(e) => {
                            backend1.borrow_mut().stats_mut().inc_failure();
                            ::futures::failed(e)
                        }
                    }
                }))
            }
            None => {
                let e = io::Error::new(io::ErrorKind::Other, "Pool is exhausted of servers");
                // TODO should this return a Bad Gateway error ?
                Box::new(::futures::failed(hyper::Error::Io(e)))
            }
        }
    }

    /// Returns all `Server` from the pool
    pub fn all(&self) -> Vec<Server> {
        self.inner.borrow_mut().all()
    }

    /// Add a new server to the pool
    ///
    /// If the server is already added, then it cannot be added again. The function will return
    /// false if the server already exists.
    pub fn add(&self, server: Server) -> bool {
        self.inner.borrow_mut().add(Backend::new(server))
    }

    /// Remove a server in the pool
    ///
    pub fn remove(&self, backend: &Server) {
        self.inner.borrow_mut().remove(backend)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ServerState {
    Active,
    //Down,
    //Disabled,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Backend {
    server: Server,
    state: ServerState,
    stats: Stats,
}

impl Backend {
    pub fn new(server: Server) -> Backend {
        Backend {
            server: server,
            state: ServerState::Active,
            stats: Stats::new(),
        }
    }

    pub fn stats_mut(&mut self) -> &mut Stats {
        &mut self.stats
    }
}

#[derive(Debug, Default)]
pub struct InnerPool {
    backends: Vec<Rc<RefCell<Backend>>>,
    last_used: usize,
}

impl InnerPool {

    // this is only used in test code
    #[cfg(test)]
    fn new(backends: Vec<Backend>) -> InnerPool {
        InnerPool {
            backends: backends.into_iter().map(|b| Rc::new(RefCell::new(b))).collect(),
            last_used: 0,
        }
    }

    fn get(&mut self) -> Option<Rc<RefCell<Backend>>> {
        if self.backends.is_empty() {
            warn!("Pool is exhausted of socket addresses");
            return None;
        }
        self.last_used = (self.last_used + 1) % self.backends.len();
        self.backends.get(self.last_used).map(|backend| {
            debug!("Pool is cloaning (hehe) out {:?}", backend);
            // TODO make this Rc ?
            backend.clone()
        })
    }

    fn all(&mut self) -> Vec<Server> {
        if self.backends.is_empty() {
            warn!("Pool is exhausted of socket addresses");
            return Vec::new();
        }

        // TODO fix this to not need to clone the server. We can probably just pass the
        // entire backend to the mgmt API and use the server value as a reference.
        self.backends.iter().map(|backend| backend.borrow().server.clone()).collect()
    }

    fn add(&mut self, backend: Backend) -> bool {
        let backend = Rc::new(RefCell::new(backend));
        if self.backends.contains(&backend) {
            return false;
        }

        self.backends.push(backend);
        true
    }

    fn remove(&mut self, server: &Server) {
        self.backends.retain(|b| &b.borrow().server != server);
    }
}

#[cfg(test)]
mod tests {
    use super::{Backend, InnerPool};
    use server::Server;
    use std::str::FromStr;

    #[test]
    fn test_rrb_backend() {
        let backends: Vec<Backend> = vec![
            Backend::new(Server::new(FromStr::from_str("http://127.0.0.1:6000").unwrap(), false)),
            Backend::new(Server::new(FromStr::from_str("http://127.0.0.1:6001").unwrap(), false)),
        ];

        let mut rrb = InnerPool::new(backends);
        assert_eq!(2, rrb.backends.len());

        let first = rrb.get().unwrap();
        let second = rrb.get().unwrap();
        let third = rrb.get().unwrap();
        let fourth = rrb.get().unwrap();
        assert_eq!(first, third);
        assert_eq!(second, fourth);
        assert!(first != second);
    }

    #[test]
    fn test_empty_rrb_backend() {
        let backends= vec![];
        let mut rrb = InnerPool::new(backends);
        assert_eq!(0, rrb.backends.len());
        assert!(rrb.get().is_none());
        assert!(rrb.all().is_empty());
    }

    #[test]
    fn test_add_to_rrb_backend() {
        let mut rrb = InnerPool::new(vec![]);
        assert!(rrb.get().is_none());
        let server = Server::new(FromStr::from_str("http://127.0.0.1:6000").unwrap(), false);
        rrb.add(Backend::new(server.clone()));
        assert!(rrb.get().is_some());
        assert_eq!(vec![server], rrb.all());
    }

    #[test]
    fn test_remove_from_rrb_backend() {
        let mut rrb = InnerPool::new(vec![]);
        let server1 = Server::new(FromStr::from_str("http://127.0.0.1:6000").unwrap(), false);
        let server2 = Server::new(FromStr::from_str("http://127.0.0.1:6001").unwrap(), false);
        rrb.add(Backend::new(server1.clone()));
        rrb.add(Backend::new(server2.clone()));
        assert_eq!(2, rrb.backends.len());
        assert_eq!(vec![server1.clone(), server2.clone()], rrb.all());

        let unknown_server = Server::new(FromStr::from_str("http://127.0.0.1:1234").unwrap(), false);
        rrb.remove(&unknown_server);
        assert_eq!(2, rrb.backends.len());
        assert_eq!(vec![server1.clone(), server2.clone()], rrb.all());

        rrb.remove(&server1);
        assert_eq!(1, rrb.backends.len());
        assert_eq!(vec![server2.clone()], rrb.all());

        rrb.remove(&server2);
        assert_eq!(0, rrb.backends.len());
        assert!(rrb.all().is_empty());
    }
}

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
// TODO can probably get rid of the Rc<RefCell<_>> part
#[derive(Clone, Debug, Default)]
pub struct Pool {
    inner: Rc<RefCell<InnerPool>>,
}

impl Pool {
    /// Send a request to the pool
    ///
    /// The pool may be exhausted of eligible addresses to connect to and will return an error.
    pub fn request<F>(&self, f: F) -> Box<Future<Item = server::Response, Error = hyper::Error>>
    where
        F: FnOnce(&Server) -> Box<Future<Item = server::Response, Error = hyper::Error>>,
    {
        match self.inner.borrow_mut().get() {
            Some(backend) => {
                Box::new(f(&backend.server()).then(move |res| match res {
                    Ok(res) => {
                        if res.status().is_server_error() {
                            backend.inc_failure();
                        } else {
                            backend.inc_success();
                        }
                        ::futures::finished(res)
                    }
                    Err(e) => {
                        backend.inc_failure();
                        ::futures::failed(e)
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

    /// Returns all `Backend` from the pool
    pub fn all(&self) -> Vec<Backend> {
        self.inner.borrow().all()
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
    pub fn remove(&self, server: &Server) {
        self.inner.borrow_mut().remove(server)
    }

    pub fn find(&self, server: &Server) -> Option<Backend> {
        self.inner.borrow().find(server)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum ServerState {
    Active,
    Down,
    //Disabled,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Backend {
    inner: Rc<RefCell<InnerBackend>>,
}

use std::hash::{Hash, Hasher};

impl Hash for Backend {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner.borrow().hash(state);
    }
}

#[derive(Debug, Eq, PartialEq, Hash)]
struct InnerBackend {
    server: Server,
    state: ServerState,
    stats: Stats,
}

impl Backend {
    pub fn new(server: Server) -> Backend {
        Backend {
            inner: Rc::new(RefCell::new(InnerBackend {
                server: server,
                state: ServerState::Active,
                stats: Stats::new(),
            })),
        }
    }

    pub fn inc_success(&self) {
        self.inner.borrow_mut().stats.inc_success()
    }

    pub fn inc_failure(&self) {
        self.inner.borrow_mut().stats.inc_failure()
    }

    pub fn server(&self) -> Server {
        self.inner.borrow().server.clone()
    }

    pub fn is_active(&self) -> bool {
        self.inner.borrow().state == ServerState::Active
    }

    pub fn is_down(&self) -> bool {
        self.inner.borrow().state == ServerState::Down
    }

    pub fn mark_active(&self) {
        self.inner.borrow_mut().state = ServerState::Active;
    }

    pub fn mark_down(&self) {
        self.inner.borrow_mut().state = ServerState::Down;
    }
}

#[derive(Debug, Default)]
pub struct InnerPool {
    backends: Vec<Backend>,
    last_used: usize,
}

impl InnerPool {
    // this is only used in test code
    #[cfg(test)]
    fn new(backends: Vec<Backend>) -> InnerPool {
        InnerPool {
            backends: backends.into_iter().map(|b| b).collect(),
            last_used: 0,
        }
    }

    fn get(&mut self) -> Option<Backend> {
        if self.backends.is_empty() {
            warn!("Pool is empty of backends");
            return None;
        }

        let start = self.last_used;
        loop {
            self.last_used = (self.last_used + 1) % self.backends.len();
            let backend = match self.backends.get(self.last_used) {
                Some(b) => b,
                None => return None,
            };

            if backend.is_down() {
                if start == self.last_used {
                    warn!("Pool has no active backends");
                    return None;
                }
                continue;
            }

            debug!("Pool is cloaning (hehe) out {:?}", backend);
            return Some(backend.clone());
        }
    }

    fn all(&self) -> Vec<Backend> {
        //if self.backends.is_empty() {
        //    warn!("Pool is exhausted of backends");
        //    return Vec::new();
        //}

        //self.backends.iter().map(|backend| backend.clone()).collect()
        self.backends.clone()
    }

    fn add(&mut self, backend: Backend) -> bool {
        let backend = backend;
        if self.backends.contains(&backend) {
            return false;
        }

        self.backends.push(backend);
        true
    }

    fn remove(&mut self, server: &Server) {
        self.backends.retain(|b| &b.server() != server);
    }

    fn find(&self, server: &Server) -> Option<Backend> {
        match self.backends.iter().find(|b| &b.server() == server) {
            Some(backend) => Some(backend.clone()),
            None => None,
        }
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
            Backend::new(Server::new(
                FromStr::from_str("http://127.0.0.1:6000").unwrap(),
                false,
            )),
            Backend::new(Server::new(
                FromStr::from_str("http://127.0.0.1:6001").unwrap(),
                false,
            )),
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
        let backends = vec![];
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
        let backend = Backend::new(server.clone());
        rrb.add(backend);
        let b1 = Backend::new(server.clone());
        assert!(rrb.get().is_some());
        assert_eq!(vec![b1], rrb.all());
    }

    #[test]
    fn test_remove_from_rrb_backend() {
        let mut rrb = InnerPool::new(vec![]);
        let server1 = Server::new(FromStr::from_str("http://127.0.0.1:6000").unwrap(), false);
        let server2 = Server::new(FromStr::from_str("http://127.0.0.1:6001").unwrap(), false);
        rrb.add(Backend::new(server1.clone()));
        rrb.add(Backend::new(server2.clone()));
        assert_eq!(2, rrb.backends.len());
        let b1 = Backend::new(server1.clone());
        let b2 = Backend::new(server2.clone());
        assert_eq!(vec![b1.clone(), b2.clone()], rrb.all());

        let unknown_server =
            Server::new(FromStr::from_str("http://127.0.0.1:1234").unwrap(), false);
        rrb.remove(&unknown_server);
        assert_eq!(2, rrb.backends.len());
        assert_eq!(vec![b1.clone(), b2.clone()], rrb.all());

        rrb.remove(&server1);
        assert_eq!(1, rrb.backends.len());
        assert_eq!(vec![b2.clone()], rrb.all());

        rrb.remove(&server2);
        assert_eq!(0, rrb.backends.len());
        assert!(rrb.all().is_empty());
    }
}

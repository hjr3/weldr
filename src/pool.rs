use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

/// A round-robin pool for servers
///
/// A simple pool that stores socket addresses and, for now, clones them out.
///
/// Inspired by https://github.com/NicolasLM/nucleon/blob/master/src/backend.rs
#[derive(Clone)]
pub struct Pool {
    inner: Arc<RwLock<inner::Pool>>,
}

impl Pool {
    pub fn with_servers(backends: Vec<SocketAddr>) -> Pool {
        let inner = inner::Pool::new(backends);

        Pool {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    /// Get a `SocketAddr` from the pool
    ///
    /// The pool may be exhausted of eligible addresses to connect to. The client is expected to
    /// handle this scenario.
    pub fn get(&self) -> Option<SocketAddr> {
        self.inner.write().expect("Lock is poisoned").get()
    }

    /// Returns all `SocketAddr` from the pool
    pub fn all(&self) -> Vec<SocketAddr> {
        self.inner.write().expect("Lock is poisoned").all()
    }

    /// Add a new socket (IP address and port) to the pool
    ///
    /// Currently, it is possible to add the same socket more then once
    pub fn add(&self, backend: SocketAddr) {
        self.inner.write().expect("Lock is poisoned").add(backend)
    }

    /// Remove a socket from the pool
    ///
    /// This will remove all instance of the given socket. See `add` method for details on
    /// duplicate sockets.
    pub fn remove(&self, backend: &SocketAddr) {
        self.inner.write().expect("Lock is poisoned").remove(backend)
    }
}

pub mod inner {
    use std::net::SocketAddr;

    pub struct Pool {
        backends: Vec<SocketAddr>,
        last_used: usize,
    }

    impl Pool {
        pub fn new(backends: Vec<SocketAddr>) -> Pool {
            Pool {
                backends: backends,
                last_used: 0,
            }
        }

        pub fn get(&mut self) -> Option<SocketAddr> {
            if self.backends.is_empty() {
                warn!("Pool is exhausted of socket addresses");
                return None;
            }
            self.last_used = (self.last_used + 1) % self.backends.len();
            self.backends.get(self.last_used).map(|&addr| {
                debug!("Pool is cloaning (hehe) out {}", addr);
                addr.clone()
            })
        }

        pub fn all(&mut self) -> Vec<SocketAddr> {
            if self.backends.is_empty() {
                warn!("Pool is exhausted of socket addresses");
                return Vec::new();
            }
            self.backends.clone()
        }

        pub fn add(&mut self, backend: SocketAddr) {
            self.backends.push(backend);
        }

        pub fn remove(&mut self, backend: &SocketAddr) {
            self.backends.retain(|&addr| &addr != backend);
        }
    }


    #[cfg(test)]
    mod tests {
        use super::Pool;
        use std::net::SocketAddr;
        use std::str::FromStr;

        #[test]
        fn test_rrb_backend() {
            let backends: Vec<SocketAddr> = vec![
                FromStr::from_str("127.0.0.1:6000").unwrap(),
                FromStr::from_str("127.0.0.1:6001").unwrap(),
            ];

            let mut rrb = Pool::new(backends);
            assert_eq!(2, rrb.backends.len());

            let first_socket_addr = rrb.get().unwrap();
            let second_socket_addr = rrb.get().unwrap();
            let third_socket_addr = rrb.get().unwrap();
            let fourth_socket_addr = rrb.get().unwrap();
            assert_eq!(first_socket_addr, third_socket_addr);
            assert_eq!(second_socket_addr, fourth_socket_addr);
            assert!(first_socket_addr != second_socket_addr);
        }

        #[test]
        fn test_empty_rrb_backend() {
            let backends= vec![];
            let mut rrb = Pool::new(backends);
            assert_eq!(0, rrb.backends.len());
            assert!(rrb.get().is_none());
            assert!(rrb.all().is_empty());
        }

        #[test]
        fn test_add_to_rrb_backend() {
            let mut rrb = Pool::new(vec![]);
            assert!(rrb.get().is_none());
            let addr: SocketAddr = FromStr::from_str("127.0.0.1:6000").unwrap();
            rrb.add(addr);
            assert!(rrb.get().is_some());
            assert_eq!(vec![addr], rrb.all());
        }

        #[test]
        fn test_remove_from_rrb_backend() {
            let mut rrb = Pool::new(vec![]);
            let addr1: SocketAddr = FromStr::from_str("127.0.0.1:6000").unwrap();
            let addr2: SocketAddr = FromStr::from_str("127.0.0.1:6001").unwrap();
            rrb.add(addr1);
            rrb.add(addr2);
            assert_eq!(2, rrb.backends.len());
            assert_eq!(vec![addr1, addr2], rrb.all());

            let unknown_addr = FromStr::from_str("127.0.0.1:1234").unwrap();
            rrb.remove(&unknown_addr);
            assert_eq!(2, rrb.backends.len());
            assert_eq!(vec![addr1, addr2], rrb.all());

            rrb.remove(&addr1);
            assert_eq!(1, rrb.backends.len());
            assert_eq!(vec![addr2], rrb.all());

            rrb.remove(&addr2);
            assert_eq!(0, rrb.backends.len());
            assert!(rrb.all().is_empty());
        }
    }
}

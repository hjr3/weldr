use std::cell::RefCell;
use std::net::{SocketAddr, AddrParseError};
use std::rc::Rc;

/// A round-robin pool for servers
///
/// A simple pool that stores socket addresses and, for now, clones them out.
///
/// Inspired by https://github.com/NicolasLM/nucleon/blob/master/src/backend.rs
#[derive(Clone)]
pub struct Pool {
    inner: Rc<RefCell<inner::Pool>>,
}

impl Pool {
    pub fn new(backends_str: Vec<String>) -> Result<Pool, AddrParseError> {
        let inner = try!(inner::Pool::new(backends_str));

        Ok(
            Pool {
                inner: Rc::new(RefCell::new(inner)),
            }
          )
    }


    /// Get a `SocketAddr` from the pool
    ///
    /// The pool may be exhausted of eligible addresses to connect to. The client is expected to
    /// handle this scenario.
    pub fn get(&self) -> Option<SocketAddr> {
        self.inner.borrow_mut().get()
    }
}

pub mod inner {
    use std::net::{SocketAddr, AddrParseError};
    use std::str::FromStr;

    pub struct Pool {
        backends: Vec<SocketAddr>,
        last_used: usize,
    }

    impl Pool {
        pub fn new(backends_str: Vec<String>) -> Result<Pool, AddrParseError> {
            let mut backends = Vec::new();

            for backend_str in backends_str {
                let backend_socket_addr: SocketAddr = try!(FromStr::from_str(&backend_str));
                backends.push(backend_socket_addr);
                info!("Load balancing server {:?}", backend_socket_addr);
            }

            Ok(Pool {
                backends: backends,
                last_used: 0,
            })
        }
    }

    impl Pool {

        pub fn get(&mut self) -> Option<SocketAddr> {
            if self.backends.is_empty() {
                warn!("Pool is exhausted of socket addresses");
                return None;
            }
            self.last_used = (self.last_used + 1) % self.backends.len();
            self.backends.get(self.last_used).map(|b| {
                debug!("Pool is cloaning (hehe) out {}", b);
                b.clone()
            })
        }

        /// Add a new socket (IP address and port) to the pool
        ///
        /// Currently, it is possible to add the same socket more then once
        pub fn add(&mut self, backend_str: &str) -> Result<(), AddrParseError> {
            let backend_socket_addr: SocketAddr = try!(FromStr::from_str(&backend_str));
            self.backends.push(backend_socket_addr);
            Ok(())
        }

        /// Remove a socket from the pool
        ///
        /// This will remove all instance of the given socket. See `add` method for details on
        /// duplicate sockets.
        pub fn remove(&mut self, backend_str: &str) -> Result<(), AddrParseError> {
            let backend_socket_addr: SocketAddr = try!(FromStr::from_str(&backend_str));
            self.backends.retain(|&x| x != backend_socket_addr);
            Ok(())
        }
    }


#[cfg(test)]
    mod tests {
        use super::Pool;

        #[test]
        fn test_rrb_backend() {
            let backends_str = vec!["127.0.0.1:6000".to_string(), "127.0.0.1:6001".to_string()];
            let mut rrb = Pool::new(backends_str).unwrap();
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
            let backends_str = vec![];
            let mut rrb = Pool::new(backends_str).unwrap();
            assert_eq!(0, rrb.backends.len());
            assert!(rrb.get().is_none());
        }

        #[test]
        fn test_add_to_rrb_backend() {
            let mut rrb = Pool::new(vec![]).unwrap();
            assert!(rrb.get().is_none());
            assert!(rrb.add("327.0.0.1:6000").is_err());
            assert!(rrb.get().is_none());
            assert!(rrb.add("127.0.0.1:6000").is_ok());
            assert!(rrb.get().is_some());
        }

        #[test]
        fn test_remove_from_rrb_backend() {
            let backends_str = vec!["127.0.0.1:6000".to_string(), "127.0.0.1:6001".to_string()];
            let mut rrb = Pool::new(backends_str).unwrap();
            assert!(rrb.remove("327.0.0.1:6000").is_err());
            assert_eq!(2, rrb.backends.len());
            assert!(rrb.remove("127.0.0.1:6000").is_ok());
            assert_eq!(1, rrb.backends.len());
            assert!(rrb.remove("127.0.0.1:6000").is_ok());
            assert_eq!(1, rrb.backends.len());
        }

    }
}

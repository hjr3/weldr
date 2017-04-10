use std::sync::{Arc, RwLock};
use std::str::FromStr;
use hyper::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stats {
    failure: usize,
    success: usize,
}

impl Stats {
    pub fn new() -> Stats {
        Stats {
            failure: 0,
            success: 0,
        }
    }

    pub fn inc_success(&mut self) {
        self.success += 1;
    }

    pub fn inc_failure(&mut self) {
        self.failure += 1;
    }

    pub fn success(&self) -> usize {
        self.success
    }

    pub fn failure(&self) -> usize {
        self.failure
    }
}

/// A wrapper around the internals of Server
///
/// All functions hide the RwLock away so that we can ensure the lock is not held for a long time.
#[derive(Clone, Debug)]
pub struct Server {
    inner: Arc<RwLock<inner::Server>>,
}

impl Server {
    pub fn new(url: Url) -> Server {
        let inner = inner::Server::new(url);

        Server {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    pub fn with_map_host(self, map_host: bool) -> Self {
        let rwlock = Arc::try_unwrap(self.inner).expect("Multiple references");
        let server = rwlock.into_inner().expect("Lock is poisoned");
        let inner = server.with_map_host(map_host);

        Server {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    pub fn url(&self) -> Url {
        self.inner.read().expect("Lock is poisoned").url()
    }

    pub fn map_host(&self) -> bool {
        self.inner.read().expect("Lock is poisoned").map_host()
    }

    pub fn inc_success(&self) {
        self.inner.write().expect("Lock is poisoned").stats_mut().inc_success();
    }

    pub fn inc_failure(&self) {
        self.inner.write().expect("Lock is poisoned").stats_mut().inc_failure();
    }

    pub fn stats(&self) -> Stats {
        self.inner.read().expect("Lock is poisoned").stats().clone()
    }
}

impl FromStr for Server {
    type Err = ::hyper::error::ParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url: Url = if s.starts_with("http") {
            try!(s.parse())
        } else {
            try!(format!("http://{}", s).parse())
        };
        Ok(Server::new(url))
    }
}

/// Check if two servers are likely the same
///
/// This check is used by the Pool to find another server. The comparison is shallow as we are not
/// looking for a server with the exact same stats.
impl PartialEq for Server {
    fn eq(&self, other: &Server) -> bool {
        self.url() == other.url()
    }
}

mod inner {
    use hyper::Url;

    use super::Stats;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Server {
        url: Url,
        /// Whether to use the server host name in the Host header when making a backend request
        ///
        /// The default case is for the upstream Host header to be used. Some origins, such as
        /// Heroku or Amazon S3, must use the backend server hostname.
        map_host: bool,
        hc_failure: usize,
        stats: Stats,
    }

    impl Server {
        pub fn new(url: Url) -> Server {
            Server {
                url: url,
                map_host: false,
                hc_failure: 0,
                stats: Stats::new(),
            }
        }

        pub fn url(&self) -> Url {
            self.url.clone()
        }

        pub fn stats(&self) -> &Stats {
            &self.stats
        }

        pub fn stats_mut(&mut self) -> &mut Stats {
            &mut self.stats
        }

        pub fn map_host(&self) -> bool {
            self.map_host
        }

        /// Force the host name of the server to be used for backend requests
        ///
        /// This will consume the server
        pub fn with_map_host(self, map_host: bool) -> Self {
            Server {
                url: self.url,
                map_host: map_host,
                hc_failure: self.hc_failure,
                stats: self.stats,
            }
        }
    }
}

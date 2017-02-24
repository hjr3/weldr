use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use futures::Future;

use hyper::{self, Headers, Client, HttpVersion};
use hyper::client::{self, HttpConnector};
use hyper::client::Service;
use hyper::header;
use hyper::server::{self, Http};


// testing here before sending PR upstream
// TODO make this typed
header! { (Via, "Via") => [String] }
header! { (TE, "TE") => [String] }
header! { (ProxyAuthorization, "Proxy-Authorization") => [String] }
header! { (ProxyAuthenticate, "Proxy-Authenticate") => [String] }
header! { (Trailer, "Trailer") => [String] }

impl Via {

    /// Append a Via header to existing Via header
    ///
    /// This is used when the upstream sends a Via header and we need to create a list of Via
    /// header values.
    pub fn append(&mut self, other: Via) {
        let s = format!(", {}", other);
        self.0.push_str(&s);
    }
}

/// Create Via header for proxy to send downstream to origin server
///
/// The Via header may already exist, so create a new header based off the upstream value
pub fn create_via_header(via: Option<& Via>, version: &HttpVersion) -> Via {

    let version = match version {
        &HttpVersion::Http09 => "0.9",
        &HttpVersion::Http10 => "1.0",
        &HttpVersion::Http11 => "1.1",
        &HttpVersion::H2 => "2",
        &HttpVersion::H2c => "2",
        _ => unreachable!(),
    };

    let value = Via(format!("{} alacrity", version));

    match via {
        Some(v) => {
            let mut v = v.clone();
            v.append(value);
            v
        }
        None => {
            value
        }
    }
}

/// Remove frontend request headers that should not be sent to backend
///
/// This creates a new collection rather than modify the existing one. Hyper can sometimes return
/// `None` even though a header is removed. Thus, we ignore the result.
pub fn filter_frontend_request_headers(headers: &Headers) -> Headers {

    let mut h = headers.clone();

    headers.get::<header::Connection>().and_then(|c| {
        for c_h in &c.0 {
            match c_h {
                &header::ConnectionOption::Close => {
                    let _ = h.remove_raw("Close");
                }

                &header::ConnectionOption::KeepAlive => {
                    let _ = h.remove_raw("Keep-Alive");
                }

                &header::ConnectionOption::ConnectionHeader(ref o) => {
                    let _ = h.remove_raw(&o);
                }
            }
        }

        Some(())
    });

    let _ = h.remove::<header::Connection>();
    let _ = h.remove::<TE>();
    let _ = h.remove::<header::TransferEncoding>();
    let _ = h.remove::<ProxyAuthorization>();
    let _ = h.remove::<Trailer>();
    let _ = h.remove::<header::Upgrade>();

    h
}

/// Map a frontend request to a backend request
///
/// The primary purpose of this function is to add and remove headers as required by an
/// intermediary conforming to the HTTP spec.
fn map_request(req: server::Request, url: &str) -> client::Request {
    // TODO fix
    let mut r = client::Request::new(req.method().clone(), url.parse().unwrap());

    let via = create_via_header(
        req.headers().get::<Via>(),
        req.version());

    let mut headers = filter_frontend_request_headers(req.headers());
    headers.set(via);
    r.headers_mut().extend(headers.iter());

    r.set_body(req.body());
    r
}

pub fn filter_backend_response_headers(headers: &Headers) -> Headers {
    let mut h = headers.clone();

    let _ = h.remove::<header::TransferEncoding>();
    let _ = h.remove::<ProxyAuthenticate>();
    let _ = h.remove::<Trailer>();
    let _ = h.remove::<header::Upgrade>();

    h
}

/// Map a backend response to a frontend response
///
/// The primary purpose of this function is to add and remove headers as required by an
/// intermediary conforming to the HTTP spec.
fn map_response(res: client::Response) -> server::Response {
    let mut r = server::Response::new().with_status(*res.status());

    let headers = filter_backend_response_headers(res.headers());
    r.headers_mut().extend(headers.iter());

    r.set_body(res.body());
    r
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Server {
    addr: SocketAddr,
    hc_failure: usize,
    stats: Stats,
}

impl Server {
    pub fn new(addr: SocketAddr) -> Server {
        Server {
            addr: addr,
            hc_failure: 0,
            stats: Stats::new(),
        }
    }

    pub fn stats_mut(&mut self) -> &mut Stats {
        &mut self.stats
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    pub fn addr(&self) -> &SocketAddr {
        &self.addr
    }
}

pub struct HttpPool {
    client: Client<HttpConnector>,
    pool: Pool,
}

impl HttpPool {
    pub fn new(client: Client<HttpConnector>, pool: Pool) -> HttpPool {
        HttpPool {
            client: client,
            pool: pool,
        }
    }
}

impl Service for HttpPool {
    type Request = server::Request;
    type Response = server::Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item=server::Response, Error=Self::Error>>;

    fn call(&self, req: server::Request) -> Self::Future {
        let f = |server: &Server| -> Self::Future {
            let url = format!("http://{}{}", server.addr(), req.path());
            debug!("Preparing backend request to {:?}", url);

            let client_req = map_request(req, &url);

            let backend = self.client.call(client_req).then(move |res| {
                match res {
                    Ok(res) => {
                        debug!("Response: {}", res.status());
                        debug!("Headers: \n{}", res.headers());

                        let server_response = map_response(res);
                        //server.stats_mut().inc_success();

                        ::futures::finished(server_response)
                    }
                    Err(e) => {
                        error!("Error connecting to backend: {:?}", e);
                        //server.stats_mut().inc_failure();
                        ::futures::failed(e)
                    }
                }
            });

            Box::new(backend)
        };

        self.pool.loan(f)
    }
}

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
    pub fn with_servers(backends: Vec<Server>) -> Pool {
        let inner = inner::Pool::new(backends);

        Pool {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    pub fn new() -> Pool {
        Pool::with_servers(vec![])
    }

    pub fn loan<F>(&self, f: F) -> Box<Future<Item=server::Response, Error=hyper::Error>>
        where F: FnOnce(&Server) -> Box<Future<Item=server::Response, Error=hyper::Error>>
    {
        self.inner.write().expect("Lock is poisoned").loan(f)
    }

    /// Get a `Server` from the pool
    ///
    /// The pool may be exhausted of eligible addresses to connect to. The client is expected to
    /// handle this scenario.
    pub fn get(&self) -> Option<Server> {
        self.inner.write().expect("Lock is poisoned").get()
    }

    /// Returns all `Server` from the pool
    pub fn all(&self) -> Vec<Server> {
        self.inner.write().expect("Lock is poisoned").all()
    }

    /// Add a new server to the pool
    ///
    /// Currently, it is possible to add the same server more then once
    pub fn add(&self, backend: SocketAddr) {
        let server = Server::new(backend);
        self.inner.write().expect("Lock is poisoned").add(server)
    }

    /// Remove a server from the pool
    ///
    /// This will remove all instance of the given server. See `add` method for details on
    /// duplicate servers.
    pub fn remove(&self, backend: &Server) {
        self.inner.write().expect("Lock is poisoned").remove(backend)
    }
}

pub mod inner {
    use std::io::{Error, ErrorKind};
    use futures::Future;
    use hyper;
    use hyper::server;
    use super::Server;

    pub struct Pool {
        backends: Vec<Server>,
        last_used: usize,
    }

    impl Pool {
        pub fn new(backends: Vec<Server>) -> Pool {
            Pool {
                backends: backends,
                last_used: 0,
            }
        }

        pub fn loan<F>(&mut self, f: F) -> Box<Future<Item=server::Response, Error=hyper::Error>>
            where F: FnOnce(&Server) -> Box<Future<Item=server::Response, Error=hyper::Error>>
        {
            if self.backends.is_empty() {
                let e = Error::new(ErrorKind::Other, "Pool is exhausted of socket addresses");
                return Box::new(::futures::failed(hyper::Error::Io(e)));
            }

            self.last_used = (self.last_used + 1) % self.backends.len();

            if self.backends.get(self.last_used).is_none() {
                let e = Error::new(ErrorKind::Other, format!("No server found at index {}", self.last_used));
                return Box::new(::futures::failed(hyper::Error::Io(e)));
			}

		    // TODO make this safe. for some reason the type checker does like when i used
            // map_or_else
            let server = unsafe { self.backends.get_unchecked_mut(self.last_used) };
			f(&server)
        }

        pub fn get(&mut self) -> Option<Server> {
            if self.backends.is_empty() {
                warn!("Pool is exhausted of socket addresses");
                return None;
            }
            self.last_used = (self.last_used + 1) % self.backends.len();
            self.backends.get(self.last_used).map(|server| {
                debug!("Pool is cloaning (hehe) out {:?}", server);
                server.clone()
            })
        }

        pub fn all(&mut self) -> Vec<Server> {
            if self.backends.is_empty() {
                warn!("Pool is exhausted of socket addresses");
                return Vec::new();
            }
            self.backends.clone()
        }

        pub fn add(&mut self, server: Server) {
            self.backends.push(server);
        }

        pub fn remove(&mut self, server: &Server) {
            self.backends.retain(|s| s != server);
        }
    }


    #[cfg(test)]
    mod tests {
        use super::Pool;
        use super::Server;
        use std::str::FromStr;

        #[test]
        fn test_rrb_backend() {
            let backends: Vec<Server> = vec![
                Server::new(FromStr::from_str("127.0.0.1:6000").unwrap()),
                Server::new(FromStr::from_str("127.0.0.1:6001").unwrap()),
            ];

            let mut rrb = Pool::new(backends);
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
            let mut rrb = Pool::new(backends);
            assert_eq!(0, rrb.backends.len());
            assert!(rrb.get().is_none());
            assert!(rrb.all().is_empty());
        }

        #[test]
        fn test_add_to_rrb_backend() {
            let mut rrb = Pool::new(vec![]);
            assert!(rrb.get().is_none());
            let server = Server::new(FromStr::from_str("127.0.0.1:6000").unwrap());
            rrb.add(server.clone());
            assert!(rrb.get().is_some());
            assert_eq!(vec![server], rrb.all());
        }

        #[test]
        fn test_remove_from_rrb_backend() {
            let mut rrb = Pool::new(vec![]);
            let server1 = Server::new(FromStr::from_str("127.0.0.1:6000").unwrap());
            let server2 = Server::new(FromStr::from_str("127.0.0.1:6001").unwrap());
            rrb.add(server1.clone());
            rrb.add(server2.clone());
            assert_eq!(2, rrb.backends.len());
            assert_eq!(vec![server1.clone(), server2.clone()], rrb.all());

            let unknown_server = Server::new(FromStr::from_str("127.0.0.1:1234").unwrap());
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
}

#[cfg(test)]
mod tests {
    use hyper::{HttpVersion, Headers};
    use hyper::header;

    use super::*;

    #[test]
    #[ignore]
    /// Send HTTP 1.0 request and ensure proxy sends HTTP 1.1
    ///
    /// Per RFC 7230 Section 2.6 - MUST send own HTTP version
    fn test_must_send_own_http_version() {
        // logic is in, waiting for mocked request support to test
    }

    #[test]
    #[ignore]
    /// Send HTTP request and ensure proxy sets proper Via header
    ///
    /// Per RFC 7230 Section 5.7.1
    fn test_proxy_sets_via_header() {
        // logic is in, waiting for mocked request support to test
    }

    #[test]
    fn test_create_via_header() {
        let via = Via("1.0 proxy".to_owned());
        let version = HttpVersion::Http11;

        let given = create_via_header(None, &version);

        assert_eq!(Via("1.1 alacrity".to_owned()), given);

        let given = create_via_header(Some(&via), &version);

        assert_eq!(Via("1.0 proxy, 1.1 alacrity".to_owned()), given);
    }

    #[test]
    /// Per RFC 2616 Section 13.5.1 - MUST remove hop-by-hop headers
    /// Per RFC 7230 Section 6.1 - MUST remove Connection and Connection option headers
    fn test_filter_frontend_request_headers() {

        let bad = vec![
            ("TE", "gzip"),
            ("Transfer-Encoding", "chunked"),
            ("Host", "example.net"),
            ("Connection", "Keep-Alive, Foo, Bar"),
            ("Foo", "abc"),
            ("Foo", "def"),
            ("Keep-Alive", "timeout=30"),
            ("Proxy-Authorization", "randombase64value"),
            ("Trailer", "X-Random-Header"),
            ("Upgrade", "HTTP/2.0, SHTTP/1.3, IRC/6.9, RTA/x11"),
        ];

        let mut headers = Headers::new();

        for (name, value) in bad {
            headers.set_raw(name, value);
        }

        let given = filter_frontend_request_headers(&headers);

        // defining these here only to let me assert
        header! { (Foo, "Foo") => [String] }
        header! { (KeepAlive, "Keep-Alive") => [String] }

        assert_eq!(false, given.has::<TE>());
        assert_eq!(false, given.has::<header::TransferEncoding>());
        assert_eq!(true, given.has::<header::Host>());
        assert_eq!(false, given.has::<header::Connection>());
        assert_eq!(false, given.has::<Foo>());
        assert_eq!(false, given.has::<KeepAlive>());
        assert_eq!(false, given.has::<ProxyAuthorization>());
        assert_eq!(false, given.has::<Trailer>());
        assert_eq!(false, given.has::<header::Upgrade>());
    }

    #[test]
    /// Per RFC 2616 Section 13.5.1 - MUST remove hop-by-hop headers
    fn test_filter_backend_response_headers() {

        let bad = vec![
            ("Transfer-Encoding", "chunked"),
            ("Host", "example.net"),
            ("Proxy-Authenticate", "randombase64value"),
            ("Trailer", "X-Random-Header"),
            ("Upgrade", "HTTP/2.0"),
        ];

        let mut headers = Headers::new();

        for (name, value) in bad {
            headers.set_raw(name, value);
        }

        let given = filter_backend_response_headers(&headers);

        assert_eq!(false, given.has::<header::TransferEncoding>());
        assert_eq!(true, given.has::<header::Host>());
        assert_eq!(false, given.has::<ProxyAuthenticate>());
        assert_eq!(false, given.has::<Trailer>());
        assert_eq!(false, given.has::<header::Upgrade>());
    }
}

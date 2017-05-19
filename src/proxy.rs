use std::io;
use std::net::SocketAddr;
use std::str;

use net2::TcpBuilder;
use net2::unix::UnixTcpBuilderExt;
use futures::{future, Future, Stream};
use tokio_core::reactor::{Core, Handle};
use tokio_core::net::{TcpListener, TcpStream};
use hyper::{self, Headers, Client, HttpVersion};
use hyper::client;
use hyper::client::Service;
use hyper::header;
use hyper::server::{self, Http};
use hyper_tls::HttpsConnector;
use hyper::{Url, Uri};

use pool::Pool;

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

    let value = Via(format!("{} weldr", version));

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

fn map_uri_to_url(uri: &Uri) -> Url {
    debug!("uri = {:?}", uri);
    Url::parse(
        &format!(
            "{}://{}{}?{}",
            uri.scheme().unwrap_or("http"),
            uri.authority().unwrap_or("example.com"),
            uri.path(),
            uri.query().unwrap_or(""),
        )
    ).expect("Failed to map uri to url")
}

/// Map a frontend request to a backend request
///
/// The primary purpose of this function is to add and remove headers as required by an
/// intermediary conforming to the HTTP spec.
fn map_request(req: server::Request) -> client::Request {
    let via = create_via_header(
        req.headers().get::<Via>(),
        req.version());

    let mut headers = filter_frontend_request_headers(req.headers());
    headers.set(via);

    let url = map_uri_to_url(req.uri());

    let mut r = client::Request::new(req.method().clone(), url);
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

struct Proxy {
    client: Client<HttpsConnector>,
    pool: Pool,
}

impl Service for Proxy {
    type Request = server::Request;
    type Response = server::Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item=server::Response, Error=Self::Error>>;

    fn call(&self, req: server::Request) -> Self::Future {

        let mut client_req = map_request(req);

        self.pool.request(|server| {

            // TODO need to add query strings in here as well
            let url = server.url().join(client_req.url().path()).unwrap();
            let map_host = server.map_host();
            debug!("Preparing backend request to {:?}", url);

            if map_host {
                // add host header related to backend
                let _ = client_req.headers_mut().remove::<header::Host>();
                let host = url.host_str().unwrap().to_string();
                let port = url.port_or_known_default();
                client_req.headers_mut().set(header::Host::new(host, port));
            }
            client_req.set_url(url);

            let backend = self.client.call(client_req).then(move |res| {
                match res {
                    Ok(res) => {
                        debug!("Response: {}", res.status());
                        debug!("Headers: \n{}", res.headers());

                        let server_response = map_response(res);

                        ::futures::finished(server_response)
                    }
                    Err(e) => {
                        error!("Error connecting to backend: {:?}", e);
                        ::futures::failed(e)
                    }
                }
            });

            Box::new(backend)
        })
    }
}

/// Run server with default Core
pub fn run(addr: SocketAddr, pool: Pool, core: Core) -> io::Result<()> {
    let handle = core.handle();

    let listener = TcpBuilder::new_v4()?;
    listener.reuse_address(true)?;
    listener.reuse_port(true)?;
    let listener = listener.bind(&addr)?;
    let listener = listener.listen(128)?;
    let listener = TcpListener::from_listener(listener, &addr, &handle)?;

    run_with(core, listener, pool, future::empty())
}

/// Run server with specified Core, TcpListener, Pool
///
/// This is useful for integration testing where the port is set to 0 and the test code needs to
/// determine the local addr.
pub fn run_with<F>(mut core: Core, listener: TcpListener, pool: Pool, shutdown_signal: F) -> io::Result<()>
    where F: Future<Item = (), Error = hyper::Error>,
{
    let handle = core.handle();

    let local_addr = listener.local_addr()?;
    let srv = listener.incoming().for_each(move |(socket, addr)| {
        proxy(socket, addr, pool.clone(), &handle);

        Ok(())
    });

    info!("Listening on http://{}", &local_addr);
    match core.run(shutdown_signal.select(srv.map_err(|e| e.into()))) {
        Ok(((), _incoming)) => Ok(()),
        Err((e, _other)) => return Err(io::Error::new(io::ErrorKind::Other, e)),
    }
}

fn proxy(socket: TcpStream, addr: SocketAddr, pool: Pool, handle: &Handle) {

    // disable Nagle's algo
    // https://github.com/hyperium/hyper/issues/944
    socket.set_nodelay(true).unwrap();
    let client = Client::configure()
        .connector(HttpsConnector::new(4, handle))
        .build(&handle);
    let service = Proxy {
        client: client,
        pool: pool,
    };

    let http = Http::new();
    http.bind_connection(&handle, socket, addr, service);
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

        assert_eq!(Via("1.1 weldr".to_owned()), given);

        let given = create_via_header(Some(&via), &version);

        assert_eq!(Via("1.0 proxy, 1.1 weldr".to_owned()), given);
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

use std::io;
use std::net::SocketAddr;
use std::str::{self, FromStr};

use futures::{Future, Stream};
use tokio_core::reactor::Handle;
use tokio_core::net::{TcpListener, TcpStream};
use hyper::{self, Headers, Body, Client, HttpVersion};
use hyper::client::{self, HttpConnector, Service};
use hyper::header;
use hyper::server::{self, Http};
use hyper_tls::HttpsConnector;
use hyper::Uri;
use hyper_timeout::TimeoutConnector;

use pool::Pool;
use config::Config;

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
pub fn create_via_header(via: Option<&Via>, version: &HttpVersion) -> Via {

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
        None => value,
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
fn map_request(req: server::Request) -> client::Request {
    let via = create_via_header(req.headers().get::<Via>(), &req.version());

    let mut headers = filter_frontend_request_headers(req.headers());
    headers.set(via);

    // TODO fix clone
    let mut r = client::Request::new(req.method().clone(), req.uri().clone());
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
    let mut r = server::Response::new().with_status(res.status());

    let headers = filter_backend_response_headers(res.headers());
    r.headers_mut().extend(headers.iter());

    r.set_body(res.body());
    r
}

struct Proxy {
    client: Client<TimeoutConnector<HttpsConnector<HttpConnector>>, Body>,
    pool: Pool,
}

impl Service for Proxy {
    type Request = server::Request;
    type Response = server::Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = server::Response, Error = Self::Error>>;

    fn call(&self, req: server::Request) -> Self::Future {

        let mut client_req = map_request(req);

        self.pool.request(|server| {

            let url = format!(
                "{}{}?{}",
                server.url(),
                client_req.uri().path(),
                client_req.uri().query().unwrap_or("")
            );
            // TODO proper error handling
            let uri = Uri::from_str(&url).expect("Failed to parse url");
            let map_host = server.map_host();
            debug!("Preparing backend request to {:?}", url);

            if map_host {
                // add host header related to backend
                let _ = client_req.headers_mut().remove::<header::Host>();
                let host = uri.host().unwrap().to_string();
                let port = uri.port();
                client_req.headers_mut().set(header::Host::new(host, port));
            }
            client_req.set_uri(uri);

            let backend = self.client.call(client_req).then(move |res| match res {
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
            });

            Box::new(backend)
        })
    }
}

pub fn serve(listener: TcpListener, pool: Pool, handle: &Handle, config: &Config) -> io::Result<Box<Future<Item = (), Error = io::Error>>>
{
    let handle = handle.clone();
    let config = config.clone();
    let local_addr = listener.local_addr()?;
    info!("Listening on http://{}", &local_addr);
    let srv = listener.incoming().for_each(move |(socket, addr)| {
        proxy(socket, addr, pool.clone(), &handle, &config);

        Ok(())
    });

    return Ok(Box::new(srv));
}

fn proxy(socket: TcpStream, addr: SocketAddr, pool: Pool, handle: &Handle, config: &Config) {

    // disable Nagle's algo
    // https://github.com/hyperium/hyper/issues/944
    socket.set_nodelay(true).unwrap();
    let connector = HttpsConnector::new(4, handle).unwrap();
    let mut tm = TimeoutConnector::new(connector, &handle);
    tm.set_connect_timeout(config.timeout.connect);
    tm.set_read_timeout(config.timeout.read);
    tm.set_write_timeout(config.timeout.write);
    let client = Client::configure()
        .connector(tm)
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

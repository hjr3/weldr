use hyper::Uri;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Server {
    url: Uri,

    /// Track whether the upstream server wants the client host or server host header
    map_host: bool,
}

impl Server {
    pub fn new(url: Uri, map_host: bool) -> Self {
        Server {
            url: url,
            map_host: map_host,
        }
    }

    pub fn url(&self) -> Uri {
        self.url.clone()
    }

    pub fn map_host(&self) -> bool {
        self.map_host
    }
}

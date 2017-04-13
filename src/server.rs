use hyper::Url;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Server {
    url: Url,

    /// Track whether the upstream server wants the client host or server host header
    map_host: bool,
}

impl Server {
    pub fn new(url: Url, map_host: bool) -> Self {
        Server {
            url: url,
            map_host: map_host,
        }
    }

    pub fn url(&self) -> Url {
        self.url.clone()
    }

    pub fn map_host(&self) -> bool {
        self.map_host
    }

}

use std::str;

use bytes::{Buf, ByteBuf};
use nom::{IResult, Needed};
use nom::is_alphanumeric;

use http::{Request, RequestHead, Header, Response, ResponseHead, Version, Error, Chunk};

pub fn parse_request_head(buf: &[u8]) -> RequestHead {
    match request_head(buf) {
        IResult::Done(remaining, mut request_head) => {

            match parse_headers(remaining) {
                IResult::Done(_, mut headers) => {
                    request_head.headers.append(&mut headers);
                    request_head
                }
                _ => panic!("did not parse headers"),
            }
        }
        _ => panic!("did not parse request"),
    }
}

#[derive(Debug, Clone, Copy)]
enum RequestState {
    RequestLine,
    Headers,
    Body(usize),
    Complete,
}

pub struct RequestParser {
    state: RequestState,
    request: Option<Request>,
}

impl RequestParser {

    pub fn new() -> RequestParser {
        RequestParser {
            state: RequestState::RequestLine,
            request: None,
        }
    }

    /// Parse a request byte stream into a native object
    pub fn parse_request(&mut self, buf: &mut ByteBuf) -> Result<Option<Request>, Error> {
        loop {
            match self.state {
                RequestState::RequestLine => {
                    let consumed = match request_head(buf.bytes()) {
                        IResult::Done(remaining, request_head) => {
                            self.request = Some(Request {
                                head: request_head,
                                body: Vec::new(),
                            });
                            self.state = RequestState::Headers;

                            buf.len() - remaining.len()
                        }
                        IResult::Incomplete(n) => {
                            debug!("Incomplete parse attempt on frame. Needed {:?} more bytes", n);
                            return Ok(None);
                        }
                        IResult::Error(e) => {
                            error!("Error parsing request-line: {:?}", e);
                            return Err(Error::Invalid);
                        }
                    };

                    buf.drain_to(consumed);
                }
                RequestState::Headers => {
                    let consumed = match parse_headers(buf.bytes()) {
                        IResult::Done(remaining, mut headers) => {
                            let mut request = self.request.take().expect("Request does not exist");
                            request.head.headers.append(&mut headers);

                            // TODO: handle chunked request body if the request has a Transfer-Encoding header
                            if let Some(content_length) = request.head.content_length() {
                                self.state = RequestState::Body(content_length);
                            } else {
                                // no body
                                self.request = Some(request);
                                self.state = RequestState::Complete;
                                return Ok(self.request.take())
                            }

                            self.request = Some(request);
                            buf.len() - remaining.len()
                        }
                        IResult::Incomplete(n) => {
                            debug!("Incomplete parse attempt on frame. Needed {:?} more bytes", n);
                            return Ok(None);
                        }
                        IResult::Error(e) => {
                            error!("Error parsing request headers: {:?}", e);
                            return Err(Error::Invalid);
                        }
                    };

                    buf.drain_to(consumed);
                }
                RequestState::Body(content_length) => {
                    // buffer does not contain enough bytes to finish parsing the body,
                    // so return incomplete
                    if buf.len() < content_length {
                        return Ok(None);
                    }
                    let body = buf.drain_to(content_length);

                    let mut request = self.request.take().expect("Request does not exist");
                    request.body.push(Chunk(Vec::from(body.as_ref())));
                    self.request = Some(request);

                    self.state = RequestState::Complete;
                    return Ok(self.request.take())
                }
                RequestState::Complete => {
                    panic!("Tried to parse a completed request");
                }
            }
        }
    }
}



#[derive(Debug, Clone, Copy)]
enum ResponseState {
    StatusLine,
    Headers,
    Body(usize),
    ChunkedBody,
    Complete,
}

pub struct ResponseParser {
    state: ResponseState,
    response: Option<Response>,
}

impl ResponseParser {

    pub fn new() -> ResponseParser {
        ResponseParser {
            state: ResponseState::StatusLine,
            response: None,
        }
    }

    /// Parse a response byte stream into a native object
    ///
    /// This function acts as a boundary between nom and tokio. Instead of leaking IResult into the
    /// rest of the code, we map the three IResult variants to native Result/Option variants.
    pub fn parse_response(&mut self, buf: &mut ByteBuf) -> Result<Option<Response>, Error> {
        loop {
            match self.state {
                ResponseState::StatusLine => {
                    let consumed = match status_line(buf.bytes()) {
                        IResult::Done(remaining, response_head) => {
                            self.response = Some(Response {
                                head: response_head,
                                body: Vec::new(),
                            });

                            self.state = ResponseState::Headers;

                            buf.len() - remaining.len()
                        }
                        IResult::Incomplete(n) => {
                            debug!("Incomplete parse attempt on frame. Needed {:?} more bytes", n);
                            return Ok(None);
                        }
                        IResult::Error(e) => {
                            error!("Error parsing reponse status line: {:?}", e);
                            return Err(Error::Invalid);
                        }
                    };

                    buf.drain_to(consumed);
                }
                ResponseState::Headers => {
                    let consumed = match parse_headers(buf.bytes()) {
                        IResult::Done(remaining, mut headers) => {
                            let mut response = self.response.take().expect("Response does not exist");
                            response.head.headers.append(&mut headers);

                            match response.head.content_length() {
                                Some(content_length) => {
                                    self.state = ResponseState::Body(content_length);
                                }
                                None => {
                                    self.state = ResponseState::ChunkedBody;
                                }
                            }

                            self.response = Some(response);

                            buf.len() - remaining.len()
                        }
                        IResult::Incomplete(n) => {
                            debug!("Incomplete parse attempt on frame. Needed {:?} more bytes", n);
                            return Ok(None);
                        }
                        IResult::Error(e) => {
                            error!("Error parsing reponse headers: {:?}", e);
                            return Err(Error::Invalid);
                        }
                    };

                    buf.drain_to(consumed);
                }
                ResponseState::Body(content_length) => {
                    // buffer does not contain enough bytes to finish parsing the body,
                    // so return incomplete
                    if buf.len() < content_length {
                        return Ok(None);
                    }

                    let body = buf.drain_to(content_length);

                    let mut response = self.response.take().expect("Response does not exist");
                    response.body.push(Chunk(Vec::from(body.as_ref())));
                    self.response = Some(response);

                    self.state = ResponseState::Complete;

                    return Ok(self.response.take())
                }
                ResponseState::ChunkedBody => {
                    // TODO support chunked Trailer headers
                    let (consumed, size) = match chunk_header(buf.bytes()) {
                        IResult::Done(remaining, size) => {

                            let consumed = buf.len() - remaining.len();
                            (consumed, size)
                        }
                        IResult::Incomplete(n) => {
                            debug!("Incomplete parse attempt on chunk header. Needed {:?} more bytes", n);
                            return Ok(None);
                        }
                        IResult::Error(e) => {
                            error!("Error parsing chunk header: {:?}", e);
                            return Err(Error::Invalid);
                        }
                    };

                    buf.drain_to(consumed);

                    if size > 0 {
                        let body = buf.drain_to(size);

                        let mut response = self.response.take().expect("Response does not exist");
                        response.body.push(Chunk(Vec::from(body.as_ref())));
                        self.response = Some(response);
                    }

                    let consumed = match crlf(buf.bytes()) {
                        IResult::Done(remaining, _) => {
                            buf.len() - remaining.len()
                        }
                        IResult::Incomplete(n) => {
                            debug!("Incomplete parse attempt on crlf for chunked transfer termination. Needed {:?} more bytes", n);
                            return Ok(None);
                        }
                        IResult::Error(e) => {
                            error!("Error parsing crlf chunked transfer termination: {:?}", e);
                            return Err(Error::Invalid);
                        }
                    };

                    buf.drain_to(consumed);

                    if size == 0 {
                        self.state = ResponseState::Complete;
                        return Ok(self.response.take())
                    }
                }
                ResponseState::Complete => {
                    panic!("Tried to parse a completed request");
                }
            }
        }
    }
}

named!(request_head<RequestHead>,
    do_parse!(
        method: token >>
        sp >>
        uri: vchar_1 >> // ToDo proper URI parsing?
        sp >>
        version: http_version >>
        crlf >>
        (
            RequestHead {
                method: String::from_utf8_lossy(&method[..]).into_owned(),
                uri: String::from_utf8_lossy(&uri[..]).into_owned(),
                version: version,
                headers: Vec::new(),
            }
        )
    )
);

named!(status_line<ResponseHead>,
    do_parse!(
        version: http_version >>
        sp >>
        status: map_res!(
            take!(3),
            str::from_utf8
            ) >>
        sp >>
        reason: status_token >>
        crlf >>
        (
            ResponseHead {
                version: version,
                status: status.parse::<u16>().expect("Failed to parse status"),
                reason: String::from_utf8_lossy(&reason[..]).into_owned(),
                headers: Vec::new(),
            }
        )
    )
);

named!(parse_message_header<Header>,
    do_parse!(
        name: token >>
        tag!(":") >>
        many0!(lws) >> // per RFC 2616 "The field value MAY be preceded by any amount of LWS"
        value: take_while!(is_header_value_char) >> // ToDo handle folding?
        crlf >>
        (
            Header {
                name: String::from_utf8_lossy(name).into_owned(),
                value: String::from_utf8_lossy(value).into_owned(),
            }
        )
    )
);

named!(parse_headers< Vec<Header> >, terminated!(many0!(parse_message_header), crlf));

named!(http_version<Version>,
    do_parse!(
        tag!("HTTP/") >>
        version: alt!(
            tag!("0.9") => {|_| Version::Http09} |
            tag!("1.0") => {|_| Version::Http10} |
            tag!("1.1") => {|_| Version::Http11}
        ) >>
        (version)
    )
);

pub struct BodyParser {
    /// Number of remaining bytes to parse
    remaining: usize,
}

/// Progressive body parser
///
/// This parser will keep track of how many remaining bytes there are to extract
impl BodyParser {
    pub fn new(remaining: usize) -> BodyParser {
        BodyParser {
            remaining: remaining
        }
    }

    /// Parse the contents of a HTTP body into a chunk
    ///
    /// This function will attempt to make as much progress as possible. A
    /// value of `None` indicates that the body has been completely extracted
    /// from the buffer.
    pub fn parse(&mut self, buf: &mut ByteBuf) -> Result<Option<Chunk>, Error> {
        if self.remaining == 0 {
            return Ok(None);
        }

        if buf.len() == 0 {
            // TODO make this a better error
            return Err(Error::Invalid);
        }

        let length = ::std::cmp::min(buf.len(), self.remaining);

        let body = buf.drain_to(length);
        self.remaining -= length;
        Ok(Some(Chunk(Vec::from(body.as_ref()))))
    }
}

named!(hex_string<&str>,
    map_res!(
        take_while!(is_hex_digit),
        ::std::str::from_utf8
    )
);

pub fn chunk_size(input: &[u8]) -> IResult<&[u8], usize> {
    let (i, s) = try_parse!(input, hex_string);
    if i.len() == 0 {
        return IResult::Incomplete(Needed::Unknown);
    }
    match usize::from_str_radix(s, 16) {
        Ok(sz) => IResult::Done(i, sz),
        Err(_) => IResult::Error(::nom::ErrorKind::MapRes)
    }
}

named!(chunk_header<usize>, terminated!(chunk_size, crlf));

// Primitives
fn is_token_char(i: u8) -> bool {
    is_alphanumeric(i) ||
        b"!#$%&'*+-.^_`|~".contains(&i)
}
named!(token, take_while!(is_token_char));

fn is_status_token_char(i: u8) -> bool {
    is_alphanumeric(i) ||
        b"!#$%&'*+-.^_`|~ \t".contains(&i)
}
named!(status_token, take_while!(is_status_token_char));

named!(ht<char>, char!('\t'));
named!(sp<char>, char!(' '));
named!(lws<char>, alt!(sp | ht));
named!(crlf, tag!("\r\n"));

fn is_vchar(i: u8) -> bool {
    i > 32 && i <= 126
}

fn is_header_value_char(i: u8) -> bool {
    i >= 32 && i <= 126
}

named!(vchar_1, take_while!(is_vchar));

fn is_hex_digit(chr: u8) -> bool {
  (chr >= 0x30 && chr <= 0x39) || // 0-9
  (chr >= 0x41 && chr <= 0x46) || // A-F
  (chr >= 0x61 && chr <= 0x66)    // a-f
}


//
// Internal parser tests that are not part of the public interface
//
#[cfg(test)]
#[test]
fn test_http_version() {
    let tests = vec![
        ("HTTP/0.9", Version::Http09),
        ("HTTP/1.0", Version::Http10),
        ("HTTP/1.1", Version::Http11),
    ];

    for (given, expected) in tests {
        assert_eq!(IResult::Done(&b""[..], expected), http_version(given.as_bytes()));
    }
}

#[cfg(test)]
#[test]
fn test_chunk_header() {
    let tests = vec![
        ("7\r\n", 7),
        ("0\r\n", 0),
        ("4f\r\n", 79),
        ("4F\r\n", 79),
    ];

    for (given, expected) in tests {
        assert_eq!(IResult::Done(&b""[..], expected), chunk_header(given.as_bytes()));
    }
}

#[cfg(test)]
mod tests {
    use bytes::{Buf, ByteBuf};

    use super::*;
    use http::{Request, RequestHead, Response, ResponseHead, Header, Version, Error, Chunk};

    #[test]
    fn test_request_get() {
        let input =
            b"GET /index.html HTTP/1.1\r\n\
              Host: www.example.com\r\n\
              User-Agent: curl/7.43.0\r\n\
              Accept: */*\r\n\
              \r\n";

        let mut parser = RequestParser::new();
        let mut input = ByteBuf::from_slice(&input[..]);
        let given = parser.parse_request(&mut input);

        let expected = Request {
            head: RequestHead {
                method: String::from("GET"),
                uri: String::from("/index.html"),
                version: Version::Http11,
                headers: vec![
                    Header{
                        name: String::from("Host"),
                        value: String::from("www.example.com"),
                    },
                    Header{
                        name: String::from("User-Agent"),
                        value: String::from("curl/7.43.0"),
                    },
                    Header{
                        name: String::from("Accept"),
                        value: String::from("*/*"),
                    },
                ],
            },
            body: Vec::new(),
        };

        assert_eq!(expected, given.unwrap().unwrap());
    }

    #[test]
    fn test_request_post() {
        let input =
            b"POST /index.html HTTP/1.1\r\n\
              Host: www.example.com\r\n\
              Content-Length: 5\r\n\
              \r\n\
              hello";

        let mut parser = RequestParser::new();
        let mut input = ByteBuf::from_slice(&input[..]);
        let given = parser.parse_request(&mut input);

        let expected = Request {
            head: RequestHead {
                method: String::from("POST"),
                uri: String::from("/index.html"),
                version: Version::Http11,
                headers: vec![
                    Header{
                        name: String::from("Host"),
                        value: String::from("www.example.com"),
                    },
                    Header{
                        name: String::from("Content-Length"),
                        value: String::from("5"),
                    },
                ],
            },
            body: vec![Chunk(Vec::from("hello".as_bytes()))],
        };

        assert_eq!(expected, given.unwrap().unwrap());
    }

    #[test]
    fn test_request_partial_head() {
        let input =
            b"GET /index.html HTTP/1.1\r\n\
              Host: www.example.com\r\n\
              User-Agent: curl/7.43.0\r\n\
              Accept: */*\r\n";

        let mut parser = RequestParser::new();
        let mut input = ByteBuf::from_slice(&input[..]);
        let given = parser.parse_request(&mut input);

        assert_eq!(None, given.unwrap());
    }

    #[test]
    fn test_request_partial_body() {
        let input =
            b"POST /index.html HTTP/1.1\r\n\
              Host: www.example.com\r\n\
              Content-Length: 5\r\n\
              \r\n\
              hell";

        let mut parser = RequestParser::new();
        let mut input = ByteBuf::from_slice(&input[..]);
        let given = parser.parse_request(&mut input);

        assert_eq!(None, given.unwrap());
    }

    #[test]
    fn test_response_ok() {
        let input =
			b"HTTP/1.1 200 OK\r\n\
			  Content-Type: text/html; charset=UTF-8\r\n\
			  Content-Length: 11\r\n\
			  Cache-Control: public, max-age=600\r\n\
			  Date: Tue, 08 Nov 2016 19:11:27 GMT\r\n\
			  Connection: keep-alive\r\n\
			  \r\n\
			  Hello World";

        let mut parser = ResponseParser::new();
        let mut input = ByteBuf::from_slice(&input[..]);
        let given = parser.parse_response(&mut input);

        let expected = Response {
            head: ResponseHead {
                version: Version::Http11,
                status: 200,
                reason: String::from("OK"),
                headers: vec![
                    Header{
                        name: String::from("Content-Type"),
                        value: String::from("text/html; charset=UTF-8"),
                    },
                    Header{
                        name: String::from("Content-Length"),
                        value: String::from("11"),
                    },
                    Header{
                        name: String::from("Cache-Control"),
                        value: String::from("public, max-age=600"),
                    },
                    Header{
                        name: String::from("Date"),
                        value: String::from("Tue, 08 Nov 2016 19:11:27 GMT"),
                    },
                    Header{
                        name: String::from("Connection"),
                        value: String::from("keep-alive"),
                    },
                ],
            },
            body: vec![Chunk(Vec::from("Hello World".as_bytes()))]
        };

        assert_eq!(expected, given.unwrap().unwrap());
    }

    #[test]
    fn test_response_ok_extra() {
        let input =
			b"HTTP/1.1 200 OK\r\n\
			  Content-Type: text/html; charset=UTF-8\r\n\
			  Content-Length: 11\r\n\
			  Cache-Control: public, max-age=600\r\n\
			  Date: Tue, 08 Nov 2016 19:11:27 GMT\r\n\
			  Connection: keep-alive\r\n\
			  \r\n\
			  Hello WorldHTTP/1.1 200 OK";

        let mut parser = ResponseParser::new();
        let mut input = ByteBuf::from_slice(&input[..]);
        let given = parser.parse_response(&mut input);

        let expected = Response {
            head: ResponseHead {
                version: Version::Http11,
                status: 200,
                reason: String::from("OK"),
                headers: vec![
                    Header{
                        name: String::from("Content-Type"),
                        value: String::from("text/html; charset=UTF-8"),
                    },
                    Header{
                        name: String::from("Content-Length"),
                        value: String::from("11"),
                    },
                    Header{
                        name: String::from("Cache-Control"),
                        value: String::from("public, max-age=600"),
                    },
                    Header{
                        name: String::from("Date"),
                        value: String::from("Tue, 08 Nov 2016 19:11:27 GMT"),
                    },
                    Header{
                        name: String::from("Connection"),
                        value: String::from("keep-alive"),
                    },
                ],
            },
            body: vec![Chunk(Vec::from("Hello World".as_bytes()))]
        };

        assert_eq!(expected, given.unwrap().unwrap());

        assert_eq!("HTTP/1.1 200 OK".as_bytes(), input.bytes());
    }

    #[test]
    fn test_response_partial_head() {
        let input =
			b"HTTP/1.1 200 OK\r\n\
			  Content-Type: text/html; charset=UTF-8\r\n\
			  Content-Length: 11\r\n\
			  Cache-Control: public, max-age=600\r\n\
			  Date: Tue, 08 Nov 2016 19:11:27 GMT\r\n\
			  Connection: keep-alive\r\n";

        let mut parser = ResponseParser::new();
        let mut input = ByteBuf::from_slice(&input[..]);
        let given = parser.parse_response(&mut input);

        assert_eq!(None, given.unwrap());
    }

    #[test]
    fn test_response_partial_body() {
        let input =
			b"HTTP/1.1 200 OK\r\n\
			  Content-Type: text/html; charset=UTF-8\r\n\
			  Content-Length: 11\r\n\
			  Cache-Control: public, max-age=600\r\n\
			  Date: Tue, 08 Nov 2016 19:11:27 GMT\r\n\
			  Connection: keep-alive\r\n\
			  \r\n\
			  Hell";

        let mut parser = ResponseParser::new();
        let mut input = ByteBuf::from_slice(&input[..]);
        let given = parser.parse_response(&mut input);

        assert_eq!(None, given.unwrap());
    }

    #[test]
    fn test_response_invalid() {
        let input =
			b"junk";

        let mut parser = ResponseParser::new();
        let mut input = ByteBuf::from_slice(&input[..]);
        let given = parser.parse_response(&mut input);

        match given.unwrap_err() {
            Error::Invalid => {},
            _ => { panic!("Expected Error::Invalid") }
        }

        // TODO determine why malformed headers are not causing error
        // content length header is malformed
        //let input =
		//	b"HTTP/1.1 200 OK\r\n\
		//	  Content-Type: text/html; charset=UTF-8\r\n\
		//	  Content-Length:\r\n\
		//	  Cache-Control: public, max-age=600\r\n";

        //let given = parse_response(&input[..]);
        //println!("given = {:?}", given);

        //match given.unwrap_err() {
        //    Error::Invalid => {},
        //    _ => { panic!("Expected Error::Invalid") }
        //}
    }

    #[test]
    fn test_response_chunked() {
        let input =
            b"HTTP/1.1 200 OK\r\n\
              Content-Type: text/plain\r\n\
              Transfer-Encoding: chunked\r\n\
              \r\n\
              7\r\n\
              Mozilla\r\n\
              9\r\n\
              Developer\r\n\
              7\r\n\
              Network\r\n\
              0\r\n\
              \r\n";

        let mut parser = ResponseParser::new();
        let mut input = ByteBuf::from_slice(&input[..]);
        let given = parser.parse_response(&mut input);

        let expected = Response {
            head: ResponseHead {
                version: Version::Http11,
                status: 200,
                reason: String::from("OK"),
                headers: vec![
                    Header{
                        name: String::from("Content-Type"),
                        value: String::from("text/plain"),
                    },
                    Header{
                        name: String::from("Transfer-Encoding"),
                        value: String::from("chunked"),
                    },
                ],
            },
            body: vec![
                Chunk(Vec::from("Mozilla".as_bytes())),
                Chunk(Vec::from("Developer".as_bytes())),
                Chunk(Vec::from("Network".as_bytes())),
            ]
        };

        assert_eq!(expected, given.unwrap().unwrap());
    }

    #[test]
    fn test_parse_body() {
        let input = b"Hello world";
        let mut input = ByteBuf::from_slice(&input[..]);

        let mut bp = BodyParser::new(input.len());
        let given = bp.parse(&mut input);
        assert_eq!(Chunk(Vec::from("Hello world".as_bytes())), given.unwrap().unwrap());

        let given = bp.parse(&mut input);
        assert_eq!(None, given.unwrap());

    }
}

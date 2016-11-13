use std::str;
use std::ascii::AsciiExt;

use nom::IResult;
use nom::{digit,is_alphanumeric};

use http::{RequestHead, Header};

#[derive(Debug, Eq, PartialEq)]
pub struct ResponseHead {
    pub version: String,
    pub status: u16,
    pub reason: String,
    pub headers: Vec<Header>,
}

impl ResponseHead {
    pub fn content_length(&self) -> Option<usize> {
        self.headers
            .iter()
            .find(|h| h.name.to_ascii_lowercase().as_str() == "content-length")
            .map(|h| h.value.parse::<usize>().ok())
            .and_then(|len| len)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Response {
    pub head: ResponseHead,
    pub body: Vec<u8>,
}

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

pub fn parse_response(buf: &[u8]) -> Response {
    match response_head(buf) {
        IResult::Done(remaining, mut response_head) => {

            match parse_headers(remaining) {
                IResult::Done(remaining, mut headers) => {
                    response_head.headers.append(&mut headers);
                    assert!(response_head.content_length().unwrap() == remaining.len());

                    Response {
                        head: response_head,
                        body: Vec::from(remaining),
                    }
                }
                _ => panic!("did not parse headers"),
            }
        }
        _ => panic!("did not parse request"),
    }
}

named!(request_head<RequestHead>,
       chain!(
           method: token ~
           sp ~
           uri: vchar_1 ~ // ToDo proper URI parsing?
           sp ~
           version: http_version ~
           crlf, || {
               let _version = version;
               RequestHead {
                   method: String::from_utf8_lossy(&method[..]).into_owned(),
                   uri: String::from_utf8_lossy(&uri[..]).into_owned(),
                   version: String::from("11"), // TODO fix this
                   headers: Vec::new(),
               }
           }
           )
      );

named!(response_head<ResponseHead>,
       chain!(
           version: http_version ~
           sp           ~
           status:  take!(3)     ~
           sp           ~
           reason:  status_token ~
           crlf, || {
               let _version = version;
               let status = str::from_utf8(&status[..]).expect("Failed to read status bytes");
               let status = status.parse::<u16>().expect("Failed to parse status");
               ResponseHead {
                   version: String::from("11"), // TODO fix this
                   status: status,
                   reason: String::from_utf8_lossy(&reason[..]).into_owned(),
                   headers: Vec::new(),
               }
           }
           )
      );

named!(parse_message_header<Header>,
       chain!(
           name: token ~
           tag!(":") ~
           many0!(lws) ~ // per RFC 2616 "The field value MAY be preceded by any amount of LWS"
           value: take_while!(is_header_value_char) ~ // ToDo handle folding?
           crlf, || {
               Header {
                   name: String::from_utf8_lossy(name).into_owned(),
                   value: String::from_utf8_lossy(value).into_owned(),
               }
           }
           )
      );

named!(parse_headers< Vec<Header> >, terminated!(many0!(parse_message_header), opt!(crlf)));

named!(http_version<[&[u8];2]>,
       chain!(
           tag!("HTTP/") ~
           major: digit ~
           tag!(".") ~
           minor: digit, || {
               [major, minor] // ToDo do we need it?
           }
           )
      );

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

#[cfg(test)]
mod tests {
    use super::*;
    use http::{RequestHead, Header};

    #[test]
    fn test_request_get() {
        let input =
            b"GET /index.html HTTP/1.1\r\n\
              Host: www.example.com\r\n\
              User-Agent: curl/7.43.0\r\n\
              Accept: */*\r\n\
              \r\n";

        let head = parse_request_head(&input[..]);

        let expected = RequestHead {
            method: String::from("GET"),
            uri: String::from("/index.html"),
            version: String::from("11"),
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
        };

        assert_eq!(expected, head);
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

        let head = parse_response(&input[..]);

        let expected = Response {
            head: ResponseHead {
                version: String::from("11"),
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
            body: Vec::from("Hello World".as_bytes())
        };

        assert_eq!(expected, head);
    }
}

use std::{io, str};

use tokio_core::io::EasyBuf;

use super::{Slice, Headers};

use httparse;

pub struct Response {
    status_code: u16,
    // TODO: use a small vec to avoid this unconditional allocation
    headers: Vec<(Slice, Slice)>,
    data: EasyBuf,
}

impl Response {

    pub fn status_code(&self) -> u16 {
        self.status_code
    }

    pub fn content_length(&self) -> Option<usize> {
        self.headers().content_length()
    }

    pub fn transfer_encoding_chunked(&self) -> bool {
        self.headers().transfer_encoding_chunked()
    }

    pub fn append_data(&mut self, buf: &[u8]) {
        self.data.get_mut().extend_from_slice(buf);
    }

    fn headers(&self) -> Headers {
        Headers {
            headers: self.headers.iter(),
            data: &self.data,
        }
    }
}

pub fn decode(buf: &mut EasyBuf) -> io::Result<Option<Response>> {
    let (status_code, headers, amt) = {
        let mut headers = [httparse::EMPTY_HEADER; 16];
        let mut r = httparse::Response::new(&mut headers);

        let status = try!(r.parse(buf.as_slice()).map_err(|e| {
            let msg = format!("failed to parse http response: {:?}", e);
            io::Error::new(io::ErrorKind::Other, msg)
        }));

        let amt = match status {
            httparse::Status::Complete(amt) => amt,
            httparse::Status::Partial => return Ok(None),
        };

        let toslice = |a: &[u8]| {
            let start = a.as_ptr() as usize - buf.as_slice().as_ptr() as usize;
            assert!(start < buf.len());
            (start, start + a.len())
        };

        let status_code = r.code.unwrap();
        //let reason_phrase = toslice(r.reason.unwrap().as_bytes());
        let headers = r.headers
            .iter()
            .map(|h| (toslice(h.name.as_bytes()), toslice(h.value)))
            .collect();

        (status_code, headers, amt)
    };

    let response = Response {
        status_code: status_code,
        headers: headers,
        data: buf.drain_to(amt)
    };

    Ok(response.into())
}

pub fn encode(msg: Response, buf: &mut Vec<u8>) {
    buf.extend_from_slice(msg.data.as_ref());
}

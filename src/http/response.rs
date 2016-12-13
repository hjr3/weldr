use std::ascii::AsciiExt;
use std::{io, slice, str};

use tokio_core::io::EasyBuf;

use httparse;

pub struct Response {
    status_code: u16,
    // TODO: use a small vec to avoid this unconditional allocation
    headers: Vec<(Slice, Slice)>,
    data: EasyBuf,
}

type Slice = (usize, usize);

impl Response {

    pub fn status_code(&self) -> u16 {
        self.status_code
    }

    pub fn content_length(&self) -> Option<usize> {
        self.headers()
            .find(|h| h.0.to_ascii_lowercase().as_str() == "content-length")
            .and_then(|h| {
                let v = ::std::str::from_utf8(&h.1).unwrap();
                v.parse::<usize>().ok()
            })
    }

    pub fn append_data(&mut self, buf: &[u8]) {
        self.data.get_mut().extend_from_slice(buf);
    }

    fn headers(&self) -> ResponseHeaders {
        ResponseHeaders {
            headers: self.headers.iter(),
            res: self,
        }
    }

    fn slice(&self, slice: &Slice) -> &[u8] {
        &self.data.as_slice()[slice.0..slice.1]
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

pub struct ResponseHeaders<'res> {
    headers: slice::Iter<'res, (Slice, Slice)>,
    res: &'res Response,
}

impl<'res> Iterator for ResponseHeaders<'res> {
    type Item = (&'res str, &'res [u8]);

    fn next(&mut self) -> Option<(&'res str, &'res [u8])> {
        self.headers.next().map(|&(ref a, ref b)| {
            let a = self.res.slice(a);
            let b = self.res.slice(b);
            (str::from_utf8(a).unwrap(), b)
        })
    }
}

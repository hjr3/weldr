use std::io;

use futures::{Async, Poll, Stream, Sink, StartSend, AsyncSink};
use tokio_core::io::{Io, Codec, EasyBuf};
use tokio_proto::streaming::pipeline::Transport;

/// A unified `Stream` and `Sink` interface to an underlying `Io` object, using
/// the `Encode` and `Decode` traits to encode and decode frames.
///
/// You can acquire a `Framed` instance by using the `Io::framed` adapter.
pub struct Framed<T, C> {
    upstream: T,
    codec: C,
    eof: bool,
    is_readable: bool,
    rd: EasyBuf,
    wr: Vec<u8>,
}

impl<T: Io, C: Codec> Stream for Framed<T, C> {
    type Item = C::In;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<C::In>, io::Error> {
        loop {
            // If the read buffer has any pending data, then it could be
            // possible that `decode` will return a new frame. We leave it to
            // the decoder to optimize detecting that more data is required.
            if self.is_readable {
                if self.eof {
                    if self.rd.len() == 0 {
                        return Ok(None.into())
                    } else {
                        let frame = try!(self.codec.decode_eof(&mut self.rd));
                        return Ok(Async::Ready(Some(frame)))
                    }
                }
                trace!("attempting to decode a frame");
                if let Some(frame) = try!(self.codec.decode(&mut self.rd)) {
                    trace!("frame decoded from buffer");
                    return Ok(Async::Ready(Some(frame)));
                }
                self.is_readable = false;
            }

            assert!(!self.eof);

            // Otherwise, try to read more data and try again
            //
            // TODO: shouldn't read_to_end, that may read a lot
            let before = self.rd.len();
            let ret = self.upstream.read_to_end(&mut self.rd.get_mut());
            match ret {
                Ok(n) => {
                    debug!("Read {} bytes", n);
                    self.eof = true;
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if self.rd.len() == before {
                        return Ok(Async::NotReady)
                    }
                }
                Err(e) => return Err(e),
            }
            self.is_readable = true;
        }
    }
}

impl<T: Io, C: Codec> Sink for Framed<T, C> {
    type SinkItem = C::Out;
    type SinkError = io::Error;

    fn start_send(&mut self, item: C::Out) -> StartSend<C::Out, io::Error> {
        // If the buffer is already over 8KiB, then attempt to flush it. If after flushing it's
        // *still* over 8KiB, then apply backpressure (reject the send).
        if self.wr.len() > 8 * 1024 {
            try!(self.poll_complete());
            if self.wr.len() > 8 * 1024 {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        try!(self.codec.encode(item, &mut self.wr));
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), io::Error> {
        trace!("flushing framed transport");

        while !self.wr.is_empty() {
            trace!("writing; remaining={}", self.wr.len());
            let n = try_nb!(self.upstream.write(&self.wr));
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::WriteZero,
                                          "failed to write frame to transport"));
            }
            self.wr.drain(..n);
        }

        // Try flushing the underlying IO
        try_nb!(self.upstream.flush());

        trace!("framed transport flushed");
        return Ok(Async::Ready(()));
    }
}

pub fn framed<T, C>(io: T, codec: C) -> Framed<T, C> {
    Framed {
        upstream: io,
        codec: codec,
        eof: false,
        is_readable: false,
        rd: EasyBuf::new(),
        wr: Vec::with_capacity(8 * 1024),
    }
}

impl<T, C> Framed<T, C> {

    /// Returns a reference to the underlying I/O stream wrapped by `Framed`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise being
    /// worked with.
    pub fn get_ref(&self) -> &T {
        &self.upstream
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `Framed`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise being
    /// worked with.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.upstream
    }

    /// Consumes the `Framed`, returning its underlying I/O stream.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise being
    /// worked with.
    pub fn into_inner(self) -> T {
        self.upstream
    }
}

impl <T, C> Transport for Framed<T, C> where C: Codec + 'static, T: Io + 'static {}

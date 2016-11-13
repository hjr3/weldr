use std::io;

use futures::{Poll, Async};
use tokio_core::io::{Io, FramedIo};
use tokio_proto::{TryRead, TryWrite};

use bytes::Buf;
use bytes::ByteBuf;

/// Implementation of parsing a frame from an internal buffer.
///
/// This trait is used when constructing an instance of `ProxyFramed`. It defines how
/// to parse the incoming bytes on a stream to the specified type of frame for
/// that framed I/O stream.
///
/// The primary method of this trait, `parse`, attempts to parse a method from a
/// buffer of bytes. It has the option of returning `NotReady`, indicating that
/// more bytes need to be read before parsing can continue as well.
pub trait Parse {

    /// The type that this instance of `Parse` will attempt to be parsing.
    ///
    /// This is typically a frame being parsed from an input stream, such as an
    /// HTTP request, a Redis command, etc.
    type Out;

    /// Attempts to parse a frame from the provided buffer of bytes.
    ///
    /// This method is called by `ProxyFramed` whenever bytes are ready to be parsed.
    /// The provided buffer of bytes is what's been read so far, and this
    /// instance of `Parse` can determine whether an entire frame is in the
    /// buffer and is ready to be returned.
    ///
    /// If an entire frame is available, then this instance will remove those
    /// bytes from the buffer provided and return them as a parsed frame. Note
    /// that removing bytes from the provided buffer doesn't always necessarily
    /// copy the bytes, so this should be an efficient operation in most
    /// circumstances.
    ///
    /// If the bytes look valid, but a frame isn't fully available yet, then
    /// `Async::NotReady` is returned. This indicates to the `ProxyFramed` instance
    /// that it needs to read some more bytes before calling this method again.
    ///
    /// Finally, if the bytes in the buffer are malformed then an error is
    /// returned indicating why. This informs `ProxyFramed` that the stream is now
    /// corrupt and should be terminated.
    fn parse(&mut self, buf: &mut ByteBuf) -> Poll<Self::Out, io::Error>;

    /// A default method available to be called when there are no more bytes
    /// available to be read from the underlying I/O.
    ///
    /// This method defaults to calling `parse` and returns an error if
    /// `NotReady` is returned. Typically this doesn't need to be implemented
    /// unless the framing protocol differs near the end of the stream.
    fn done(&mut self, buf: &mut ByteBuf) -> io::Result<Self::Out> {
        match try!(self.parse(buf)) {
            Async::Ready(frame) => Ok(frame),
            Async::NotReady => Err(io::Error::new(io::ErrorKind::Other,
                                                  "bytes remaining on stream")),
        }
    }
}

/// A trait for serializing frames into a byte buffer.
///
/// This trait is used as a building block of `ProxyFramed` to define how frames are
/// serialized into bytes to get passed to the underlying byte stream. Each
/// frame written to `ProxyFramed` will be serialized with this trait to an internal
/// buffer. That buffer is then written out when possible to the underlying I/O
/// stream.
pub trait Serialize {

    /// The frame that's being serialized to a byte buffer.
    ///
    /// This type is the type of frame that's also being written to a `ProxyFramed`.
    type In;

    /// Serializes a frame into the buffer provided.
    ///
    /// This method will serialize `msg` into the byte buffer provided by `buf`.
    /// The `buf` provided is an internal buffer of the `ProxyFramed` instance and
    /// will be written out when possible.
    fn serialize(&mut self, msg: Self::In, buf: &mut ByteBuf);
}

pub struct ProxyFramed<T, P, S> {
    upstream: T,
    parse: P,
    serialize: S,
    is_readable: bool,
    is_writeable: bool,
    rd: ByteBuf,
    wr: ByteBuf,
}

impl<T, P, S> ProxyFramed<T, P, S>
    where T: Io + TryRead + TryWrite,
          P: Parse,
          S: Serialize,
{
    /// Creates a new instance of `ProxyFramed` from the given component pieces.
    ///
    /// This method will create a new instance of `ProxyFramed` which implements
    /// `FramedIo` for reading and writing frames from an underlying I/O stream.
    /// The `upstream` argument here is the byte-based I/O stream that it will
    /// be operating on. Data will be read from this stream and parsed with
    /// `parse` into frames. Frames written to this instance will be serialized
    /// by `serialize` and then written to `upstream`.
    ///
    /// The `rd` and `wr` buffers provided are used for reading and writing
    /// bytes and provide a small amount of control over how buffering happens.
    pub fn new(upstream: T,
               parse: P,
               serialize: S) -> ProxyFramed<T, P, S> {

        trace!("Creating new ProxyFramed transport");
        ProxyFramed {
            upstream: upstream,
            parse: parse,
            serialize: serialize,
            is_readable: true,
            is_writeable: true,
            rd: ByteBuf::with_capacity(8 * 1024),
            wr: ByteBuf::with_capacity(8 * 1024),
        }
    }
}

impl<T, P, S> FramedIo for ProxyFramed<T, P, S>
    where T: Io,
          P: Parse,
          S: Serialize,
{
    type In = S::In;
    type Out = P::Out;

    fn poll_read(&mut self) -> Async<()> {
        if self.is_readable {
            Async::Ready(())
        } else {
            Async::NotReady
        }
    }

    fn read(&mut self) -> Poll<Self::Out, io::Error> {
        trace!("Reading from upstream");

        let bytes = try_ready!(self.upstream.try_read_buf(&mut self.rd));
        trace!("Read {} bytes", bytes);

        let request = try_ready!(self.parse.parse(&mut self.rd));

        Ok(Async::Ready(request))
    }

    fn poll_write(&mut self) -> Async<()> {
        if self.is_writeable {
            Async::Ready(())
        } else {
            Async::NotReady
        }
    }

    fn write(&mut self, msg: Self::In) -> Poll<(), io::Error> {
        //trace!("Writing to {}", self.upstream.peer_addr().unwrap());

        // Serialize the msg
        self.serialize.serialize(msg, &mut self.wr);

        trace!("writing; remaining={:?}", self.wr.len());

        let bytes = try_ready!(self.upstream.try_write_buf(&mut self.wr));
        trace!("Wrote {} bytes", bytes);
        self.wr.clear();
        self.is_writeable = false;

        // TODO: should provide some backpressure, such as when the buffer is
        //       too full this returns `NotReady` or something like that.
        Ok(Async::Ready(()))
    }

    fn flush(&mut self) -> Poll<(), io::Error> {
        //trace!("Flushing I/O to {}", self.upstream.peer_addr().unwrap());

        try_ready!(self.upstream.try_flush());

        loop {
            if self.wr.bytes().len() == 0 {
                trace!("framed transport flushed");
                return Ok(Async::Ready(()));
            }

            trace!("writing; remaining={:?}", self.wr.len());

            let bytes = try_ready!(self.upstream.try_write_buf(&mut self.wr));
            trace!("Wrote {} bytes", bytes);
            self.wr.clear();
        }
    }
}

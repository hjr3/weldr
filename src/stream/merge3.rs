use futures::{Poll, Async};
use futures::stream::{Stream, Fuse};

/// An adapter for merging the output of two streams.
///
/// The merged stream produces items from one or both of the underlying
/// streams as they become available. Errors, however, are not merged: you
/// get at most one error at a time.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Merge3<S1, S2, S3: Stream> {
    stream1: Fuse<S1>,
    stream2: Fuse<S2>,
    stream3: Fuse<S3>,
    queued_error: Option<S3::Error>,
}

pub fn new<S1, S2, S3>(stream1: S1, stream2: S2, stream3: S3) -> Merge3<S1, S2, S3>
    where S1: Stream, S2: Stream<Error = S1::Error>, S3: Stream<Error = S1::Error>
{
    Merge3 {
        stream1: stream1.fuse(),
        stream2: stream2.fuse(),
        stream3: stream3.fuse(),
        queued_error: None,
    }
}

/// An item returned from a merge stream, which represents an item from one or
/// both of the underlying streams.
#[derive(Debug)]
pub enum Merged3Item<I1, I2, I3> {
    /// An item from the first stream
    First(I1),
    /// An item from the second stream
    Second(I2),
    /// An item from the third stream
    Third(I3),
    /// Items from first and second stream
    FirstSecond(I1, I2),
    /// Items from second and third stream
    SecondThird(I2, I3),
    /// Items from first and third stream
    FirstThird(I1, I3),
    /// Items from all streams
    All(I1, I2, I3),
}

impl<S1, S2, S3> Stream for Merge3<S1, S2, S3>
    where S1: Stream, S2: Stream<Error = S1::Error>, S3: Stream<Error = S1::Error>
{
    type Item = Merged3Item<S1::Item, S2::Item, S3::Item>;
    type Error = S1::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if let Some(e) = self.queued_error.take() {
            return Err(e)
        }

        match try!(self.stream1.poll()) {
            Async::NotReady => {
                match try!(self.stream2.poll()) {
                    Async::NotReady => {
                        match try_ready!(self.stream3.poll()) {
                            Some(item3) => Ok(Async::Ready(Some(Merged3Item::Third(item3)))),
                            None => Ok(Async::NotReady),
                        }
                    }
                    Async::Ready(None) => {
                        match try_ready!(self.stream3.poll()) {
                            Some(item3) => Ok(Async::Ready(Some(Merged3Item::Third(item3)))),
                            None => Ok(Async::NotReady),
                        }
                    }
                    Async::Ready(Some(item2)) => {
                        match self.stream3.poll() {
                            Err(e) => {
                                self.queued_error = Some(e);
                                Ok(Async::Ready(Some(Merged3Item::Second(item2))))
                            }
                            Ok(Async::NotReady) | Ok(Async::Ready(None)) => {
                                Ok(Async::Ready(Some(Merged3Item::Second(item2))))
                            }
                            Ok(Async::Ready(Some(item3))) => {
                                Ok(Async::Ready(Some(Merged3Item::SecondThird(item2, item3))))
                            }
                        }
                    }
                }
            }
            Async::Ready(None) => {
                match try!(self.stream2.poll()) {
                    Async::NotReady => {
                        match try_ready!(self.stream3.poll()) {
                            Some(item3) => Ok(Async::Ready(Some(Merged3Item::Third(item3)))),
                            None => Ok(Async::NotReady),
                        }
                    }
                    Async::Ready(None) => {
                        match try_ready!(self.stream3.poll()) {
                            Some(item3) => Ok(Async::Ready(Some(Merged3Item::Third(item3)))),
                            None => Ok(Async::Ready(None)),
                        }
                    }
                    Async::Ready(Some(item2)) => {
                        match self.stream3.poll() {
                            Err(e) => {
                                self.queued_error = Some(e);
                                Ok(Async::Ready(Some(Merged3Item::Second(item2))))
                            }
                            Ok(Async::NotReady) | Ok(Async::Ready(None)) => {
                                Ok(Async::Ready(Some(Merged3Item::Second(item2))))
                            }
                            Ok(Async::Ready(Some(item3))) => {
                                Ok(Async::Ready(Some(Merged3Item::SecondThird(item2, item3))))
                            }
                        }
                    }
                }
            }
            Async::Ready(Some(item1)) => {
                match self.stream2.poll() {
                    Err(e) => {
                        self.queued_error = Some(e);
                        match self.stream3.poll() {
                            Err(e) => {

                                // FIXME this overwrites the stream2 error
                                self.queued_error = Some(e);
                                Ok(Async::Ready(Some(Merged3Item::First(item1))))
                            }
                            Ok(Async::NotReady) | Ok(Async::Ready(None)) => {
                                Ok(Async::Ready(Some(Merged3Item::First(item1))))
                            }
                            Ok(Async::Ready(Some(item3))) => {
                                Ok(Async::Ready(Some(Merged3Item::FirstThird(item1, item3))))
                            }
                        }
                    }
                    Ok(Async::NotReady) | Ok(Async::Ready(None)) => {
                        match self.stream3.poll() {
                            Err(e) => {
                                self.queued_error = Some(e);
                                Ok(Async::Ready(Some(Merged3Item::First(item1))))
                            }
                            Ok(Async::NotReady) | Ok(Async::Ready(None)) => {
                                Ok(Async::Ready(Some(Merged3Item::First(item1))))
                            }
                            Ok(Async::Ready(Some(item3))) => {
                                Ok(Async::Ready(Some(Merged3Item::FirstThird(item1, item3))))
                            }
                        }
                    }
                    Ok(Async::Ready(Some(item2))) => {
                        match self.stream3.poll() {
                            Err(e) => {
                                self.queued_error = Some(e);
                                Ok(Async::Ready(Some(Merged3Item::FirstSecond(item1, item2))))
                            }
                            Ok(Async::NotReady) | Ok(Async::Ready(None)) => {
                                Ok(Async::Ready(Some(Merged3Item::FirstSecond(item1, item2))))
                            }
                            Ok(Async::Ready(Some(item3))) => {
                                Ok(Async::Ready(Some(Merged3Item::All(item1, item2, item3))))
                            }
                        }
                    }
                }
            }
        }
    }
}

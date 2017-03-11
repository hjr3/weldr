use futures::stream::Stream;

mod merge3;
pub use self::merge3::{Merge3, Merged3Item};

/// An adapter for merging the output of two streams.
///
/// The merged stream produces items from one or both of the underlying
/// streams as they become available. Errors, however, are not merged: you
/// get at most one error at a time.
pub fn merge3<S1, S2, S3>(first: S1, second: S2, third: S3) -> Merge3<S1, S2, S3>
    where S1: Stream, S2: Stream<Error = S1::Error>, S3: Stream<Error = S1::Error>
{
    merge3::new(first, second, third)
}

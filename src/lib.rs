#[macro_use] extern crate log;
#[macro_use] extern crate futures;
#[macro_use] extern crate tokio_core;
#[macro_use] extern crate tokio_proto;
extern crate tokio_timer;
extern crate bytes;

// pub mod for now until the entire API is used internally
pub mod pool;
mod pipe;

use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::time::Duration;

use futures::{Future};
use futures::stream::Stream;
use tokio_core::reactor::{Core, Timeout};
use tokio_core::net::{TcpListener, TcpStream};
use pool::Pool;

pub fn new_proxy(addr: SocketAddr, backend: String) {
    let mut pool = Pool::new(vec![backend]).unwrap();

    // Create the event loop that will drive this server
    let mut lp = Core::new().unwrap();
    let handle = lp.handle();
    let h2 = handle.clone();
    let h3 = handle.clone();

    // Create a TCP listener which will listen for incoming connections
    let listener = TcpListener::bind(&addr, &handle.clone()).expect("Failed to bind to socket");

    info!("Listening on: {}", addr);

    let proxy = listener.incoming().for_each(|(sock, addr)| {
        debug!("Incoming connection on {}", addr);

        let timeout = match Timeout::new(Duration::from_millis(500), &h3) {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to create connection timeout - {}", e);
                return Ok(())
            }
        };

        // create a timeout future and map it to Err
        // this gives us future that will map to std::result::Result<_, Timeout>
        let connect_timer = timeout.map(Err);

        // TODO turn this into a pool managed by raft
        let backend = pool.get().unwrap();
        let pipe = TcpStream::connect(&backend, &h2)
            // create a stream connect future and map it to Ok
            // this gives us future that will map to std::result::Result<TcpStream, _>
            .map(Ok)

            // this now reads: "A TcpStream future calling select on a Timeout future". the select
            // combinator will return whichever future finishes first _and_ the future that has
            // not finished in the form of a tuple.
            //
            // if we had not wrapped the TcpStream in Ok and the Timeout future in Err, the
            // compiler would have complained that we were trying to return two different types.
            // instead, the select combinator can return the same result enum of
            // Ok((Result<TcpStream, Timeout>, OtherFuture)) or Err((Result<TcpStream, Timeout>, TcpStreamFuture))
            .select(connect_timer)

            // stay with me here. TcpStream itself returns io::Result, so we first need to deal
            // with TcpStream returning Ok. If it does return Ok, then we either have a TcpStream
            // or a Timeout.
            .map(|(res, _)| {
                res
                    .map(|tcp| tcp)
                    .map_err(|_timeout| {
                        Error::new(ErrorKind::TimedOut, "Pool connection timeout")
                    })
            }).map_err(|(e, _)| {
            // this is taking our `(io:Error, Timeout future)` tuple and mapping it to only be
            // `io::Error`. we do this to have a consistent type in `Err` from here on out
            e
        }).and_then(|tcp| {
            // we use this `and_then` to flatten out our result. our call to `.map` above
            // returned either `Ok` or `Err`. we want to chain the ``Ok(TcpStream)` to the next
            // step and send the `Err` down to the `map_err` at the bottom.
            //
            // if we did not do this, then our next `and_then` would have to match on the
            // result and chain the error to the next `and_then` where we could chain the error
            // again. this defeats the purpose of combinators/railroad style dev/etc
            tcp
        }).and_then(move |server| {

            // Pipe implements future. The `Pipe` is hiding away a state machine that is
            // managing non-blocking reads and writes between a client and server.
            //
            // Note: this is a bi-directional pipe. I should probably change the name so I do
            // not confuse people who assume this is a conventional uni-directional pipe.
            pipe::Pipe::new(
                addr,
                sock,
                backend,
                server
            )

        }).map(|()| {
            // the above `Pipe` implements `Future` with `Item=()`, so that is the value passed
            // to our closure. we could have also returned the total bytes proxied, the time it
            // took, etc

            debug!("Finished proxying");

            // we have to return `()`, which will create a `Future<Item=(), Error=_>` in order
            // for `handle.spawn` to work
            //
            // You can elide `()` as functions return `()` by default. Note: _elide_ is what
            // really smart CS PhD people use to confuse the general masses. The word _elide_
            // means to omit. It is used all over Rust documentation, so now you know.
            ()
        }).map_err(|e| {
            error!("Error trying proxy - {:?}", e);

            // we have to return `()`, which will create a `Future<Item=_, Error=()>` in order
            // for `handle.spawn` to work
            //
            // You can elide `()` as functions return `()` by default. Note: _elide_ is what
            // really smart CS PhD people use to confuse the general masses. The word _elide_
            // means to omit. It is used all over Rust documentation, so now you know.
            ()
        });

        // spawn expects Future<Item=(), Error=()>
        //
        // The `spawn` method adds a future to the event pool. There is no threading going on here.
        // If you are familiar with mio/epoll/kqueue, this is registering with the event loop. G
        handle.spawn(pipe);

        // The `Stream::for_each` combinator will continue passing new connections as long as we
        // return an `Ok(())`. If we return `Err`, then we will abort the loop and our error will
        // show up in the below call to `lp.run(proxy)`
        Ok(())
    });

    // The run method runs a future on the _main_ `Task`. In our case, we are running a Stream that
    // calls the `for_each` combinator. The `for_each` combinator will loop forever until it
    // receives an error. Thus, `lp.run` will only return if it receives an error.
    //
    // If you are wondering where the `Task` was introduced, calling `lp.run` creates one. You can
    // think of a task as a container for one or more futures.
    //
    // Questions:
    // * What does it mean for a future to be pinned to a stack frame? I believe it means the
    // future is owned by the run method.
    // * Based on the above, what does it mean for a non-reference to have `'static` bounds?
    lp.run(proxy).expect("Unexpected error while proxying connection");
}
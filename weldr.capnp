@0xf955e7933ca38a79;

interface Subscription {}

interface Publisher(T) {
    # A source of messages of type T.

    subscribe @0 (subscriber: Subscriber(T)) -> (subscription: Subscription);
    # Registers `subscriber` to receive published messages. Dropping the returned `subscription`
    # signals to the `Publisher` that the subscriber is no longer interested in receiving messages.
}

interface Subscriber(T) {
    pushMessage @0 (message: T) -> ();
    # Sends a message from a publisher to the subscriber. To help with flow control, the subscriber should not
    # return from this method until it is ready to process the next message.
}

struct AddBackendServerRequest {
  # A request from the manager to the workers to add a new backend server to the pool

   url @0 :Text;
   # The url of the new server to add to the pool
}

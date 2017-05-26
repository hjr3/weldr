@0xf955e7933ca38a79;

interface Subscription {}

interface Publisher(T) {
    # A source of messages of type T.

    subscribe @0 (subscriber: Subscriber(T)) -> (subscription: Subscription);
    # Registers `subscriber` to receive published messages. Dropping the returned `subscription`
    # signals to the `Publisher` that the subscriber is no longer interested in receiving messages.
}

interface Subscriber(T) {
    addServer @0 (url: Text) -> ();
    # A request from the manager to the workers to add a new backend server to the pool

    markServerDown @1 (url: Text) -> ();
    # A request from the manager to the workers mark a server as down

    markServerActive @2 (url: Text) -> ();
    # A request from the manager to the workers mark a server as down
}

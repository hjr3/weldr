extern crate hyper;

use std::thread;
use hyper::Client;

static NTHREADS: i32 = 1;

fn main() {

    for i in 0..NTHREADS {

        let _ = thread::spawn(move|| {

            loop {
                println!("thread {} - Sending HTTP request to /echo", i);

                let client = Client::new();
                let res = client
                    .get("http://127.0.0.1:5555/echo")
                    .send()
                    .unwrap();

                println!("thread {} - Received HTTP response code: {}", i, res.status);
            }
        });
    }

    loop {}
}

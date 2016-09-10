use std::io::prelude::*;
use std::net::TcpListener;

fn main() {

    let listener = TcpListener::bind("127.0.0.1:12345").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut buf = Vec::new();
                let bytes = stream.read_to_end(&mut buf).unwrap();

                println!("Read {} bytes", bytes);
                std::io::stdout().flush().unwrap();

                stream.write_all(&buf).unwrap();

                println!("Wrote {} bytes", buf.len());
                std::io::stdout().flush().unwrap();
            },
            Err(e) => { panic!("Connection failed - {}", e) }
        }
    }
}

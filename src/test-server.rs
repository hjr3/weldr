use std::io::prelude::*;
use std::net::TcpListener;

fn main() {

    let listener = TcpListener::bind("127.0.0.1:12345").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut buf = [0u8; 1024];
                let bytes = stream.read(&mut buf).unwrap();

                println!("Read {} bytes", bytes);
                std::io::stdout().flush().unwrap();

                let bytes = stream.write(&buf[0..bytes]).unwrap();

                println!("Wrote {} bytes", bytes);
                std::io::stdout().flush().unwrap();
            },
            Err(e) => { panic!("Connection failed - {}", e) }
        }
    }
}

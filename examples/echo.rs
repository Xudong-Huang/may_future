#![feature(async_await)]

use may::{coroutine, go};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_reactor::Handle;

#[cfg(unix)]
fn convert_stream(s: may::net::TcpStream) -> TcpStream {
    use std::os::unix::io::{FromRawFd, IntoRawFd};
    let raw = s.into_raw_fd();
    let std = unsafe { std::net::TcpStream::from_raw_fd(raw) };
    TcpStream::from_std(std, &Handle::default()).expect("error")
}

// FIXME: windows is not working for that read async socket would return
// invalid parameter error
#[cfg(windows)]
fn convert_stream(s: may::net::TcpStream) -> TcpStream {
    use std::os::windows::io::{FromRawSocket, IntoRawSocket};
    let raw = s.into_raw_socket();
    let std = unsafe { std::net::TcpStream::from_raw_socket(raw) };
    TcpStream::from_std(std, &Handle::default()).expect("error")
}

fn handle_client(stream: may::net::TcpStream) {
    let mut socket = convert_stream(stream);
    may_future::RT.block_on(async move {
        let mut buf = [0; 1024];

        // In a loop, read data from the socket and write the data back.
        loop {
            // windows read is not correct!
            let n = socket.read(&mut buf).await.unwrap();

            if n == 0 {
                return;
            }

            socket.write_all(&buf[0..n]).await.unwrap();
        }
    });
}

fn main() {
    may::config().set_stack_size(0x2000);
    coroutine::scope(|s| {
        for _ in 0..1 {
            go!(s, move || {
                let listener = may::net::TcpListener::bind(("0.0.0.0", 8080)).unwrap();
                for stream in listener.incoming() {
                    match stream {
                        Ok(s) => {
                            go!(move || handle_client(s));
                        }
                        Err(e) => println!("err = {:?}", e),
                    }
                }
            });
        }
    });
}

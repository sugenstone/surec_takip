use std::{
    io::{BufRead, BufReader, Write},
    net::{SocketAddr, TcpStream},
    process::ExitCode,
    time::Duration,
};

fn main() -> ExitCode {
    if check().is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn check() -> Result<(), Box<dyn std::error::Error>> {
    let address: SocketAddr = "127.0.0.1:8080".parse()?;
    let timeout = Duration::from_secs(5);
    let mut stream = TcpStream::connect_timeout(&address, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream
        .write_all(b"GET /api/v1/ready HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut status = String::new();
    BufReader::new(stream).read_line(&mut status)?;
    if status.starts_with("HTTP/1.1 200 ") {
        Ok(())
    } else {
        Err("NOT_READY".into())
    }
}

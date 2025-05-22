use std::io;
use std::io::Write;
use std::os::unix::net::UnixStream;

pub(crate) struct Notifier {
    socket: UnixStream,
}

impl Notifier {
    pub(crate) fn new() -> Result<Self, io::Error> {
        let socket = Self::connect()?;
        Ok(Self { socket })
    }

    pub(crate) fn send_message(&mut self, message: &str) -> Result<(), io::Error> {
        for _ in 0..10 {
            let result = self.socket.write_all(message.as_bytes());
            if result.is_ok() {
                return Ok(());
            } else {
                self.socket = Self::connect()?;
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
        Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "Failed to send message",
        ))
    }

    fn connect() -> Result<UnixStream, io::Error> {
        for _ in 0..10 {
            let socket = UnixStream::connect("/tmp/vpn-status.sock");
            if socket.is_ok() {
                return socket;
            } else {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Failed to connect to socket",
        ))
    }
}

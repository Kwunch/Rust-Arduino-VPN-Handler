use crate::tools::config;
use std::process::{Child, Command};

pub(crate) struct Handler {
    config: config::File,
    child: Option<Child>,
}

impl Handler {
    pub(crate) fn new() -> Result<Self, std::io::Error> {
        let config = config::File::new();
        config.init()?;

        Ok(Self {
            config,
            child: None,
        })
    }

    pub(crate) fn start(&mut self) -> Result<(), std::io::Error> {
        if self.child.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "OpenVPN is already running",
            ));
        }
        for _ in 0..10 {
            let child = Command::new("openvpn")
                .arg("--config")
                .arg(self.config.get_random_file_path()?)
                .arg("--auth-user-pass")
                .arg(self.config.get_auth())
                .spawn();
            match child {
                Ok(child) => {
                    println!("OpenVPN process started.");
                    self.child = Some(child);
                    return Ok(());
                }
                Err(_) => {
                    std::thread::sleep(std::time::Duration::from_secs(1));
                }
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Failed to start OpenVPN",
        ))
    }

    pub(crate) fn stop(&mut self) -> Result<(), std::io::Error> {
        match self.child.take() {
            Some(mut child) => {
                for _ in 0..10 {
                    match child.kill() {
                        Ok(_) => return Ok(()),
                        Err(_) => {
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                    }
                }
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to stop OpenVPN",
                ))
            }
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let handler = Handler::new();
        assert!(handler.is_ok());
    }

    #[test]
    fn test_start_and_stop() {
        //Attempt to create a handler
        let handler = Handler::new();
        assert!(handler.is_ok());

        //Attempt to start handler
        let mut handler = handler.unwrap();
        let result = handler.start();
        assert!(result.is_ok());
        assert!(handler.child.is_some());

        //Attempt to stop a handler
        let result = handler.stop();
        assert!(result.is_ok());
        assert!(handler.child.is_none());
    }

    #[test]
    fn test_stop_no_start() {
        //Attempt to create a handler
        let handler = Handler::new();
        assert!(handler.is_ok());

        //Assert handler.child is None
        let mut handler = handler.unwrap();
        assert!(handler.child.is_none());

        //Make sure stopping didnt change anything
        let result = handler.stop();
        assert!(result.is_ok());
        assert!(handler.child.is_none());
    }

    #[test]
    fn test_start_twice_no_stop() {
        //Attempt to create a handler
        let handler = Handler::new();
        assert!(handler.is_ok());

        //Attempt to start handler
        let mut handler = handler.unwrap();
        let result = handler.start();
        assert!(result.is_ok());
        assert!(handler.child.is_some());

        //Attempt to start handler again
        let result = handler.start();
        assert!(result.is_err());
        assert!(handler.child.is_some());

        //Attempt to stop the running handler
        let result = handler.stop();
        assert!(result.is_ok());
        assert!(handler.child.is_none());
    }
}

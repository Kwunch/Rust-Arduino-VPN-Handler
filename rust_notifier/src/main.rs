use notify_rust::{Notification, NotificationHandle, Timeout};
use reqwest::blocking::get;
use serde::Deserialize;
use std::io::Read;
use std::os::unix::net::UnixListener;
use std::{fs, thread};

use geolocation;

#[derive(Debug)]
enum NotifyError {
    NotifyError(notify_rust::error::Error),
    IPError(reqwest::Error),
}

type Result<T> = std::result::Result<T, NotifyError>;

impl std::fmt::Display for NotifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotifyError::NotifyError(e) => write!(f, "Notify Error: {}", e),
            NotifyError::IPError(e) => write!(f, "IP Error: {}", e),
        }
    }
}

#[derive(Deserialize, Debug)]
struct IpResponse {
    ip: String,
}

fn get_public_ip() -> Result<String> {
    let response = get("https://api.ipify.org?format=json").map_err(|e| NotifyError::IPError(e))?;

    let ip_response: IpResponse = response.json().map_err(|e| NotifyError::IPError(e))?;

    Ok(ip_response.ip)
}

fn main() {
    let socket_path = "/tmp/vpn-status.sock";
    fs::remove_file(socket_path).ok();

    let listener = match UnixListener::bind(socket_path) {
        Ok(listener) => listener,
        Err(e) => {
            // TODO Add some sort of error handling
            // TODO Til then just return
            return;
        }
    };

    /*
        TODO remove print when published and made into daemon maybe consider logging
         (probably just wont do anything about signaling that program is listening not sure)
    */
    println!("Listening on {}", socket_path);

    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(unwrapped) => unwrapped,
            Err(e) => {
                // TODO Add some sort of error handling
                // TODO Til then just return
                return;
            }
        };

        let mut buffer = [0; 1024];
        match stream.read(&mut buffer) {
            Ok(bytes_read) if bytes_read > 0 => {
                let message = String::from_utf8_lossy(&buffer[..bytes_read]);
                let status = message.trim();

                /*
                  Possible commands so far
                  STATUS Connected
                  STATUS Disconnected
                  FAIL - Error message
                */

                let notification = match status.split(" ").collect::<Vec<&str>>()[0] {
                    "STATUS" => {
                        let state = match status.split(" ").collect::<Vec<&str>>()[1] {
                            "Connected" => true,
                            "Disconnected" => false,
                            _ => {
                                // TODO add some sort of failed command handler just in case
                                // TODO Til then just return
                                println!("Invalid status: {}", status);
                                continue
                            }
                        };
                        match vpn_status_change(state) {
                            Ok(state) => state,
                            Err(e) => {
                                // TODO add some sort of failure handle
                                // TODO Til then just return
                                println!("Error {}", e);
                                continue
                            }
                        }
                    }
                    "FAIL" => {
                        let message = status.split("-").collect::<Vec<&str>>()[1..].join(" ");
                        match report_failure(&message.trim()) {
                            Ok(state) => state,
                            Err(e) => {
                                // TODO add some sort of failure handle
                                // TODO Til then just return
                                return;
                            }
                        }
                    }

                    _ => {
                        // TODO add some sort of failed command handler just in case
                        // TODO Til then just return
                        return;
                    }
                };

                println!("Notification Sent"); //TODO change this add maybe a second logger not sure

                thread::sleep(std::time::Duration::from_secs(5));
                notification.close();

                println!("Notification Closed"); // TODO Change this as well maybe with logger 
            }
            Err(e) => println!("Error: {}", e),
            _ => {}
        }
    }
}

fn vpn_status_change(status: bool) -> Result<NotificationHandle> {
    let ip = match get_public_ip() {
        Ok(ip) => ip,
        Err(e) => {
            eprintln!("Error: {}", e);
            return Err(e);
        }
    };

    let _info = geolocation::find(&*ip).unwrap();

    Notification::new()
        .summary("VPN Status")
        .body(&format!(
            "VPN {}. IP: {}",
            if status { "Connected" } else { "Disconnected" },
            ip
        ))
        .icon("system")
        .timeout(Timeout::Milliseconds(6000))
        .show()
        .map_err(|e| NotifyError::NotifyError(e))
}

fn report_failure(message: &str) -> Result<NotificationHandle> {
    Notification::new()
        .summary("VPN Handler Error")
        .body(message)
        .icon("system")
        .urgency(notify_rust::Urgency::Critical)
        .timeout(Timeout::Milliseconds(6000))
        .show()
        .map_err(|e| NotifyError::NotifyError(e))
}

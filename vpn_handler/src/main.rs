mod tools;

use crate::tools::logger::Logger;
use crate::tools::notifier::Notifier;
use serialport;
use std::io::{Error, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use std::{fs, thread};
use tools::handler;

static KILL_RUNNER: AtomicBool = AtomicBool::new(false);
const CONTROL_SOCKET_PATH: &str = "/tmp/vpn-control.sock";

fn main() {
    let logger = Arc::new(Mutex::new(Logger::new()));
    if let Err(e) = logger.lock().unwrap().update() {
        panic!("Failed to update logger: {:?}", e)
    }

    let notifier = match create_notifier() {
        Ok(notifier) => Arc::new(Mutex::new(notifier)),
        Err(e) => {
            let logger = Arc::clone(&logger);
            let msg = format!("Failed to initialize Notifier: {:?}", e);
            logger.lock().unwrap().log(&msg).unwrap();
            panic!("{}", msg)
        }
    };

    let update_logger = Arc::clone(&logger);
    let update_notifier = Arc::clone(&notifier);
    let update_thread = thread::spawn(move || {
        check_for_updates(update_logger, update_notifier);
    });

    let mut process: Option<JoinHandle<()>> = None;
    fs::remove_file(CONTROL_SOCKET_PATH).ok(); // Remove existing socket

    let listener = UnixListener::bind(CONTROL_SOCKET_PATH).expect("Failed to bind socket");

    println!("VPN Control Daemon listening...");

    for stream in listener.incoming() {
        let mut stream = stream.unwrap();
        let mut buffer = [0; 7];
        match stream.read(&mut buffer) {
            Ok(bytes_read) => {
                let command = String::from_utf8_lossy(&buffer[..bytes_read])
                    .trim()
                    .to_string();

                println!("Received command: {}!", command);

                match command.as_str() {
                    "status" => {
                        write_to_stream(
                            &mut stream,
                            if process.is_some() {
                                "Daemon is running"
                            } else {
                                "Daemon is not running"
                            },
                            &logger,
                        );
                    }
                    "start" => match &process {
                        Some(_) => {
                            write_to_stream(&mut stream, "Daemon is already running", &logger);
                        }
                        None => {
                            let notifier = Arc::clone(&notifier);
                            let closure_logger = Arc::clone(&logger);
                            process = Some(thread::spawn(move || {
                                match runner(&closure_logger, &notifier) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        let msg = format!("Runner encountered error: {:?}", e);
                                        closure_logger.lock().unwrap().log(&msg).unwrap();
                                        panic!("{}", msg);
                                    }
                                }
                                KILL_RUNNER.store(false, Ordering::Relaxed);
                            }));
                            let msg = "Daemon started".to_string();
                            if let Err(_) = logger.lock().unwrap().log(&msg) {
                                continue;
                            }
                            write_to_stream(&mut stream, &msg, &logger);
                        }
                    },
                    "stop" => {
                        KILL_RUNNER.store(true, Ordering::Relaxed);

                        if let Some(handle) = process.take() {
                            write_to_stream(&mut stream, "Killing VPN if needed...", &logger);

                            let result = handle.join();

                            write_to_stream(
                                &mut stream,
                                "Stopped listening to Arduino...",
                                &logger,
                            );

                            if let Err(e) = result {
                                write_to_stream(
                                    &mut stream,
                                    &format!(
                                        "Process threw error when terminating...\nThrown error: {:?}",
                                        e
                                    ),
                                    &logger,
                                );
                            }

                            stream.flush().unwrap();
                            if let Err(_) =
                                logger.lock().unwrap().log(&"Stopped listening".to_string())
                            {
                                continue;
                            }
                        }
                    }
                    _ => {
                        write_to_stream(&mut stream, "Received invalid command!", &logger);
                    }
                }
            }
            Err(e) => {
                write_to_stream(
                    &mut stream,
                    format!("Error reading from socket: {:?}", e).as_str(),
                    &logger,
                );
                if let Err(_) = logger
                    .lock()
                    .unwrap()
                    .log(&format!("Error reading from socket: {:?}", e))
                {
                    continue;
                }
            }
        }
    }
    if let Err(e) = update_thread.join() {
        logger
            .lock()
            .unwrap()
            .log(&format!("Failed to join update thread: {:?}", e))
            .unwrap();
    }
}

fn check_for_updates(logger: Arc<Mutex<Logger>>, notifier: Arc<Mutex<Notifier>>) {
    // Every hour check
    loop {
        thread::sleep(Duration::from_secs(3600));
        {
            let mut logger = logger.lock().unwrap();
            match logger.update() {
                Ok(_) => {}
                Err(e) => {
                    let msg = format!("Failed to update logger: {:?}", e);
                    if let Err(_) = logger.log(&msg) {
                        continue;
                    }
                    let mut notifier = notifier.lock().unwrap();
                    if let Err(_) = notifier.send_message(&format!("FAIL - {}", msg)) {
                        continue;
                    }
                }
            }
        }
    }
}

fn create_notifier() -> Result<Notifier, Error> {
    let mut attempt = 0;

    while attempt < 10 {
        match Notifier::new() {
            Ok(success) => return Ok(success),
            Err(_) => {
                thread::sleep(Duration::from_millis(250));
                attempt += 1;
                continue;
            }
        }
    }
    Err(Error::new(
        std::io::ErrorKind::Other,
        "Failed to initialize Notifier after 10 attempts",
    ))
}

fn write_to_stream(stream: &mut UnixStream, message: &str, logger: &Arc<Mutex<Logger>>) {
    let mut attempt = 0;
    while attempt <= 5 {
        match writeln!(stream, "{}\n", message) {
            Ok(_) => {
                break;
            }
            Err(_) if attempt < 5 => {
                thread::sleep(Duration::from_millis(50)); // Small delay before retry
                attempt += 1;
                continue;
            }
            Err(e) => {
                let msg = format!("Failed to write to stream: {:?}", e);
                logger.lock().unwrap().log(&msg).unwrap();
            }
        }
    }
}

fn runner(
    logger: &Arc<Mutex<Logger>>,
    notifier: &Arc<Mutex<Notifier>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let port_name = "/dev/ttyACM0";
    let settings = serialport::new(port_name, 57600).timeout(Duration::from_secs(10));

    let mut port = settings.open()?;

    let mut handler = handler::Handler::new()?;

    let mut previous_command: u8 = 0;
    loop {
        if KILL_RUNNER.load(Ordering::Relaxed) {
            // Check KILL flag safely
            return match handler.stop() {
                Ok(_) => Ok(()),
                Err(e) => Err(Box::new(e)),
            };
        }
        let mut buffer = [0; 9];
        match port.read(&mut buffer) {
            Ok(bytes_read) if bytes_read > 0 => {
                let message = String::from_utf8_lossy(&buffer[0..bytes_read])
                    .trim()
                    .to_string();

                match message.as_str() {
                    "Turn On" => {
                        if previous_command != 255 {
                            println!("Turning VPN On");
                            previous_command = 255;
                            handler.start()?;
                            thread::sleep(Duration::from_secs(10));
                            {
                                let mut notifier = notifier.lock().unwrap();
                                notifier.send_message("STATUS Connected")?;
                                let msg = "VPN STATUS CHANGE: Connected".to_string();
                                if let Err(_) = logger.lock().unwrap().log(&msg) {
                                    continue;
                                }
                            }
                        }
                    }
                    "Turn Off" => {
                        if previous_command != 0 {
                            println!("Turning VPN Off");
                            previous_command = 0;
                            handler.stop()?;
                            thread::sleep(Duration::from_secs(5));
                            {
                                let mut notifier = notifier.lock().unwrap();
                                notifier.send_message("STATUS Disconnected")?;
                                let msg = "VPN STATUS CHANGE: Disconnected".to_string();
                                if let Err(_) = logger.lock().unwrap().log(&msg) {
                                    continue;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => return Err(Box::new(e)),
            _ => {}
        }
    }
}

use chrono::{Duration, Local, NaiveDateTime, ParseError as TimeParseError};
use std::cell::RefCell;
use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::{fs, io};

thread_local! {
    static TEST_LOG_PATH: RefCell<Option<String>> = RefCell::new(None);
}

#[derive(Debug)]
pub(crate) enum ParseError {
    ParseError(TimeParseError),
    MissingPrefixError,
}

#[derive(Debug)]
pub enum LoggerError {
    IOError(io::Error),
    DateTimeParseError(ParseError),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseError::ParseError(err) => write!(f, "{}", err),
            ParseError::MissingPrefixError => write!(f, "Missing DateTime Prefix"),
        }
    }
}

impl std::fmt::Display for LoggerError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LoggerError::IOError(err) => write!(f, "I/O Error: {}", err),
            LoggerError::DateTimeParseError(err) => {
                write!(f, "Error parsing NaiveDateTime: {}", err)
            }
        }
    }
}

impl From<io::Error> for LoggerError {
    fn from(err: io::Error) -> Self {
        LoggerError::IOError(err)
    }
}

#[derive(Debug)]
pub(crate) struct Logger {
    timestamp: NaiveDateTime,
}

impl Logger {
    const LOG_PATH: &'static str = "/home/kwunch/Documents/Rust/vpn_handler/log.txt";

    fn log_path() -> String {
        TEST_LOG_PATH.with(|p| {
            p.borrow()
                .clone()
                .unwrap_or_else(|| Self::LOG_PATH.to_string())
        })
    }

    pub(crate) fn new() -> Self {
        let logger = Self {
            timestamp: Local::now().naive_local(),
        };
        logger
    }

    pub(crate) fn update(&mut self) -> Result<(), LoggerError> {
        if self.rotate_needed()? {
            self.rotate_logs()?;
        }
        Ok(())
    }

    fn rotate_needed(&mut self) -> Result<bool, LoggerError> {
        let file = fs::File::open(Self::log_path());

        match file {
            Ok(file) => {
                // Read the first line
                let first_line = io::BufReader::new(file).lines().next().ok_or_else(|| {
                    LoggerError::IOError(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "Log File Is Empty",
                    ))
                })??;

                // Extract timestamp
                let timestamp_str =
                    first_line.strip_prefix("LOG CREATED AT: ").ok_or_else(|| {
                        LoggerError::DateTimeParseError(ParseError::MissingPrefixError)
                    })?;

                // Parse Timestamp
                let timestamp =
                    match NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S") {
                        Ok(timestamp) => timestamp,
                        Err(err) => {
                            return Err(LoggerError::DateTimeParseError(ParseError::ParseError(
                                err,
                            )));
                        }
                    };

                // Compare Timestamp
                let now = Local::now().naive_local();
                if now.signed_duration_since(timestamp) > Duration::hours(24) {
                    // If 'now - timestamp' is > 24 hours return true to get a new file
                    Ok(true)
                } else {
                    // If it's not assign Timestamp and return false, so no new file is made
                    self.timestamp = timestamp;
                    Ok(false)
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // Return true so rotate_logs creates a new file
                Ok(true)
            }
            Err(e) => Err(LoggerError::IOError(e)),
        }
    }

    fn rotate_logs(&mut self) -> Result<(), LoggerError> {
        // Get the current Timestamp for the new file
        let now = Local::now().naive_local();

        // Ensure old log removal doesn't cause unnecessary errors
        if fs::metadata(Self::log_path()).is_ok() {
            fs::remove_file(Self::log_path()).map_err(LoggerError::IOError)?;
        }

        // Create the new log file with Timestamp at the first line
        let contents = format!("LOG CREATED AT: {}\n", now.format("%Y-%m-%d %H:%M:%S"));
        fs::write(Self::log_path(), contents).map_err(LoggerError::IOError)?;

        // Update stored timestamp
        self.timestamp = now;

        Ok(())
    }

    pub(crate) fn log(&self, msg: &String) -> Result<(), LoggerError> {
        let now = Local::now().naive_local();

        let mut file = OpenOptions::new()
            .append(true)
            .open(Self::log_path())
            .map_err(LoggerError::IOError)?;

        let msg = format!("[{}] > {}\n", now.format("%Y-%m-%d %H:%M:%S"), msg);

        match file.write_all(msg.as_bytes()) {
            Ok(_) => Ok(()),
            Err(e) => Err(LoggerError::IOError(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDateTime, TimeDelta};
    use tempfile::NamedTempFile;

    fn setup_log_path() -> NamedTempFile {
        // Create a temporary path and assert the result
        let log_path = NamedTempFile::new_in("/home/kwunch/Documents/Rust/vpn_handler");
        assert!(log_path.is_ok());
        let log_path = log_path.unwrap();

        // Set the test path as the LOG_PATH in the main program and assert the change
        TEST_LOG_PATH
            .with(|p| *p.borrow_mut() = Some(log_path.path().to_str().unwrap().to_string()));
        assert_eq!(Logger::log_path(), log_path.path().to_str().unwrap());

        // Remove the log file if it exists
        if log_path.path().exists() {
            let result = fs::remove_file(log_path.path());
            assert!(
                result.is_ok(),
                "Failed to delete log file! Error: {}",
                result.unwrap_err()
            );
        }

        log_path
    }

    #[test]
    fn test_rotate_needed_no_file() {
        let _log_path = setup_log_path();

        // Create the logger
        let mut logger = Logger::new();

        // Run rotate needed (should return Ok(true))
        let result = logger.rotate_needed();
        assert!(
            result.is_ok(),
            "Rotate needed returned error! Error: {}",
            result.unwrap_err()
        );
        assert_eq!(result.unwrap(), true, "Rotate needed returned false!");
    }

    #[test]
    fn test_rotate_needed_old_file() {
        let log_path = setup_log_path();

        // Create a new file with a timestamp older than 24 hours
        let outdated_timestamp = (Local::now() - Duration::hours(25))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        // Attempt to Write the outdated Timestamp to File.
        let result = fs::write(
            log_path.path(),
            format!("LOG CREATED AT: {}", outdated_timestamp),
        );
        assert!(
            result.is_ok(),
            "Failed to write outdated timestamp to log file! Error: {}",
            result.unwrap_err()
        );

        // Create the logger
        let mut logger = Logger::new();

        // Run rotate needed, then assert it's ok and returns true
        let result = logger.rotate_needed();
        assert!(
            result.is_ok(),
            "Result Returned Error  -> {} and TempFilePath -> {}",
            result.err().unwrap(),
            log_path.path().to_str().unwrap()
        );
        assert_eq!(result.unwrap(), true);

        let result = log_path.close();
        assert!(
            result.is_ok(),
            "Failed to close log file! Error: {}",
            result.unwrap_err()
        );
    }

    #[test]
    fn test_rotate_needed_new_file() {
        let log_path = setup_log_path();

        // Create a fresh timestamp older than 24 hours
        let fresh_timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

        // Attempt to Write to File. We use write here unlike the actual rotate function for simplicity
        let result = fs::write(
            log_path.path(),
            format!("LOG CREATED AT: {}", fresh_timestamp),
        );
        assert!(
            result.is_ok(),
            "Failed to write fresh timestamp to log file! Error: {}",
            result.unwrap_err()
        );

        // Create the Logger
        let mut logger = Logger::new();

        // Run rotate needed, then assert it's ok and returns false
        let result = logger.rotate_needed();
        assert!(
            result.is_ok(),
            "Result Returned Error  -> {} and TempFilePath -> {}",
            result.err().unwrap(),
            log_path.path().to_str().unwrap()
        );
        assert_eq!(result.unwrap(), false);

        let result = log_path.close();
        assert!(
            result.is_ok(),
            "Failed to close log file! Error: {}",
            result.unwrap_err()
        );
    }

    #[test]
    fn test_rotate_logs_no_file() {
        let log_path = setup_log_path();

        // Create the logger
        let mut logger = Logger::new();

        // Run Rotate Needed and Assert Result it Ok and True
        let result = logger.rotate_needed();
        assert!(
            result.is_ok(),
            "Failed to run rotate needed! Error: {}",
            result.unwrap_err()
        );
        assert_eq!(result.unwrap(), true);

        // Run rotate_logs to create the file
        let result = logger.rotate_logs();
        assert!(
            result.is_ok(),
            "Failed to run rotate logs! Error: {}",
            result.unwrap_err()
        );

        // Check to make sure the logfile now exists, and its time stamp is within 5 seconds
        if log_path.path().exists() {
            let file = fs::File::open(log_path.path());
            assert!(
                file.is_ok(),
                "Failed to open log file! Error: {}",
                file.unwrap_err()
            );
            let file = file.unwrap();

            // Read the First Line
            let first_line = io::BufReader::new(file).lines().next().ok_or_else(|| {
                LoggerError::IOError(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Log File Is Empty",
                ))
            });

            // Assert twice that the first line is Ok
            assert!(
                first_line.is_ok(),
                "Failed to read first line of log file! 1st Error: {}",
                first_line.unwrap_err()
            );
            let first_line = first_line.unwrap();
            assert!(
                first_line.is_ok(),
                "Failed to read first line of log file! 2nd Error: {}",
                first_line.unwrap_err()
            );
            let first_line = first_line.unwrap();

            // Extract the timestamp and assert that it's ok
            let timestamp_str = first_line
                .strip_prefix("LOG CREATED AT: ")
                .ok_or_else(|| LoggerError::DateTimeParseError(ParseError::MissingPrefixError));
            assert!(timestamp_str.is_ok());

            // Unwrap Timestamp and parse it then asserting that it's ok
            let timestamp = timestamp_str.unwrap();
            let timestamp = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S");
            assert!(timestamp.is_ok());

            // Unwrap it and create a new var now to use for measuring time
            let timestamp = timestamp.unwrap();
            let now = Local::now().naive_local();

            // Assert that now - timestamp < 1 seconds apart
            let total_time_passed = now.signed_duration_since(timestamp);
            assert!(
                total_time_passed > TimeDelta::seconds(0),
                "Total time passed is {}",
                total_time_passed
            );
            assert!(total_time_passed < Duration::seconds(1));

            let result = log_path.close();
            assert!(
                result.is_ok(),
                "Failed to close log file! Error: {}",
                result.unwrap_err()
            );
        }
    }

    #[test]
    fn test_rotate_logs_with_old_file() {
        let log_path = setup_log_path();

        // Create a new file with a timestamp older than 24 hours
        let outdated_timestamp = (Local::now() - Duration::hours(25))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        // Attempt to Write to File. We use write here unlike the actual rotate function for simplicity
        let result = fs::write(
            log_path.path(),
            format!("LOG CREATED AT: {}", outdated_timestamp),
        );
        assert!(
            result.is_ok(),
            "Failed to write outdated timestamp to log file! Error: {}",
            result.unwrap_err()
        );

        // Check if logger is ok
        let mut logger = Logger::new();

        // Run rotate needed, then assert it's ok and returns true
        let result = logger.rotate_needed();
        assert!(
            result.is_ok(),
            "Result Returned Error  -> {} and TempFilePath -> {}",
            result.err().unwrap(),
            log_path.path().to_str().unwrap()
        );
        assert_eq!(result.unwrap(), true);

        // Run rotate_logs to create the file
        let result = logger.rotate_logs();
        assert!(
            result.is_ok(),
            "Failed to run rotate logs! Error: {}",
            result.unwrap_err()
        );

        let file = fs::File::open(log_path.path());
        assert!(
            file.is_ok(),
            "Failed to open log file! Error: {}",
            file.unwrap_err()
        );
        let file = file.unwrap();

        // Read the First Line
        let first_line = io::BufReader::new(file).lines().next().ok_or_else(|| {
            LoggerError::IOError(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Log File Is Empty",
            ))
        });

        // Assert twice that the first line is Ok
        assert!(
            first_line.is_ok(),
            "Failed to read first line of log file! 1st Error: {}",
            first_line.unwrap_err()
        );
        let first_line = first_line.unwrap();
        assert!(
            first_line.is_ok(),
            "Failed to read first line of log file! 2nd Error: {}",
            first_line.unwrap_err()
        );
        let first_line = first_line.unwrap();

        // Extract the timestamp and assert that it's ok
        let timestamp_str = first_line
            .strip_prefix("LOG CREATED AT: ")
            .ok_or_else(|| LoggerError::DateTimeParseError(ParseError::MissingPrefixError));
        assert!(timestamp_str.is_ok());

        // Unwrap Timestamp and parse it then asserting that it's ok
        let timestamp = timestamp_str.unwrap();
        let timestamp = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S");
        assert!(timestamp.is_ok());

        // Unwrap it and create a new var now to use for measuring time
        let timestamp = timestamp.unwrap();
        let now = Local::now().naive_local();

        // Assert that now - timestamp < 1 seconds apart
        let total_time_passed = now.signed_duration_since(timestamp);
        assert!(
            total_time_passed < Duration::seconds(1),
            "Total time passed is {}",
            total_time_passed
        );

        let result = log_path.close();
        assert!(
            result.is_ok(),
            "Failed to close log file! Error: {}",
            result.unwrap_err()
        );
    }

    #[test]
    fn test_logger_log() {
        let log_path = setup_log_path();

        // Create the logger
        let mut logger = Logger::new();

        // Run Rotate Needed and Assert Result it Ok and True
        let result = logger.rotate_needed();
        assert!(
            result.is_ok(),
            "Failed to run rotate needed! Error: {}",
            result.unwrap_err()
        );
        assert_eq!(result.unwrap(), true);

        // Run rotate_logs to create the file
        let result = logger.rotate_logs();
        assert!(
            result.is_ok(),
            "Failed to run rotate logs! Error: {}",
            result.unwrap_err()
        );

        // Check that a log file now exists, if so, open it and assert that it's ok
        assert!(
            log_path.path().exists(),
            "Log file does not exist! Path: {}",
            log_path.path().to_str().unwrap()
        );
        let file = fs::File::open(log_path.path());
        assert!(
            file.is_ok(),
            "Failed to open log file! Error: {}",
            file.unwrap_err()
        );
        let file = file.unwrap();

        // Write to the log
        assert!(logger.log(&"Test Message".to_string()).is_ok());

        // Read the second line
        let second_line = io::BufReader::new(file).lines().nth(1).ok_or_else(|| {
            LoggerError::IOError(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Log File Is Empty",
            ))
        });

        assert!(
            second_line.is_ok(),
            "Failed to read second line of log file! 1st Error: {}",
            second_line.unwrap_err()
        );
        let second_line = second_line.unwrap();
        assert!(
            second_line.is_ok(),
            "Failed to read second line of log file! 2nd Error: {}",
            second_line.unwrap_err()
        );
        let second_line = second_line.unwrap();

        // Split the second line by ">" and assert that it's length is 2
        let second_line = second_line.split(">").collect::<Vec<&str>>();
        assert_eq!(second_line.len(), 2);

        // Assert that the first of the two parts is the timestamp
        let mut timestamp = second_line[0].trim().to_string();
        timestamp.remove(0);
        timestamp.remove(timestamp.len() - 1);
        let timestamp = NaiveDateTime::parse_from_str(&timestamp, "%Y-%m-%d %H:%M:%S");
        assert!(timestamp.is_ok());
        let timestamp = timestamp.unwrap();

        // Assert timestamp was created within 5 seconds of now
        let now = Local::now().naive_local();
        let total_time_passed = now.signed_duration_since(timestamp);
        assert!(total_time_passed < Duration::seconds(5));

        // Read the second part of the line and assert that it's "Test Message"
        let message = second_line[1].trim();
        assert_eq!(message, "Test Message");

        // Close the file
        let result = log_path.close();
        assert!(
            result.is_ok(),
            "Failed to close log file! Error: {}",
            result.unwrap_err()
        );
    }

    #[test]
    fn test_log_rotate_log() {
        let log_path = setup_log_path();

        // Create a new file with a timestamp older than 24 hours
        let outdated_timestamp = (Local::now() - Duration::hours(25))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        // Attempt to Write to File. We use write here unlike the actual rotate function for simplicity
        let result = fs::write(
            log_path.path(),
            format!("LOG CREATED AT: {}\n", outdated_timestamp),
        );
        assert!(
            result.is_ok(),
            "Failed to write outdated timestamp to log file! Error: {}",
            result.unwrap_err()
        );

        // Add an hour to the timestamp
        let old_original_timestamp =
            NaiveDateTime::parse_from_str(&outdated_timestamp, "%Y-%m-%d %H:%M:%S");
        assert!(
            old_original_timestamp.is_ok(),
            "Failed to parse timestamp! Error: {}",
            old_original_timestamp.unwrap_err()
        );
        let old_original_timestamp = old_original_timestamp;
        assert!(
            old_original_timestamp.is_ok(),
            "Failed to parse timestamp! Error: {}",
            old_original_timestamp.unwrap_err()
        );
        let old_original_timestamp = old_original_timestamp.unwrap();
        let new_timestamp = old_original_timestamp + Duration::hours(1);
        let new_timestamp = new_timestamp.format("%Y-%m-%d %H:%M:%S").to_string();

        // Open the file to append and assert that it's ok
        let file = OpenOptions::new()
            .append(true)
            .open(log_path.path())
            .map_err(LoggerError::IOError);
        assert!(file.is_ok());
        let mut file = file.unwrap();

        // Write the new timestamp to the file with "Test Message"
        let result = file.write_all(format!("[{}] > Test Message\n", new_timestamp).as_bytes());
        assert!(
            result.is_ok(),
            "Failed to write to log file! Error: {}",
            result.unwrap_err()
        );

        // Create the logger
        let mut logger = Logger::new();

        // Run Rotate Needed and Assert Result it Ok and True
        let result = logger.rotate_needed();
        assert!(
            result.is_ok(),
            "Failed to run rotate needed! Error: {}",
            result.unwrap_err()
        );
        let result = result.unwrap();
        assert_eq!(result, true);

        // Run rotates logs to create a new file with a new timestamp
        let result = logger.rotate_logs();
        assert!(
            result.is_ok(),
            "Failed to run rotate logs! Error: {}",
            result.unwrap_err()
        );

        // Check that a log file now exists, if so, open it and assert that it's ok
        assert!(
            log_path.path().exists(),
            "Log file does not exist! Path: {}",
            log_path.path().to_str().unwrap()
        );
        let file = fs::File::open(log_path.path());
        assert!(
            file.is_ok(),
            "Failed to open log file! Error: {}",
            file.unwrap_err()
        );
        let file = file.unwrap();

        // Read the First Line
        let first_line = io::BufReader::new(file).lines().next().ok_or_else(|| {
            LoggerError::IOError(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Log File Is Empty",
            ))
        });

        // Assert twice that the first line is Ok
        assert!(
            first_line.is_ok(),
            "Failed to read first line of log file! 1st Error: {}",
            first_line.unwrap_err()
        );
        let first_line = first_line.unwrap();
        assert!(
            first_line.is_ok(),
            "Failed to read first line of log file! 2nd Error: {}",
            first_line.unwrap_err()
        );
        let first_line = first_line.unwrap();

        // Extract the timestamp and assert that it's ok
        let timestamp_str = first_line
            .strip_prefix("LOG CREATED AT: ")
            .ok_or_else(|| LoggerError::DateTimeParseError(ParseError::MissingPrefixError));
        assert!(timestamp_str.is_ok());

        // Unwrap Timestamp and parse it then asserting that it's ok
        let timestamp = timestamp_str.unwrap();
        let timestamp = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S");
        assert!(timestamp.is_ok());

        // Unwrap it and create a new var now to use for measuring time
        let timestamp = timestamp.unwrap();
        let now = Local::now().naive_local();

        // Assert that now - timestamp < 1 seconds apart
        let total_time_passed = now.signed_duration_since(timestamp);
        assert!(
            total_time_passed < Duration::seconds(1),
            "Total time passed is {}",
            total_time_passed
        );

        // Write to the log
        assert!(logger.log(&"Test Message".to_string()).is_ok());

        // Reopen the file to read the new log
        let file = fs::File::open(log_path.path());
        assert!(
            file.is_ok(),
            "Failed to open log file! Error: {}",
            file.unwrap_err()
        );
        let file = file.unwrap();

        // Read the second line
        let second_line = io::BufReader::new(file).lines().nth(1).ok_or_else(|| {
            LoggerError::IOError(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Log File Is Empty",
            ))
        });

        assert!(
            second_line.is_ok(),
            "Failed to read second line of log file! 1st Error: {}",
            second_line.unwrap_err()
        );
        let second_line = second_line.unwrap();
        assert!(
            second_line.is_ok(),
            "Failed to read second line of log file! 2nd Error: {}",
            second_line.unwrap_err()
        );
        let second_line = second_line.unwrap();

        // Split the second line by ">" and assert that it's length is 2
        let second_line = second_line.split(">").collect::<Vec<&str>>();
        assert_eq!(second_line.len(), 2);

        // Assert that the first of the two parts is the timestamp
        let mut timestamp = second_line[0].trim().to_string();
        timestamp.remove(0);
        timestamp.remove(timestamp.len() - 1);
        let timestamp = NaiveDateTime::parse_from_str(&timestamp, "%Y-%m-%d %H:%M:%S");
        assert!(timestamp.is_ok());
        let timestamp = timestamp.unwrap();

        // Assert timestamp was created within 5 seconds of now
        let now = Local::now().naive_local();
        let total_time_passed = now.signed_duration_since(timestamp);
        assert!(total_time_passed < Duration::seconds(5));

        // Read the second part of the line and assert that it's "Test Message"
        let message = second_line[1].trim();
        assert_eq!(message, "Test Message");

        // Close the file
        let result = log_path.close();
        assert!(
            result.is_ok(),
            "Failed to close log file! Error: {}",
            result.unwrap_err()
        );
    }
}

use rand::Rng;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

pub(crate) struct File {
    files: Mutex<Vec<String>>,
    auth: String,
    main_dir: String,
}

impl File {
    pub(crate) fn new() -> Self {
        let main_dir = "/home/kwunch/VPN".to_string();
        let auth = "/home/kwunch/VPN/auth.txt".to_string();
        Self {
            files: Mutex::new(Vec::new()),
            auth,
            main_dir,
        }
    }

    pub(crate) fn init(&self) -> Result<(), std::io::Error> {
        let main_dir = Path::new(&self.main_dir);
        self.recurse_dir(main_dir)?;
        Ok(())
    }

    pub(crate) fn get_auth(&self) -> &String {
        &self.auth
    }

    pub(crate) fn get_random_file_path(&self) -> Result<String, std::io::Error> {
        {
            let mut file = self.lock_file()?;
            let ridx = rand::rng().random_range(0..file.len());
            let file = &mut file[ridx];
            Ok(file.to_string())
        }
    }

    fn recurse_dir(&self, path: &Path) -> Result<(), std::io::Error> {
        //TODO Add multithreading
        for entry in path.read_dir()? {
            let entry = entry?;
            if entry.file_name() == "auth.txt" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                self.recurse_dir(&path)?;
            } else {
                {
                    let mut file = self.lock_file()?;
                    match path.to_str() {
                        Some(path) => file.push(path.to_string()),
                        None => {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "Failed to convert path to string",
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn lock_file(&self) -> Result<MutexGuard<Vec<String>>, std::io::Error> {
        match self.files.lock() {
            Ok(file) => Ok(file),
            Err(_) => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to lock file",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let file = File::new();
        let result = file.init();
        assert!(result.is_ok());
    }

    #[test]
    fn test_file_count() {
        let file = File::new();
        let result = file.init();
        assert!(result.is_ok());
        assert_eq!(file.files.lock().unwrap().len(), 458);
    }

    #[test]
    fn test_is_valid_path() {
        let file = File::new();
        let result = file.init();
        assert!(result.is_ok());
        let path = file.get_random_file_path();
        assert!(path.is_ok());
        let path = path.unwrap();
        assert!(Path::new(&path).exists());
    }

    #[test]
    fn test_lock_file() {
        let file = File::new();
        let result = file.init();
        assert!(result.is_ok());
        let result = file.lock_file();
        assert!(result.is_ok());
    }
}

use anyhow::Result;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct CsvLogger {
    inner: Arc<Mutex<File>>,
}

impl CsvLogger {
    pub fn new(path: &str, header: &str) -> Result<Self> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;

        writeln!(file, "{header}")?;

        Ok(Self {
            inner: Arc::new(Mutex::new(file)),
        })
    }

    pub fn log_line(&self, line: &str) {
        if let Ok(mut file) = self.inner.lock() {
            let _ = writeln!(file, "{line}");
        }
    }
}

#[derive(Clone)]
pub struct LogBundle {
    pub telemetry: CsvLogger,
    pub command: CsvLogger,
    pub fault: CsvLogger,
}
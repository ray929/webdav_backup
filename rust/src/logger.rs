use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use tracing_subscriber::fmt::time::LocalTime;

#[derive(Debug)]
struct FileWriter(Mutex<std::fs::File>);

impl FileWriter {
    fn new(path: &str) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self(Mutex::new(file)))
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FileWriter {
    type Writer = FileWriterGuard<'a>;
    fn make_writer(&'a self) -> Self::Writer {
        FileWriterGuard(self.0.lock().unwrap())
    }
}

struct FileWriterGuard<'a>(std::sync::MutexGuard<'a, std::fs::File>);

impl Write for FileWriterGuard<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        (&mut *self.0).write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        (&mut *self.0).flush()
    }
}

pub fn init(log_level: Option<&str>, background: bool) -> Result<()> {
    let filter = if let Some(level) = log_level {
        EnvFilter::new(level)
    } else {
        EnvFilter::from_default_env()
            .add_directive("webdav-backup=info".parse().unwrap())
    };

    let layer = fmt::layer()
        .with_target(false)
        .with_level(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_timer(LocalTime::rfc_3339())
        .compact();

    if background {
        let writer = FileWriter::new("webdav-backup.log")?;
        tracing_subscriber::registry()
            .with(filter)
            .with(layer.with_writer(writer))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(layer)
            .init();
    }

    Ok(())
}

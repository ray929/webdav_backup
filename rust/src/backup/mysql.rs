use crate::config::MySqlConfig;
use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use zip::write::ZipWriter;
use zip::{AesMode, CompressionMethod};

pub async fn backup(config: &MySqlConfig, zip_path: &Path, password: Option<&str>) -> Result<()> {
    let mut cmd = Command::new("mysqldump");
    cmd.arg(format!("--host={}", config.host))
        .arg(format!("--port={}", config.port))
        .arg(format!("--user={}", config.username))
        .arg(format!("--password={}", config.password))
        .arg("--single-transaction")
        .arg("--routines")
        .arg("--triggers");

    if let Some(ref ssl_mode) = config.ssl_mode {
        cmd.arg(format!("--ssl-mode={}", ssl_mode));
    }

    cmd.arg(&config.database);

    if let Some(ref tables) = config.tables {
        if tables != "*" {
            for table in tables.split_whitespace() {
                cmd.arg(table);
            }
        }
    }

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("mysqldump failed: {}", stderr);
    }

    let sql = output.stdout;
    let zip_file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(zip_file);

    let filename = format!(
        "{}_{}.sql",
        config.database,
        chrono::Local::now().format("%Y%m%d_%H%M%S")
    );
    if let Some(password) = password {
        let options = zip::write::FileOptions::<'_, ()>::default()
            .compression_method(CompressionMethod::Deflated)
            .with_aes_encryption(AesMode::Aes256, password);
        zip.start_file(filename, options)?;
    } else {
        let options = zip::write::FileOptions::<'_, ()>::default()
            .compression_method(CompressionMethod::Deflated);
        zip.start_file(filename, options)?;
    }
    zip.write_all(&sql)?;
    zip.finish()?;

    Ok(())
}

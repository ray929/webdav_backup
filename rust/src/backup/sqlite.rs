use crate::config::SqliteConfig;
use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use tempfile::NamedTempFile;
use tokio::process::Command;
use zip::write::ZipWriter;
use zip::{AesMode, CompressionMethod};

pub async fn backup(
    config: &SqliteConfig,
    sqlite3_path: &str,
    zip_path: &Path,
    password: Option<&str>,
) -> Result<()> {
    let zip_file = fs::File::create(zip_path)?;
    let mut zip = ZipWriter::new(zip_file);

    for db_path in &config.database {
        let db_path = Path::new(db_path);
        if !db_path.exists() {
            anyhow::bail!("SQLite database not found: {}", db_path.display());
        }

        // Use NamedTempFile and keep() to prevent auto-deletion
        let temp = NamedTempFile::new()?;
        let (_, temp_path) = temp.keep()?;

        // Run: sqlite3 <db> ".backup '<temp>'"
        let output = Command::new(sqlite3_path)
            .arg(db_path)
            .arg(format!(".backup '{}'", temp_path.display()))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Clean up temp file before returning error
            let _ = fs::remove_file(&temp_path);
            anyhow::bail!(
                "sqlite3 backup failed for '{}': {}",
                db_path.display(),
                stderr
            );
        }

        let backup_data = fs::read(&temp_path)?;
        let _ = fs::remove_file(&temp_path);

        let filename = db_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "backup.db".to_string());

        if let Some(pwd) = password {
            let options = zip::write::FileOptions::<'_, ()>::default()
                .compression_method(CompressionMethod::Deflated)
                .with_aes_encryption(AesMode::Aes256, pwd);
            zip.start_file(filename, options)?;
        } else {
            let options = zip::write::FileOptions::<'_, ()>::default()
                .compression_method(CompressionMethod::Deflated);
            zip.start_file(filename, options)?;
        }
        zip.write_all(&backup_data)?;
    }

    zip.finish()?;
    Ok(())
}

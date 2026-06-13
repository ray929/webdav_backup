use crate::config::{parse_db_table, PgSqlConfig};
use anyhow::Result;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;
use zip::write::ZipWriter;
use zip::{AesMode, CompressionMethod};

pub async fn backup(config: &PgSqlConfig, dump_path: &str, zip_path: &Path, password: Option<&str>) -> Result<()> {
    let mut all_sql = Vec::new();

    for entry in &config.database {
        let (db_name, table) = parse_db_table(entry);

        let mut cmd = Command::new(dump_path);
        cmd.arg(format!("--host={}", config.host))
            .arg(format!("--port={}", config.port))
            .arg(format!("--username={}", config.username))
            .arg(format!("--dbname={}", db_name))
            .arg("--no-password")
            .arg("--clean")
            .arg("--if-exists");

        if let Some(table) = table {
            cmd.arg("--table").arg(table);
        }

        if let Some(ref ssl_mode) = config.ssl_mode {
            cmd.env("PGSSLMODE", ssl_mode);
        }

        cmd.env("PGPASSWORD", &config.password);

        let output = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("pg_dump failed for '{}': {}", entry, stderr);
        }

        all_sql.extend_from_slice(&output.stdout);
    }

    let zip_file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(zip_file);

    let filename = format!(
        "backup_{}.sql",
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
    zip.write_all(&all_sql)?;
    zip.finish()?;

    Ok(())
}

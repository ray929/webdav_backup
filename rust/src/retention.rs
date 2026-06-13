use crate::webdav::WebDavClient;
use anyhow::Result;
use chrono::NaiveDateTime;
use tracing::{debug, info, warn};

pub struct RetentionPolicy;

impl RetentionPolicy {
    pub async fn apply(
        client: &WebDavClient,
        remote_dir: &str,
        prefix: &str,
        retain_count: usize,
    ) -> Result<()> {
        if retain_count == 0 {
            debug!("retain_count is 0, skipping cleanup");
            return Ok(());
        }

        let files = client.list(remote_dir).await?;

        let mut backups: Vec<(String, NaiveDateTime)> = Vec::new();

        for file in files {
            let name = file.trim_start_matches('/');
            if !name.starts_with(prefix) || !name.ends_with(".zip") {
                continue;
            }
            if let Some(dt) = parse_timestamp(name) {
                backups.push((file, dt));
            }
        }

        if backups.len() <= retain_count {
            debug!(
                "found {} backups, retain_count is {}, no cleanup needed",
                backups.len(),
                retain_count
            );
            return Ok(());
        }

        backups.sort_by(|a, b| b.1.cmp(&a.1));

        let to_delete = &backups[retain_count..];
        info!(
            "retaining {} newest backups, deleting {} old ones",
            retain_count,
            to_delete.len()
        );

        for (file, dt) in to_delete {
            info!("deleting old backup: {} ({})", file, dt);
            if let Err(e) = client.delete(&format!("{}/{}", remote_dir, file)).await {
                warn!("failed to delete {}: {}", file, e);
            }
        }

        Ok(())
    }
}

fn parse_timestamp(filename: &str) -> Option<NaiveDateTime> {
    let stem = filename.strip_suffix(".zip")?;
    let parts: Vec<&str> = stem.split('_').collect();
    if parts.len() < 3 {
        return None;
    }
    let date = parts[parts.len() - 2];
    let time = parts[parts.len() - 1];
    NaiveDateTime::parse_from_str(&format!("{} {}", date, time), "%Y%m%d %H%M%S").ok()
}

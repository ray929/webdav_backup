pub mod file;
pub mod mysql;
pub mod pgsql;

use crate::config::{BackupProject, Config};
use crate::retention::RetentionPolicy;
use crate::webdav::WebDavClient;
use anyhow::Result;
use chrono::Local;
use tempfile::NamedTempFile;
use tracing::{info, instrument};

#[instrument(skip(config, project, client))]
pub async fn run_project(
    config: &Config,
    project: &BackupProject,
    client: &WebDavClient,
) -> Result<()> {
    info!(project = %project.name, "starting backup");

    let password = config.resolve_password(project);
    let retain_count = config.resolve_retain_count(project);
    let sub_dir = config.resolve_sub_dir(project);
    let remote_dir = if sub_dir.is_empty() {
        project.name.clone()
    } else {
        format!("{}/{}", sub_dir, project.name)
    };

    client.mkdir(&sub_dir).await.ok();
    client.mkdir(&remote_dir).await.ok();

    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let zip_name = format!("{}_{}.zip", project.name, timestamp);
    let remote_path = format!("{}/{}", remote_dir, zip_name);

    let temp_zip = NamedTempFile::with_suffix(".zip")?;
    let temp_path = temp_zip.path().to_path_buf();

    if let Some(ref file_config) = project.file {
        file::backup(file_config, &temp_path, password.as_deref()).await?;
    } else if let Some(ref mysql_config) = project.mysql {
        mysql::backup(mysql_config, &temp_path, password.as_deref()).await?;
    } else if let Some(ref pgsql_config) = project.pgsql {
        pgsql::backup(pgsql_config, &temp_path, password.as_deref()).await?;
    }

    info!(zip = %zip_name, "uploading to webdav");
    client.upload(temp_path.to_str().unwrap(), &remote_path).await?;

    if retain_count > 0 {
        RetentionPolicy::apply(client, &remote_dir, &project.name, retain_count).await?;
    }

    info!(project = %project.name, "backup completed");
    Ok(())
}

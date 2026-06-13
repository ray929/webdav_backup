mod backup;
mod config;
mod logger;
mod retention;
mod webdav;

use anyhow::Result;
use clap::Parser;
use config::Config;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(name = "webdav-backup", about = "Backup files and databases to WebDAV")]
struct Cli {
    #[arg(short, long, default_value = "config.json")]
    config: String,
    #[arg(long)]
    background: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    let config = Config::from_file(&cli.config)?;
    logger::init(config.global.log_level.as_deref(), cli.background)?;

    info!("webdav_backup started");

    let mut errors = Vec::new();

    for (idx, project) in config.backup.iter().enumerate() {
        let source = match config.source.iter().find(|s| s.name == project.source) {
            Some(s) => s,
            None => {
                error!("source '{}' not found for backup[{}]", project.source, idx);
                continue;
            }
        };

        let client = match webdav::WebDavClient::new(
            source.url.clone(),
            source.username.clone(),
            source.password.clone(),
            source.proxy.as_deref(),
        ) {
            Ok(c) => c,
            Err(e) => {
                error!("failed to create webdav client for '{}': {}", source.name, e);
                continue;
            }
        };

        if let Err(e) = backup::run_project(&config, project, &client).await {
            error!("backup[{}] failed: {}", idx, e);
            errors.push((idx, e.to_string()));
        }
    }

    if errors.is_empty() {
        info!("all backups completed successfully");
        Ok(())
    } else {
        error!("{} backup(s) failed", errors.len());
        for (idx, err) in &errors {
            error!("  - backup[{}]: {}", idx, err);
        }
        anyhow::bail!("some backups failed");
    }
}

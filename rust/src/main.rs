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
#[command(name = "webdav_backup", about = "Backup files and databases to WebDAV")]
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

    for project in &config.project {
        let source = match config.source.iter().find(|s| s.name == project.source) {
            Some(s) => s,
            None => {
                error!("source '{}' not found for project '{}'", project.source, project.name);
                continue;
            }
        };

        let client = match webdav::WebDavClient::new(
            source.url.clone(),
            source.username.clone(),
            source.password.clone(),
        ) {
            Ok(c) => c,
            Err(e) => {
                error!("failed to create webdav client for '{}': {}", source.name, e);
                continue;
            }
        };

        if let Err(e) = backup::run_project(&config, project, &client).await {
            error!(project = %project.name, "backup failed: {}", e);
            errors.push((project.name.clone(), e.to_string()));
        }
    }

    if errors.is_empty() {
        info!("all backups completed successfully");
        Ok(())
    } else {
        error!("{} backup(s) failed", errors.len());
        for (name, err) in &errors {
            error!("  - {}: {}", name, err);
        }
        anyhow::bail!("some backups failed");
    }
}

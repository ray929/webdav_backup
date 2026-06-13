use crate::config::FileConfig;
use anyhow::Result;
use globset::GlobSetBuilder;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use walkdir::WalkDir;
use zip::write::ZipWriter;
use zip::{AesMode, CompressionMethod};

pub async fn backup(config: &FileConfig, zip_path: &Path, password: Option<&str>) -> Result<()> {
    let glob_set = build_exclude_set(config.exclude.as_deref())?;
    let zip_file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(zip_file);

    for local_path in &config.local_path {
        let local_path = Path::new(local_path);
        if !local_path.exists() {
            anyhow::bail!("local path does not exist: {}", local_path.display());
        }

        for entry in WalkDir::new(local_path).follow_links(false) {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let relative = path.strip_prefix(local_path)?;
            let relative_str = relative.to_string_lossy().replace('\\', "/");

            if glob_set.is_match(&relative_str) {
                tracing::debug!(file = %relative_str, "excluded");
                continue;
            }

            let mut file = File::open(path)?;
            let mut contents = Vec::new();
            file.read_to_end(&mut contents)?;

            if let Some(password) = password {
                let options = zip::write::FileOptions::<'_, ()>::default()
                    .compression_method(CompressionMethod::Deflated)
                    .with_aes_encryption(AesMode::Aes256, password);
                zip.start_file(relative_str, options)?;
            } else {
                let options = zip::write::FileOptions::<'_, ()>::default()
                    .compression_method(CompressionMethod::Deflated);
                zip.start_file(relative_str, options)?;
            }
            zip.write_all(&contents)?;
        }
    }

    zip.finish()?;
    Ok(())
}

fn build_exclude_set(patterns: Option<&str>) -> Result<globset::GlobSet> {
    let mut builder = GlobSetBuilder::new();
    if let Some(patterns) = patterns {
        for pattern in patterns.split_whitespace() {
            if pattern.is_empty() {
                continue;
            }
            let glob = globset::Glob::new(pattern)?;
            builder.add(glob);
        }
    }
    Ok(builder.build()?)
}

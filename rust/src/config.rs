use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    pub source: Vec<RemoteSource>,
    pub project: Vec<BackupProject>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct GlobalConfig {
    pub zip_password: Option<String>,
    #[serde(default)]
    pub retain_count: Option<usize>,
    pub log_level: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RemoteSource {
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: String,
    pub sub_dir: Option<String>,
    pub zip_password: Option<String>,
    #[serde(default)]
    pub retain_count: Option<usize>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BackupProject {
    pub name: String,
    pub source: String,
    pub sub_dir: Option<String>,
    pub zip_password: Option<String>,
    #[serde(default)]
    pub retain_count: Option<usize>,
    pub file: Option<FileConfig>,
    pub mysql: Option<MySqlConfig>,
    pub pgsql: Option<PgSqlConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FileConfig {
    pub local_path: String,
    pub exclude: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MySqlConfig {
    pub host: String,
    #[serde(default = "default_mysql_port")]
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub tables: Option<String>,
    pub ssl_mode: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PgSqlConfig {
    pub host: String,
    #[serde(default = "default_pgsql_port")]
    pub port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub tables: Option<String>,
    pub ssl_mode: Option<String>,
}

fn default_mysql_port() -> u16 {
    3306
}

fn default_pgsql_port() -> u16 {
    5432
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = serde_json::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        for project in &self.project {
            if !self.source.iter().any(|s| s.name == project.source) {
                anyhow::bail!(
                    "project '{}' references unknown source '{}'",
                    project.name,
                    project.source
                );
            }
            let config_count = [
                project.file.is_some(),
                project.mysql.is_some(),
                project.pgsql.is_some(),
            ]
            .iter()
            .filter(|&&x| x)
            .count();
            if config_count != 1 {
                anyhow::bail!(
                    "project '{}' must have exactly one of: file, mysql, pgsql",
                    project.name
                );
            }
        }
        Ok(())
    }

    pub fn resolve_password(
        &self,
        project: &BackupProject,
    ) -> Option<String> {
        if let Some(ref pwd) = project.zip_password {
            return Some(pwd.clone());
        }
        if let Some(source) = self.source.iter().find(|s| s.name == project.source) {
            if let Some(ref pwd) = source.zip_password {
                return Some(pwd.clone());
            }
        }
        self.global.zip_password.clone()
    }

    pub fn resolve_retain_count(
        &self,
        project: &BackupProject,
    ) -> usize {
        if let Some(count) = project.retain_count {
            return count;
        }
        if let Some(source) = self.source.iter().find(|s| s.name == project.source) {
            if let Some(count) = source.retain_count {
                return count;
            }
        }
        self.global.retain_count.unwrap_or(0)
    }

    pub fn resolve_sub_dir(&self, project: &BackupProject) -> String {
        if let Some(ref dir) = project.sub_dir {
            return dir.trim_end_matches('/').to_string();
        }
        if let Some(source) = self.source.iter().find(|s| s.name == project.source) {
            if let Some(ref dir) = source.sub_dir {
                return dir.trim_end_matches('/').to_string();
            }
        }
        String::new()
    }
}

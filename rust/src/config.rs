use anyhow::Context;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub global: GlobalConfig,
    pub source: Vec<RemoteSource>,
    pub backup: Vec<BackupProject>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct GlobalConfig {
    pub zip_password: Option<String>,
    #[serde(default)]
    pub retain_count: Option<usize>,
    pub log_level: Option<String>,
    pub mysqldump_path: Option<String>,
    pub pg_dump_path: Option<String>,
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
    pub proxy: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BackupProject {
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
    #[serde(deserialize_with = "one_or_many_strings")]
    pub local_path: Vec<String>,
    pub exclude: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MySqlConfig {
    pub host: String,
    #[serde(default = "default_mysql_port")]
    pub port: u16,
    pub username: String,
    pub password: String,
    #[serde(deserialize_with = "one_or_many_strings")]
    pub database: Vec<String>,
    pub ssl_mode: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PgSqlConfig {
    pub host: String,
    #[serde(default = "default_pgsql_port")]
    pub port: u16,
    pub username: String,
    pub password: String,
    #[serde(deserialize_with = "one_or_many_strings")]
    pub database: Vec<String>,
    pub ssl_mode: Option<String>,
}

fn one_or_many_strings<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;

    struct OneOrMany;

    impl<'de> de::Visitor<'de> for OneOrMany {
        type Value = Vec<String>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or an array of strings")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Vec<String>, E> {
            Ok(vec![v.to_string()])
        }

        fn visit_seq<S: de::SeqAccess<'de>>(self, seq: S) -> Result<Vec<String>, S::Error> {
            de::Deserialize::deserialize(de::value::SeqAccessDeserializer::new(seq))
        }
    }

    deserializer.deserialize_any(OneOrMany)
}

/// Parse a `"db.table"` entry into (database_name, optional_table_pattern).
/// `"db.*"` or `"db"` means all tables in that database.
pub fn parse_db_table(entry: &str) -> (&str, Option<&str>) {
    if let Some(dot_pos) = entry.find('.') {
        let db = &entry[..dot_pos];
        let table = &entry[dot_pos + 1..];
        if table == "*" {
            (db, None)
        } else {
            (db, Some(table))
        }
    } else {
        (entry, None)
    }
}

fn default_mysql_port() -> u16 {
    3306
}

fn default_pgsql_port() -> u16 {
    5432
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            anyhow::bail!(
                "Configuration file '{}' not found.\n       Please create it based on the example: config.example.json",
                path.display()
            );
        }
        let content = std::fs::read_to_string(path).with_context(|| {
            format!("Failed to read configuration file '{}'", path.display())
        })?;
        let config: Config =
            serde_json::from_str(&content).with_context(|| {
                format!(
                    "Failed to parse configuration file '{}'.\n       Please check the JSON format.",
                    path.display()
                )
            })?;
        config.validate().with_context(|| {
            format!(
                "Configuration file '{}' validation failed",
                path.display()
            )
        })?;
        Ok(config)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        for (idx, project) in self.backup.iter().enumerate() {
            if !self.source.iter().any(|s| s.name == project.source) {
                anyhow::bail!(
                    "backup[{}] references unknown source '{}'",
                    idx,
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
                    "backup[{}] must have exactly one of: file, mysql, pgsql",
                    idx
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

    pub fn resolve_mysqldump_path(&self) -> &str {
        self.global.mysqldump_path.as_deref().unwrap_or("mysqldump")
    }

    pub fn resolve_pg_dump_path(&self) -> &str {
        self.global.pg_dump_path.as_deref().unwrap_or("pg_dump")
    }
}

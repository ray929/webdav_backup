mod auth;

use anyhow::{anyhow, Result};
use auth::{build_digest_auth_header, parse_www_authenticate, Auth};
use reqwest::{Client, Method, StatusCode};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, warn};

pub struct WebDavClient {
    client: Client,
    base_url: String,
    username: String,
    password: String,
    auth: Arc<Mutex<Option<Auth>>>,
}

impl WebDavClient {
    pub fn new(
        base_url: String,
        username: String,
        password: String,
        proxy: Option<&str>,
    ) -> Result<Self> {
        let mut builder = Client::builder().danger_accept_invalid_certs(false);
        if let Some(proxy_url) = proxy {
            let proxy = reqwest::Proxy::all(proxy_url)
                .map_err(|e| anyhow!("invalid proxy '{}': {}", proxy_url, e))?;
            builder = builder.proxy(proxy);
        }
        let client = builder.build()?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            username,
            password,
            auth: Arc::new(Mutex::new(None)),
        })
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Vec<u8>>,
        extra_headers: Option<Vec<(&str, &str)>>,
    ) -> Result<reqwest::Response> {
        let url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        let uri = path.trim_start_matches('/').to_string();

        let build_req = |auth_header: Option<String>| {
            let mut req = self.client.request(method.clone(), &url);
            if let Some(ref b) = body {
                req = req.body(b.clone());
            }
            if let Some(ref headers) = extra_headers {
                for (k, v) in headers {
                    req = req.header(*k, *v);
                }
            }
            if let Some(header) = auth_header {
                req = req.header("Authorization", header);
            }
            req
        };

        let mut req = build_req(None);

        let auth_guard = self.auth.lock().await;
        if let Some(auth) = auth_guard.as_ref() {
            match auth {
                Auth::Basic => {
                    req = req.basic_auth(&self.username, Some(&self.password));
                }
                Auth::Digest(digest) => {
                    let header = build_digest_auth_header(digest, method.as_str(), &uri);
                    req = req.header("Authorization", header);
                }
            }
        }
        drop(auth_guard);

        let resp = req.send().await?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            let www_auth = resp
                .headers()
                .get("www-authenticate")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| anyhow!("401 without www-authenticate header"))?;

            let mut auth_guard = self.auth.lock().await;
            match parse_www_authenticate(www_auth) {
                Some(Auth::Basic) => {
                    debug!("using basic auth");
                    *auth_guard = Some(Auth::Basic);
                    drop(auth_guard);

                    let req = build_req(None).basic_auth(&self.username, Some(&self.password));
                    return Ok(req.send().await?);
                }
                Some(Auth::Digest(mut digest)) => {
                    debug!("using digest auth");
                    digest.username.clone_from(&self.username);
                    digest.password.clone_from(&self.password);
                    let header = build_digest_auth_header(&digest, method.as_str(), &uri);
                    *auth_guard = Some(Auth::Digest(digest));
                    drop(auth_guard);

                    let req = build_req(Some(header));
                    return Ok(req.send().await?);
                }
                None => {
                    return Err(anyhow!("unsupported auth scheme: {}", www_auth));
                }
            }
        }

        Ok(resp)
    }

    pub async fn mkdir(&self, path: &str) -> Result<()> {
        let method = Method::from_bytes(b"MKCOL")?;

        // Recursively create each path segment
        let segments: Vec<&str> = path.trim_start_matches('/').trim_end_matches('/').split('/').filter(|s| !s.is_empty()).collect();
        let mut current = String::new();
        for segment in segments {
            if !current.is_empty() {
                current.push('/');
            }
            current.push_str(segment);

            let mkcol_path = format!("{}/", current);
            debug!("MKCOL {}", mkcol_path);
            let resp = self.request(method.clone(), &mkcol_path, None, None).await?;
            let status = resp.status();
            if status.is_success() {
                debug!("MKCOL {} succeeded", mkcol_path);
                continue;
            }
            if status == StatusCode::METHOD_NOT_ALLOWED || status == StatusCode::CONFLICT {
                debug!("MKCOL {} failed ({}), checking if directory exists", mkcol_path, status);
                if self.check_collection_exists(&current).await? {
                    debug!("directory '{}' already exists", current);
                    continue;
                }
                anyhow::bail!(
                    "cannot create directory '{}' via WebDAV (MKCOL returned {}). \
                     Please create the folder manually through the Koofr web interface.",
                    current, status
                );
            }
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("MKCOL failed for '{}': {} {}", mkcol_path, status, text);
        }

        Ok(())
    }

    async fn check_collection_exists(&self, path: &str) -> Result<bool> {
        let method = Method::from_bytes(b"PROPFIND")?;
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<propfind xmlns="DAV:"><prop><resourcetype/></prop></propfind>"#
            .as_bytes()
            .to_vec();

        let check_path = format!("{}/", path.trim_end_matches('/'));
        let resp = self.request(method, &check_path, Some(body), Some(vec![("Depth", "0")])).await?;
        let status = resp.status();
        Ok(status.is_success() || status == StatusCode::MULTI_STATUS)
    }

    pub async fn upload(&self, local_path: &str, remote_path: &str) -> Result<()> {
        let bytes = tokio::fs::read(local_path).await?;
        debug!("uploading {} bytes to {}", bytes.len(), remote_path);
        let resp = self
            .request(Method::PUT, remote_path, Some(bytes), None)
            .await
            .map_err(|e| anyhow!("upload request failed for '{}': {}", remote_path, e))?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("PUT failed: {} {}", status, text));
        }
        Ok(())
    }

    pub async fn delete(&self, path: &str) -> Result<()> {
        let resp = self.request(Method::DELETE, path, None, None).await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("DELETE failed: {} {}", status, text));
        }
        Ok(())
    }

    pub async fn list(&self, path: &str) -> Result<Vec<String>> {
        let method = Method::from_bytes(b"PROPFIND")?;
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<propfind xmlns="DAV:"><prop><resourcetype/></prop></propfind>"#
            .as_bytes()
            .to_vec();

        let resp = self
            .request(method, path, Some(body), Some(vec![("Depth", "1")]))
            .await?;

        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(anyhow!("PROPFIND failed: {} {}", status, text));
        }

        let xml = resp.text().await?;
        let hrefs = parse_propfind_response(&xml);
        let prefix = format!("{}/", path.trim_end_matches('/'));

        let mut result = Vec::new();
        for href in hrefs {
            let normalized = href.trim_start_matches('/');
            if normalized == path.trim_start_matches('/')
                || normalized == path.trim_start_matches('/').trim_end_matches('/')
            {
                continue;
            }
            let name = if normalized.starts_with(prefix.trim_start_matches('/')) {
                normalized
                    .strip_prefix(prefix.trim_start_matches('/'))
                    .unwrap_or(normalized)
            } else {
                normalized
            };
            if !name.is_empty() {
                result.push(name.to_string());
            }
        }

        Ok(result)
    }
}

fn parse_propfind_response(xml: &str) -> Vec<String> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut hrefs = Vec::new();
    let mut current = String::new();
    let mut in_href = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if e.local_name().as_ref() == b"href" {
                    in_href = true;
                    current.clear();
                }
            }
            Ok(Event::Text(e)) => {
                if in_href {
                    if let Ok(txt) = e.unescape() {
                        current.push_str(&txt);
                    }
                }
            }
            Ok(Event::End(e)) => {
                if e.local_name().as_ref() == b"href" {
                    in_href = false;
                    hrefs.push(current.clone());
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("xml parse error: {}", e);
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    hrefs
}

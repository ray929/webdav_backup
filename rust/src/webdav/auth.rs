use md5::{Digest, Md5};
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug)]
pub struct DigestAuth {
    pub username: String,
    pub password: String,
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    pub qop: Option<String>,
    pub algorithm: Option<String>,
    pub nc: AtomicU32,
}

impl Clone for DigestAuth {
    fn clone(&self) -> Self {
        Self {
            username: self.username.clone(),
            password: self.password.clone(),
            realm: self.realm.clone(),
            nonce: self.nonce.clone(),
            opaque: self.opaque.clone(),
            qop: self.qop.clone(),
            algorithm: self.algorithm.clone(),
            nc: AtomicU32::new(self.nc.load(Ordering::SeqCst)),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Auth {
    Basic,
    Digest(DigestAuth),
}

pub fn parse_www_authenticate(header: &str) -> Option<Auth> {
    let header = header.trim();
    if header.starts_with("Basic ") {
        return Some(Auth::Basic);
    }
    if !header.starts_with("Digest ") {
        return None;
    }

    let rest = &header[7..];
    let mut realm = None;
    let mut nonce = None;
    let mut opaque = None;
    let mut qop = None;
    let mut algorithm = None;

    for part in rest.split(',') {
        let part = part.trim();
        if let Some((key, value)) = part.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"');
            match key {
                "realm" => realm = Some(value.to_string()),
                "nonce" => nonce = Some(value.to_string()),
                "opaque" => opaque = Some(value.to_string()),
                "qop" => qop = Some(value.to_string()),
                "algorithm" => algorithm = Some(value.to_string()),
                _ => {}
            }
        }
    }

    Some(Auth::Digest(DigestAuth {
        username: String::new(),
        password: String::new(),
        realm: realm?,
        nonce: nonce?,
        opaque,
        qop,
        algorithm,
        nc: AtomicU32::new(1),
    }))
}

pub fn build_digest_auth_header(
    auth: &DigestAuth,
    method: &str,
    uri: &str,
) -> String {
    let ha1 = format!("{}:{}:{}", auth.username, auth.realm, auth.password);
    let ha1 = hex::encode(Md5::digest(ha1.as_bytes()));

    let ha2 = format!("{}:{}", method, uri);
    let ha2 = hex::encode(Md5::digest(ha2.as_bytes()));

    let nc = auth.nc.fetch_add(1, Ordering::SeqCst);
    let nc_hex = format!("{:08x}", nc);
    let cnonce = format!("{:x}", Md5::digest(rand::random::<[u8; 16]>()));

    let response = if let Some(ref qop) = auth.qop {
        let qop = if qop.contains("auth") { "auth" } else { "" };
        let input = format!("{}:{}:{}:{}:{}:{}", ha1, auth.nonce, nc_hex, cnonce, qop, ha2);
        hex::encode(Md5::digest(input.as_bytes()))
    } else {
        let input = format!("{}:{}:{}", ha1, auth.nonce, ha2);
        hex::encode(Md5::digest(input.as_bytes()))
    };

    let mut parts = vec![
        format!("username=\"{}\"", auth.username),
        format!("realm=\"{}\"", auth.realm),
        format!("nonce=\"{}\"", auth.nonce),
        format!("uri=\"{}\"", uri),
        format!("response=\"{}\"", response),
    ];

    if let Some(ref qop) = auth.qop {
        let qop = if qop.contains("auth") { "auth" } else { qop };
        parts.push(format!("qop={}", qop));
        parts.push(format!("nc={}", nc_hex));
        parts.push(format!("cnonce=\"{}\"", cnonce));
    }

    if let Some(ref opaque) = auth.opaque {
        parts.push(format!("opaque=\"{}\"", opaque));
    }

    if let Some(ref algorithm) = auth.algorithm {
        parts.push(format!("algorithm={}", algorithm));
    }

    format!("Digest {}", parts.join(", "))
}

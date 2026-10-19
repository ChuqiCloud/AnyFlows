use std::collections::BTreeSet;

use af_db::CredentialProxyScheme;
use sha2::{Digest as _, Sha256};
use url::Url;
use zeroize::Zeroize as _;

use super::ProxyPoolError;

pub(super) const MAX_IMPORT_BYTES: usize = 1_048_576;
pub(super) const MAX_NODES: usize = 256;

pub(super) struct Candidate {
    pub scheme: CredentialProxyScheme,
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub remark: String,
    pub fingerprint: String,
}
impl Drop for Candidate {
    fn drop(&mut self) {
        if let Some(password) = &mut self.password {
            password.zeroize();
        }
    }
}
pub(super) struct Parsed {
    pub nodes: Vec<Candidate>,
    pub skipped: usize,
    pub duplicates: usize,
}

/// 只提取明确的代理 URI/主机端口，不根据地区备注推断协议。
pub(super) fn parse(
    text: &str,
    default_scheme: CredentialProxyScheme,
) -> Result<Parsed, ProxyPoolError> {
    if text.len() > MAX_IMPORT_BYTES {
        return Err(ProxyPoolError::InvalidInput);
    }
    let text = text.trim_start_matches('\u{feff}').trim();
    let mut entries = Vec::new();
    if text.starts_with('{')
        || (text.starts_with('[') && !text.lines().next().is_some_and(|line| line.contains("]:")))
    {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|_| ProxyPoolError::InvalidInput)?;
        json_entries(&value, &mut entries, 0, default_scheme)?;
    } else {
        for line in text.lines().filter(|line| !line.trim().is_empty()) {
            if line.trim_start().starts_with(['#', ';']) || line.trim_start().starts_with("//") {
                entries.push(line.to_owned());
                continue;
            }
            let mut starts = ["http://", "https://", "socks5://", "socks5h://"]
                .into_iter()
                .flat_map(|scheme| line.match_indices(scheme).map(|(index, _)| index))
                .collect::<Vec<_>>();
            starts.sort_unstable();
            if starts.len() > 1 {
                for (index, start) in starts.iter().enumerate() {
                    entries.push(
                        line[*start..starts.get(index + 1).copied().unwrap_or(line.len())]
                            .trim()
                            .trim_end_matches([',', ';', '，', '；'])
                            .to_owned(),
                    );
                }
            } else {
                entries.push(line.to_owned());
            }
        }
    }
    if entries.len() > 4096 {
        return Err(ProxyPoolError::InvalidInput);
    }
    let mut parsed = Parsed {
        nodes: Vec::new(),
        skipped: 0,
        duplicates: 0,
    };
    let mut fingerprints = BTreeSet::new();
    for line in entries {
        match candidate(&line, default_scheme) {
            Some(node) if fingerprints.insert(node.fingerprint.clone()) => parsed.nodes.push(node),
            Some(_) => parsed.duplicates += 1,
            None => parsed.skipped += 1,
        }
        if parsed.nodes.len() > MAX_NODES {
            return Err(ProxyPoolError::InvalidInput);
        }
    }
    if parsed.nodes.is_empty() {
        return Err(ProxyPoolError::InvalidInput);
    }
    Ok(parsed)
}

fn json_entries(
    value: &serde_json::Value,
    entries: &mut Vec<String>,
    depth: usize,
    default_scheme: CredentialProxyScheme,
) -> Result<(), ProxyPoolError> {
    if depth > 4 || entries.len() > 4096 {
        return Err(ProxyPoolError::InvalidInput);
    }
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                json_entries(item, entries, depth + 1, default_scheme)?;
            }
        }
        serde_json::Value::String(value) => entries.push(value.clone()),
        serde_json::Value::Object(object) => {
            if let Some(items) = object
                .get("proxies")
                .or_else(|| object.get("data"))
                .or_else(|| object.get("list"))
            {
                json_entries(items, entries, depth + 1, default_scheme)?;
            } else if let Some(value) = object.get("url").and_then(serde_json::Value::as_str) {
                entries.push(value.to_owned());
            } else {
                let host = object
                    .get("host")
                    .or_else(|| object.get("ip"))
                    .and_then(serde_json::Value::as_str);
                let Some(host) = host else {
                    entries.push(String::new());
                    return Ok(());
                };
                let Some(port) = object.get("port") else {
                    entries.push(String::new());
                    return Ok(());
                };
                let port = port
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| port.to_string());
                let scheme = object
                    .get("scheme")
                    .or_else(|| object.get("protocol"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(default_scheme.as_str());
                let host = if host.contains(':') && !host.starts_with('[') {
                    format!("[{host}]")
                } else {
                    host.to_owned()
                };
                let Ok(mut url) = Url::parse(&format!("{scheme}://{host}:{port}")) else {
                    entries.push(String::new());
                    return Ok(());
                };
                if let Some(user) = object.get("username").and_then(serde_json::Value::as_str)
                    && url.set_username(user).is_err()
                {
                    entries.push(String::new());
                    return Ok(());
                }
                if let Some(password) = object.get("password").and_then(serde_json::Value::as_str)
                    && url.set_password(Some(password)).is_err()
                {
                    entries.push(String::new());
                    return Ok(());
                }
                if let Some(remark) = object
                    .get("remark")
                    .or_else(|| object.get("name"))
                    .and_then(serde_json::Value::as_str)
                {
                    url.set_fragment(Some(remark));
                }
                entries.push(url.to_string());
            }
        }
        _ => entries.push(String::new()),
    }
    Ok(())
}

fn candidate(line: &str, default_scheme: CredentialProxyScheme) -> Option<Candidate> {
    let csv = csv_entry(line, default_scheme);
    let line = csv.as_deref().unwrap_or(line).trim();
    if line.starts_with(['#', ';']) || line.starts_with("//") {
        return None;
    }
    let start = ["http://", "https://", "socks5://", "socks5h://"]
        .into_iter()
        .filter_map(|scheme| line.find(scheme))
        .min()
        .unwrap_or(0);
    let line = &line[start..];
    let mut fields = line.split_whitespace();
    let raw = fields.next()?.trim_end_matches([',', ';', '"', '\'']);
    let raw = raw.split(['，', '；', '。']).next()?;
    let (raw, fragment) = raw
        .split_once('#')
        .map_or((raw, None), |(uri, note)| (uri, Some(note)));
    let note = fragment
        .into_iter()
        .chain(fields)
        .collect::<Vec<_>>()
        .join(" ")
        .trim_start_matches(['#', ';'])
        .trim()
        .to_owned();
    let url = if raw.contains("://") {
        Url::parse(raw).ok()?
    } else {
        let parts = raw.split(':').collect::<Vec<_>>();
        if parts.len() == 4 && !raw.contains('@') {
            let mut url = Url::parse(&format!(
                "{}://{}:{}",
                default_scheme.as_str(),
                parts[0],
                parts[1]
            ))
            .ok()?;
            url.set_username(parts[2]).ok()?;
            url.set_password(Some(parts[3])).ok()?;
            url
        } else {
            // 裸地址必须明确提供端口，避免把普通备注当作默认 80/443 代理。
            raw.rsplit_once(':')?.1.parse::<u16>().ok()?;
            Url::parse(&format!("{}://{raw}", default_scheme.as_str())).ok()?
        }
    };
    let scheme = match url.scheme() {
        "http" => CredentialProxyScheme::Http,
        "https" => CredentialProxyScheme::Https,
        "socks5" => CredentialProxyScheme::Socks5,
        "socks5h" => CredentialProxyScheme::Socks5h,
        _ => return None,
    };
    if !matches!(url.path(), "" | "/") || url.query().is_some() {
        return None;
    }
    let host = match url.host()? {
        url::Host::Ipv6(ip) => ip.to_string(),
        host => host.to_string(),
    };
    if host.len() > 255 {
        return None;
    }
    let port = url.port().or(match scheme {
        CredentialProxyScheme::Http => Some(80),
        CredentialProxyScheme::Https => Some(443),
        _ => None,
    })?;
    if port == 0 {
        return None;
    }
    let decode = |value: &str| -> Option<String> {
        let value = percent_encoding::percent_decode_str(value)
            .decode_utf8()
            .ok()?
            .into_owned();
        (!value.is_empty() && !value.chars().any(char::is_control)).then_some(value)
    };
    let username = if url.username().is_empty() {
        None
    } else {
        Some(decode(url.username())?)
    };
    let password = match url.password() {
        Some(value) => Some(decode(value)?),
        None => None,
    };
    if username.is_some() != password.is_some()
        || username.as_ref().is_some_and(|s| s.len() > 320)
        || password.as_ref().is_some_and(|s| s.len() > 4096)
    {
        return None;
    }
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(&(scheme.as_str(), &host, port, &username, &password)).ok()?
        )
    );
    let note = decode(&note).unwrap_or(note);
    let note = if note.contains("://") || note.contains('@') {
        String::new()
    } else {
        note
    };
    let remark = note
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect::<String>();
    Some(Candidate {
        scheme,
        host,
        port,
        username,
        password,
        remark,
        fingerprint,
    })
}

// CSV: host,port[,username,password[,scheme[,remark]]]. URI 行里的逗号保留为 userinfo。
fn csv_entry(line: &str, default_scheme: CredentialProxyScheme) -> Option<String> {
    if line.contains("://") {
        return None;
    }
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.trim().chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(current.trim().to_owned());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if quoted {
        return None;
    }
    fields.push(current.trim().to_owned());
    if fields.len() < 2 || fields.len() > 6 || fields[1].parse::<u16>().ok()? == 0 {
        return None;
    }
    let scheme = fields
        .get(4)
        .filter(|s| !s.is_empty())
        .map_or(default_scheme.as_str(), String::as_str);
    let host = if fields[0].contains(':') && !fields[0].starts_with('[') {
        format!("[{}]", fields[0])
    } else {
        fields[0].clone()
    };
    let mut url = Url::parse(&format!("{scheme}://{host}:{}", fields[1])).ok()?;
    if let Some(user) = fields.get(2).filter(|s| !s.is_empty()) {
        url.set_username(user).ok()?;
    }
    if let Some(password) = fields.get(3).filter(|s| !s.is_empty()) {
        url.set_password(Some(password)).ok()?;
    }
    if let Some(note) = fields.get(5).filter(|s| !s.is_empty()) {
        url.set_fragment(Some(note));
    }
    Some(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extracts_mixed_pool_text_without_importing_commented_out_uris() {
        let result = parse(
            "HTTP 池:http://alice:secret@192.0.2.1:3129,http://alice:secret@192.0.2.2:3129，socks5://192.0.2.3:1080#香港 节点\nsocks5://192.0.2.4:1080 #hk\n# http://192.0.2.5:80 socks5://192.0.2.6:1080\n; http://192.0.2.7:80\n// socks5://192.0.2.8:1080\n说明文字",
            CredentialProxyScheme::Socks5,
        )
        .unwrap();
        assert_eq!(result.nodes.len(), 4);
        assert_eq!(result.skipped, 4);
        assert_eq!(result.nodes[0].scheme, CredentialProxyScheme::Http);
        assert_eq!(result.nodes[0].username.as_deref(), Some("alice"));
        assert_eq!(result.nodes[1].password.as_deref(), Some("secret"));
        assert_eq!(result.nodes[2].scheme, CredentialProxyScheme::Socks5);
        assert_eq!(result.nodes[2].remark, "香港 节点");
        assert_eq!(result.nodes[3].remark, "hk");
    }
    #[test]
    fn skips_invalid_json_members_and_redacts_encoded_uri_remarks() {
        let result = parse(
            r#"{"data":[{"host":"192.0.2.1","port":"invalid"},null,{"missing":"host"},"http://ci-user:ci-password@192.0.2.2:3129#socks5%3A%2F%2Fsecret%40host"]}"#,
            CredentialProxyScheme::Socks5,
        ).unwrap();
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.skipped, 3);
        assert_eq!(result.nodes[0].scheme, CredentialProxyScheme::Http);
        assert_eq!(result.nodes[0].password.as_deref(), Some("ci-password"));
        assert!(result.nodes[0].remark.is_empty());
    }
    #[test]
    fn extracts_authenticated_uris_comments_ipv6_and_deduplicates() {
        let result=parse("HTTP 池:http://alice:p%40ss+word&tail@192.0.2.1:3129\nsocks5://192.0.2.2:1080 #hk\nsocks5://192.0.2.2:1080 #another\n[2001:db8::1]:1080\n192.0.2.3:1080:bob:secret\ngarbage",CredentialProxyScheme::Socks5).unwrap();
        assert_eq!(result.nodes.len(), 4);
        assert_eq!(result.duplicates, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.nodes[0].password.as_deref(), Some("p@ss+word&tail"));
        assert_eq!(result.nodes[1].remark, "hk");
        assert_eq!(result.nodes[2].host, "2001:db8::1");
    }
    #[test]
    fn supports_json_objects_and_rejects_non_proxy_urls_and_oversized_input() {
        let mixed = parse("ordinary-note\n192.0.2.1:80", CredentialProxyScheme::Http).unwrap();
        assert_eq!(mixed.nodes.len(), 1);
        assert_eq!(mixed.skipped, 1);
        let nodes=parse(r#"{"proxies":["socks5h://192.0.2.1:1080",{"host":"192.0.2.2","port":8080,"scheme":"http","username":"a","password":"b"}]}"#,CredentialProxyScheme::Socks5).unwrap();
        assert_eq!(nodes.nodes.len(), 2);
        let csv = parse(
            "ip,port,user,password,scheme,remark\n192.0.2.1,8080,alice,secret,http,hk",
            CredentialProxyScheme::Socks5,
        )
        .unwrap();
        assert_eq!(csv.nodes[0].scheme, CredentialProxyScheme::Http);
        assert_eq!(csv.nodes[0].password.as_deref(), Some("secret"));
        assert_eq!(
            parse("[2001:db8::1]:1080", CredentialProxyScheme::Socks5)
                .unwrap()
                .nodes
                .len(),
            1
        );
        assert!(
            parse(
                "https://example.com/list?token=secret",
                CredentialProxyScheme::Http
            )
            .is_err()
        );
        assert!(
            parse(
                &"x".repeat(MAX_IMPORT_BYTES + 1),
                CredentialProxyScheme::Http
            )
            .is_err()
        );
    }
}

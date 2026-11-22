use af_httpclient::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::time::Duration;
use url::Url;

use super::{
    AdminPluginLifecycleError, DatabaseAdminPluginLifecycleService,
    MAX_PLUGIN_PACKAGE_UPLOAD_BYTES, PluginMarketSource, PluginPackageUpload, execute_market_get,
    execute_market_get_with_headers,
};

const MAX_RELEASE_BYTES: usize = 2 * 1024 * 1024;

struct GitHubRepository {
    owner: String,
    name: String,
}

impl GitHubRepository {
    fn parse(input: &str) -> Result<Self, AdminPluginLifecycleError> {
        let url = Url::parse(input).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if input.len() > 256
            || url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let mut segments = url
            .path_segments()
            .ok_or(AdminPluginLifecycleError::InvalidInput)?
            .collect::<Vec<_>>();
        if segments.last() == Some(&"") {
            segments.pop();
        }
        let [owner, name] = segments.as_slice() else {
            return Err(AdminPluginLifecycleError::InvalidInput);
        };
        let owner = *owner;
        let name = *name;
        let name = name.strip_suffix(".git").unwrap_or(name);
        if !valid_segment(owner) || !valid_segment(name) {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        Ok(Self {
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    fn url(&self) -> String {
        format!("https://github.com/{}/{}", self.owner, self.name)
    }

    fn asset_url(&self, value: &str) -> Result<Url, AdminPluginLifecycleError> {
        let url = Url::parse(value).map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        if value.len() > 1024
            || url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let mut path = url.path().split('/').skip(1);
        if !path
            .next()
            .is_some_and(|part| part.eq_ignore_ascii_case(self.owner.as_str()))
            || !path
                .next()
                .is_some_and(|part| part.eq_ignore_ascii_case(self.name.as_str()))
            || path.next() != Some("releases")
            || path.next() != Some("download")
            || !path.next().is_some_and(|part| !part.is_empty())
            || !path.next().is_some_and(|part| !part.is_empty())
            || path.next().is_some()
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        Ok(url)
    }
}

fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[derive(Deserialize)]
struct GitHubRelease {
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    name: String,
    size: u64,
    browser_download_url: String,
    digest: Option<String>,
}

fn select_asset<'a>(
    assets: &'a [GitHubAsset],
    filename: Option<&str>,
) -> Result<&'a GitHubAsset, AdminPluginLifecycleError> {
    let mut packages = assets.iter().filter(|asset| {
        let name = asset.name.to_ascii_lowercase();
        name.ends_with(".afplugin") || name.ends_with(".zip")
    });
    if let Some(filename) = filename {
        let mut matching = packages.filter(|asset| asset.name == filename);
        let asset = matching.next().ok_or(AdminPluginLifecycleError::NotFound)?;
        if matching.next().is_some() {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        return Ok(asset);
    }
    let mut preferred = packages
        .clone()
        .filter(|asset| asset.name.to_ascii_lowercase().ends_with(".afplugin"));
    let (first, second) = if let Some(asset) = preferred.next() {
        (Some(asset), preferred.next())
    } else {
        (packages.next(), packages.next())
    };
    if second.is_some() {
        return Err(AdminPluginLifecycleError::InvalidInput);
    }
    first.ok_or(AdminPluginLifecycleError::NotFound)
}

impl DatabaseAdminPluginLifecycleService {
    /// Fill a host-owned, previously authorized installation slot from a public GitHub release.
    pub async fn download_github_plugin_package(
        &self,
        upload: &mut PluginPackageUpload,
        repository_url: &str,
        filename: Option<&str>,
    ) -> Result<(), AdminPluginLifecycleError> {
        if upload.uploaded != 0 {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let repository = GitHubRepository::parse(repository_url)?;
        let client = self
            .market_clients
            .as_ref()
            .ok_or(AdminPluginLifecycleError::RuntimeUnavailable)?
            .get(Some(Duration::from_secs(60)))
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        let release_url = format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            repository.owner, repository.name
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            HeaderName::from_static("user-agent"),
            HeaderValue::from_static("AnyFlows"),
        );
        let response = execute_market_get_with_headers(&client, &release_url, headers)
            .await
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::NOT_FOUND => return Err(AdminPluginLifecycleError::NotFound),
            _ => return Err(AdminPluginLifecycleError::RuntimeUnavailable),
        }
        let mut metadata = Vec::new();
        let mut stream = response.into_bytes_stream();
        while let Some(chunk) = stream.next_chunk().await {
            let chunk = chunk.map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
            if metadata.len().saturating_add(chunk.len()) > MAX_RELEASE_BYTES {
                return Err(AdminPluginLifecycleError::PayloadTooLarge);
            }
            metadata.extend_from_slice(&chunk);
        }
        let release: GitHubRelease = serde_json::from_slice(&metadata)
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        let asset = select_asset(&release.assets, filename)?;
        if asset.size == 0 {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        if asset.size > MAX_PLUGIN_PACKAGE_UPLOAD_BYTES as u64 {
            return Err(AdminPluginLifecycleError::PayloadTooLarge);
        }
        let archive_url = repository.asset_url(&asset.browser_download_url)?;
        let response = execute_market_get(&client, archive_url.as_str())
            .await
            .map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
        match response.status() {
            StatusCode::OK => {}
            StatusCode::NOT_FOUND => return Err(AdminPluginLifecycleError::NotFound),
            _ => return Err(AdminPluginLifecycleError::RuntimeUnavailable),
        }
        let mut hasher = Sha256::new();
        let mut stream = response.into_bytes_stream();
        while let Some(chunk) = stream.next_chunk().await {
            let chunk = chunk.map_err(|_| AdminPluginLifecycleError::RuntimeUnavailable)?;
            upload.write_chunk(&chunk).await?;
            hasher.update(&chunk);
        }
        let digest = format!("{:x}", hasher.finalize());
        if upload.uploaded as u64 != asset.size
            || asset
                .digest
                .as_deref()
                .is_some_and(|published| published.strip_prefix("sha256:") != Some(digest.as_str()))
        {
            return Err(AdminPluginLifecycleError::InvalidInput);
        }
        let source = PluginMarketSource {
            kind: PluginMarketSource::GITHUB_RELEASE.to_owned(),
            archive_url: archive_url.to_string(),
            sha256: digest,
            repository: Some(repository.url()),
            commit: None,
            path: None,
        };
        source
            .validate()
            .map_err(|_| AdminPluginLifecycleError::InvalidInput)?;
        upload.market_source = Some(source);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn github_repository_only_accepts_repository_urls() {
        let repo = GitHubRepository::parse("https://github.com/example/plugin.git/").unwrap();
        assert_eq!(repo.url(), "https://github.com/example/plugin");
        assert_eq!(
            GitHubRepository::parse("HTTPS://GITHUB.COM/example/plugin")
                .unwrap()
                .url(),
            "https://github.com/example/plugin"
        );
        for invalid in [
            "http://github.com/example/plugin",
            "https://user@github.com/example/plugin",
            "https://github.com:8443/example/plugin",
            "https://github.com.example.com/example/plugin",
            "https://github.com/example/plugin/tree/main",
            "https://github.com/example/plugin?ref=main",
            "https://github.com/example/plugin#main",
            "https://github.com/example/..",
        ] {
            assert!(GitHubRepository::parse(invalid).is_err(), "{invalid}");
        }
        assert!(
            repo.asset_url("https://github.com/Example/plugin/releases/download/v1/a.afplugin")
                .is_ok()
        );
        assert!(
            repo.asset_url("https://github.com/other/plugin/releases/download/v1/a.afplugin")
                .is_err()
        );
        assert!(
            repo.asset_url(
                "https://github.com.evil.example/example/plugin/releases/download/v1/a.afplugin"
            )
            .is_err()
        );
        assert!(
            repo.asset_url(
                "https://github.com/example/plugin/releases/download/v1/a.afplugin/extra"
            )
            .is_err()
        );
        assert!(
            repo.asset_url("https://github.com/example/plugin/releases/download//a.afplugin")
                .is_err()
        );
        assert!(
            repo.asset_url("https://github.com/example/plugin/releases/download/v1/")
                .is_err()
        );
        assert!(
            repo.asset_url(
                "https://github.com/example/plugin/releases/download/v1/a.afplugin?download=1"
            )
            .is_err()
        );
    }

    #[test]
    fn github_releases_prefer_afplugin_and_require_a_choice_for_multiple_packages() {
        let asset = |name: &str| GitHubAsset {
            name: name.to_owned(),
            size: 100,
            browser_download_url: String::new(),
            digest: None,
        };
        let mut assets = vec![
            asset("windows.zip"),
            asset("linux.afplugin"),
            asset("notes.txt"),
        ];
        assert_eq!(select_asset(&assets, None).unwrap().name, "linux.afplugin");
        assets.push(asset("macos.afplugin"));
        assert!(matches!(
            select_asset(&assets, None),
            Err(AdminPluginLifecycleError::InvalidInput)
        ));
        assert_eq!(
            select_asset(&assets, Some("windows.zip")).unwrap().name,
            "windows.zip"
        );
        assert!(matches!(
            select_asset(&assets, Some("notes.txt")),
            Err(AdminPluginLifecycleError::NotFound)
        ));
        assets.push(asset("windows.zip"));
        assert!(matches!(
            select_asset(&assets, Some("windows.zip")),
            Err(AdminPluginLifecycleError::InvalidInput)
        ));
    }
}

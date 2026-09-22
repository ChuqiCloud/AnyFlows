use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, RwLock},
};

use af_db::SiteSettingsRepository;
use af_http::{
    FrontendAsset, FrontendAssetSource, FrontendTemplateCatalog, FrontendTemplateError,
    FrontendTemplatePreview, FrontendTemplateService, FrontendTemplateSummary,
};
use serde::Deserialize;
use tracing::{info, warn};

const MAX_TEMPLATE_ID_BYTES: usize = 64;
const MAX_TEMPLATE_NAME_BYTES: usize = 160;
const MAX_TEMPLATE_VERSION_BYTES: usize = 64;
const MAX_API_CONTRACT_BYTES: usize = 64;
const MAX_PREVIEW_BYTES: usize = 2 * 1024 * 1024;
const BUILTIN_CLASSIC: &str = "embedded";
const BUILTIN_NEXT: &str = "embedded-next";

const MAX_METADATA_BYTES: u64 = 64 * 1024;
const MAX_TEMPLATE_FILES: usize = 4_096;
const MAX_TEMPLATE_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_TEMPLATE_TOTAL_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateMetadata {
    schema_version: u32,
    id: String,
    name: String,
    version: String,
    api_contract: String,
    entry: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    preview: Option<String>,
}

#[derive(Clone)]
struct TemplateSnapshot {
    assets: HashMap<String, FrontendAsset>,
}

impl FrontendAssetSource for TemplateSnapshot {
    fn asset(&self, path: &str) -> Option<FrontendAsset> {
        self.assets.get(path).cloned()
    }
}

struct SelectedAssets {
    selected: Option<Arc<dyn FrontendAssetSource>>,
    embedded: Arc<dyn FrontendAssetSource>,
}

impl FrontendAssetSource for SelectedAssets {
    fn asset(&self, path: &str) -> Option<FrontendAsset> {
        match self.selected.as_ref() {
            Some(source) => source.asset(path),
            None => self.embedded.asset(path),
        }
    }
}

struct ScannedTemplate {
    summary: FrontendTemplateSummary,
    snapshot: Arc<TemplateSnapshot>,
    preview: Option<FrontendTemplatePreview>,
}

/// 运行时外部前端模板管理器。
///
/// 模板在扫描阶段一次性读入内存；HTTP 请求只访问当前快照，不在请求路径读磁盘。
/// 切换通过替换 SelectedAssets 完成，因此不会出现半套资源。
pub(crate) struct FrontendTemplateManager {
    root: PathBuf,
    embedded: Arc<dyn FrontendAssetSource>,
    repository: SiteSettingsRepository,
    active: RwLock<Arc<SelectedAssets>>,
    catalog: RwLock<FrontendTemplateCatalog>,
    templates: RwLock<HashMap<String, Arc<dyn FrontendAssetSource>>>,
    embedded_next: Arc<dyn FrontendAssetSource>,
    previews: RwLock<HashMap<String, FrontendTemplatePreview>>,
    mutation: tokio::sync::Mutex<()>,
}

impl FrontendTemplateManager {
    pub(crate) fn new(
        root: PathBuf,
        embedded: Arc<dyn FrontendAssetSource>,
        embedded_next: Arc<dyn FrontendAssetSource>,
        repository: SiteSettingsRepository,
        initial_active_id: Option<String>,
    ) -> Arc<Self> {
        let manager = Arc::new(Self {
            root,
            embedded_next,
            previews: RwLock::new(HashMap::new()),
            mutation: tokio::sync::Mutex::new(()),
            embedded: Arc::clone(&embedded),
            repository,
            active: RwLock::new(Arc::new(SelectedAssets {
                selected: None,
                embedded,
            })),
            catalog: RwLock::new(FrontendTemplateCatalog {
                active_id: None,
                templates: Vec::new(),
            }),
            templates: RwLock::new(HashMap::new()),
        });
        manager.scan_now();
        if let Some(id) = initial_active_id.as_deref().filter(|id| *id != "embedded")
            && manager.activate_now(id).is_err()
        {
            warn!(
                template_id = id,
                "数据库中的前端模板不存在或损坏，继续使用内嵌前端"
            );
        }
        manager
    }

    fn scan_now(&self) {
        self.replace_scanned(scan_directory(&self.root));
    }

    fn replace_scanned(&self, scanned: Vec<ScannedTemplate>) {
        let mut templates: HashMap<String, Arc<dyn FrontendAssetSource>> = HashMap::new();
        templates.insert(BUILTIN_CLASSIC.to_owned(), Arc::clone(&self.embedded));
        templates.insert(BUILTIN_NEXT.to_owned(), Arc::clone(&self.embedded_next));
        let mut previews = HashMap::from([
            (BUILTIN_CLASSIC.to_owned(), builtin_preview(false)),
            (BUILTIN_NEXT.to_owned(), builtin_preview(true)),
        ]);
        let mut summaries = vec![builtin_summary(false), builtin_summary(true)];
        for item in scanned {
            // Builtin IDs are reserved, so disk content cannot shadow trusted assets.
            if matches!(item.summary.id.as_str(), BUILTIN_CLASSIC | BUILTIN_NEXT) {
                warn!(
                    template_id = item.summary.id,
                    "外部模板使用了保留的内置模板 ID"
                );
                continue;
            }
            if let Some(preview) = item.preview {
                previews.insert(item.summary.id.clone(), preview);
            }
            summaries.push(item.summary.clone());
            if item.summary.valid {
                templates.insert(item.summary.id.clone(), item.snapshot);
            }
        }
        summaries.sort_by(|left, right| {
            right
                .builtin
                .cmp(&left.builtin)
                .then(left.id.cmp(&right.id))
        });
        let previous_active_id = self
            .catalog
            .read()
            .ok()
            .and_then(|catalog| catalog.active_id.clone());
        let active_id = previous_active_id
            .clone()
            .filter(|id| templates.contains_key(id));
        let active_snapshot = active_id.as_ref().and_then(|id| templates.get(id)).cloned();
        if let Ok(mut destination) = self.previews.write() {
            *destination = previews;
        }
        if let Ok(mut destination) = self.templates.write() {
            *destination = templates;
        }
        if let Ok(mut catalog) = self.catalog.write() {
            catalog.templates = summaries;
            catalog.active_id = active_id;
        }
        if let Ok(mut active) = self.active.write() {
            match active_snapshot {
                Some(snapshot) => {
                    *active = Arc::new(SelectedAssets {
                        selected: Some(snapshot),
                        embedded: Arc::clone(&self.embedded),
                    });
                }
                None if previous_active_id.is_some() => {
                    *active = Arc::new(SelectedAssets {
                        selected: None,
                        embedded: Arc::clone(&self.embedded),
                    });
                    warn!("当前外部前端模板已不可用，已回退到内嵌前端");
                }
                None => {}
            }
        }
    }

    fn activate_now(&self, id: &str) -> Result<(), FrontendTemplateError> {
        let snapshot = self
            .templates
            .read()
            .map_err(|_| FrontendTemplateError::Internal)?
            .get(id)
            .cloned()
            .ok_or(FrontendTemplateError::NotFound)?;
        let mut active = self
            .active
            .write()
            .map_err(|_| FrontendTemplateError::Internal)?;
        *active = Arc::new(SelectedAssets {
            selected: Some(snapshot),
            embedded: Arc::clone(&self.embedded),
        });
        if let Ok(mut catalog) = self.catalog.write() {
            catalog.active_id = Some(id.to_owned());
        }
        Ok(())
    }

    fn catalog(&self) -> Result<FrontendTemplateCatalog, FrontendTemplateError> {
        self.catalog
            .read()
            .map_err(|_| FrontendTemplateError::Internal)
            .map(|catalog| catalog.clone())
    }

    fn active_id(&self) -> Option<String> {
        self.catalog
            .read()
            .ok()
            .and_then(|catalog| catalog.active_id.clone())
    }
}

impl FrontendAssetSource for FrontendTemplateManager {
    fn asset(&self, path: &str) -> Option<FrontendAsset> {
        self.active
            .read()
            .ok()
            .and_then(|source| source.asset(path))
    }
}

impl FrontendTemplateService for FrontendTemplateManager {
    fn list(&self) -> af_http::FrontendTemplateFuture<'_, FrontendTemplateCatalog> {
        Box::pin(async { self.catalog() })
    }

    fn scan(&self) -> af_http::FrontendTemplateFuture<'_, FrontendTemplateCatalog> {
        Box::pin(async {
            let _guard = self.mutation.lock().await;
            let root = self.root.clone();
            let scanned = tokio::task::spawn_blocking(move || scan_directory(&root))
                .await
                .map_err(|_| FrontendTemplateError::Internal)?;
            self.replace_scanned(scanned);
            let active_id = self.active_id();
            let persisted = self
                .repository
                .settings()
                .await
                .map_err(|_| FrontendTemplateError::Internal)?;
            if persisted.frontend_template_id() != active_id.as_deref() {
                self.repository
                    .set_frontend_template_id(active_id)
                    .await
                    .map_err(|_| FrontendTemplateError::Conflict)?;
            }
            self.catalog()
        })
    }

    fn preview(&self, template_id: &str) -> Result<FrontendTemplatePreview, FrontendTemplateError> {
        self.previews
            .read()
            .map_err(|_| FrontendTemplateError::Internal)?
            .get(template_id)
            .cloned()
            .ok_or(FrontendTemplateError::NotFound)
    }

    fn activate(
        &self,
        template_id: Option<String>,
    ) -> af_http::FrontendTemplateFuture<'_, FrontendTemplateCatalog> {
        Box::pin(async move {
            let _guard = self.mutation.lock().await;
            let template_id = template_id.filter(|id| id != "embedded");
            let current = self.active_id();
            if current == template_id {
                let persisted = self
                    .repository
                    .settings()
                    .await
                    .map_err(|_| FrontendTemplateError::Internal)?;
                if persisted.frontend_template_id() == template_id.as_deref() {
                    return self.catalog();
                }
            }
            match template_id.as_deref() {
                Some(id) => {
                    validate_template_id(id).map_err(|_| FrontendTemplateError::InvalidInput)?;
                    if !self
                        .templates
                        .read()
                        .map_err(|_| FrontendTemplateError::Internal)?
                        .contains_key(id)
                    {
                        return Err(FrontendTemplateError::NotFound);
                    }
                    self.repository
                        .set_frontend_template_id(Some(id.to_owned()))
                        .await
                        .map_err(|_| FrontendTemplateError::Conflict)?;
                    self.activate_now(id)?;
                }
                None => {
                    self.repository
                        .set_frontend_template_id(None)
                        .await
                        .map_err(|_| FrontendTemplateError::Conflict)?;
                    let mut active = self
                        .active
                        .write()
                        .map_err(|_| FrontendTemplateError::Internal)?;
                    *active = Arc::new(SelectedAssets {
                        selected: None,
                        embedded: Arc::clone(&self.embedded),
                    });
                    if let Ok(mut catalog) = self.catalog.write() {
                        catalog.active_id = None;
                    }
                }
            }
            info!(template_id = ?template_id, "前端模板已切换");
            self.catalog()
        })
    }
}

fn preview_url(id: &str) -> String {
    format!("/api/admin/frontend-templates/{id}/preview")
}

fn builtin_summary(next: bool) -> FrontendTemplateSummary {
    let id = if next { BUILTIN_NEXT } else { BUILTIN_CLASSIC };
    FrontendTemplateSummary {
        id: id.to_owned(),
        name: if next {
            "AnyFlows Next"
        } else {
            "AnyFlows Classic"
        }
        .to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        api_contract: "0.2".to_owned(),
        builtin: true,
        description: None,
        author: Some("AnyFlows".to_owned()),
        preview_url: Some(preview_url(id)),
        valid: true,
        error: None,
    }
}

fn builtin_preview(next: bool) -> FrontendTemplatePreview {
    let content: &'static [u8] = if next {
        include_bytes!("frontend_previews/next.svg")
    } else {
        include_bytes!("frontend_previews/classic.svg")
    };
    FrontendTemplatePreview {
        asset: FrontendAsset::from_static(content),
        content_type: "image/svg+xml",
    }
}

fn load_preview(
    path: &str,
    assets: &HashMap<String, FrontendAsset>,
) -> Result<FrontendTemplatePreview, String> {
    if path.is_empty()
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err("预览图片必须是模板内的相对路径".to_owned());
    }
    let asset = assets.get(path).ok_or("预览图片不存在")?;
    let bytes = asset.content();
    if bytes.len() > MAX_PREVIEW_BYTES {
        return Err("预览图片不能超过 2 MiB".to_owned());
    }
    let content_type = match Path::new(path).extension().and_then(|ext| ext.to_str()) {
        Some("png") if bytes.starts_with(b"\x89PNG\r\n\x1a\n") => "image/png",
        Some("jpg" | "jpeg") if bytes.starts_with(&[0xff, 0xd8, 0xff]) => "image/jpeg",
        Some("webp") if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") => {
            "image/webp"
        }
        _ => return Err("预览图片必须是有效的 PNG、JPEG 或 WebP；不接受 HTML 或 SVG".to_owned()),
    };
    Ok(FrontendTemplatePreview {
        asset: asset.clone(),
        content_type,
    })
}

fn scan_directory(root: &Path) -> Vec<ScannedTemplate> {
    if let Ok(metadata) = fs::symlink_metadata(root)
        && (metadata.file_type().is_symlink() || !metadata.file_type().is_dir())
    {
        warn!(path = %root.display(), "前端模板根目录不能是符号链接或特殊文件");
        return Vec::new();
    }
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            warn!(path = %root.display(), error = %error, "读取前端模板目录失败");
            return Vec::new();
        }
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_dir() || file_type.is_symlink() {
                return None;
            }
            let id = entry.file_name().to_string_lossy().into_owned();
            match load_template(&id, &entry.path()) {
                Ok(template) => Some(template),
                Err(error) => Some(invalid_template(&id, error)),
            }
        })
        .collect()
}

fn load_template(id: &str, directory: &Path) -> Result<ScannedTemplate, String> {
    validate_template_id(id)?;
    if matches!(id, BUILTIN_CLASSIC | BUILTIN_NEXT) {
        return Err("外部模板不能使用内置模板的保留 ID".to_owned());
    }
    let metadata_path = directory.join("template.json");
    let metadata_type = fs::symlink_metadata(&metadata_path).map_err(|_| "缺少 template.json")?;
    if metadata_type.file_type().is_symlink() || !metadata_type.file_type().is_file() {
        return Err("template.json 不能是符号链接或特殊文件".to_owned());
    }
    let metadata_size = metadata_type.len();
    if metadata_size > MAX_METADATA_BYTES {
        return Err("template.json 超出大小限制".to_owned());
    }
    let metadata: TemplateMetadata =
        serde_json::from_slice(&fs::read(&metadata_path).map_err(|_| "无法读取 template.json")?)
            .map_err(|_| "template.json 格式无效")?;
    if metadata.schema_version != 1
        || metadata.id != id
        || metadata.entry != "index.html"
        || !valid_text(&metadata.name, MAX_TEMPLATE_NAME_BYTES)
        || !valid_text(&metadata.version, MAX_TEMPLATE_VERSION_BYTES)
        || !valid_text(&metadata.api_contract, MAX_API_CONTRACT_BYTES)
        || metadata
            .description
            .as_deref()
            .is_some_and(|value| !valid_text(value, 1_000))
        || metadata
            .author
            .as_deref()
            .is_some_and(|value| !valid_text(value, 160))
    {
        return Err("template.json 字段不符合规范".to_owned());
    }
    let mut assets = HashMap::new();
    let mut file_count = 0;
    let mut total_bytes = 0;
    collect_assets(
        directory,
        directory,
        &mut assets,
        &mut file_count,
        &mut total_bytes,
    )?;
    if !assets.contains_key("index.html") {
        return Err("缺少 index.html".to_owned());
    }
    let preview = metadata
        .preview
        .as_deref()
        .map(|path| load_preview(path, &assets))
        .transpose()?;
    let preview_url = preview.as_ref().map(|_| preview_url(id));
    Ok(ScannedTemplate {
        preview,
        summary: FrontendTemplateSummary {
            id: id.to_owned(),
            name: metadata.name,
            version: metadata.version,
            api_contract: metadata.api_contract,
            builtin: false,
            description: metadata.description,
            author: metadata.author,
            preview_url,
            valid: true,
            error: None,
        },
        snapshot: Arc::new(TemplateSnapshot { assets }),
    })
}

fn collect_assets(
    directory: &Path,
    root: &Path,
    assets: &mut HashMap<String, FrontendAsset>,
    file_count: &mut usize,
    total_bytes: &mut u64,
) -> Result<(), String> {
    for entry in fs::read_dir(directory).map_err(|_| "无法读取模板文件")? {
        let entry = entry.map_err(|_| "无法读取模板目录项")?;
        let file_type = entry.file_type().map_err(|_| "无法读取模板文件类型")?;
        if file_type.is_symlink() {
            return Err("模板不能包含符号链接".to_owned());
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_assets(&path, root, assets, file_count, total_bytes)?;
            continue;
        }
        if !file_type.is_file() {
            return Err("模板包含特殊文件".to_owned());
        }
        let relative = path.strip_prefix(root).map_err(|_| "模板路径越界")?;
        if relative == Path::new("template.json") {
            continue;
        }
        if relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err("模板路径包含非法段".to_owned());
        }
        *file_count = file_count.saturating_add(1);
        if *file_count > MAX_TEMPLATE_FILES {
            return Err("模板文件数量超出限制".to_owned());
        }
        let size = fs::metadata(&path)
            .map_err(|_| "无法读取模板文件大小")?
            .len();
        if size > MAX_TEMPLATE_FILE_BYTES
            || total_bytes.saturating_add(size) > MAX_TEMPLATE_TOTAL_BYTES
        {
            return Err("模板资源大小超出限制".to_owned());
        }
        let content = fs::read(&path).map_err(|_| "无法读取模板资源")?;
        *total_bytes = total_bytes.saturating_add(size);
        let key = relative.to_string_lossy().replace('\\', "/");
        assets.insert(key, FrontendAsset::from_bytes(content));
    }
    Ok(())
}

fn invalid_template(id: &str, error: String) -> ScannedTemplate {
    ScannedTemplate {
        preview: None,
        summary: FrontendTemplateSummary {
            id: id.to_owned(),
            name: String::new(),
            version: String::new(),
            api_contract: String::new(),
            builtin: false,
            description: None,
            author: None,
            preview_url: None,
            valid: false,
            error: Some(error),
        },
        snapshot: Arc::new(TemplateSnapshot {
            assets: HashMap::new(),
        }),
    }
}

fn validate_template_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > MAX_TEMPLATE_ID_BYTES
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        return Err("模板 ID 无效".to_owned());
    }
    Ok(())
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn preview_rejects_active_content_traversal_and_oversized_images() {
        let assets = HashMap::from([
            (
                "preview.png".to_owned(),
                FrontendAsset::from_bytes(b"\x89PNG\r\n\x1a\n".as_slice()),
            ),
            (
                "fake.png".to_owned(),
                FrontendAsset::from_bytes("<script>alert(1)</script>"),
            ),
            (
                "preview.svg".to_owned(),
                FrontendAsset::from_bytes("<svg/>"),
            ),
            (
                "huge.png".to_owned(),
                FrontendAsset::from_bytes(vec![0; MAX_PREVIEW_BYTES + 1]),
            ),
        ]);
        assert_eq!(
            load_preview("preview.png", &assets).unwrap().content_type,
            "image/png"
        );
        for path in [
            "fake.png",
            "preview.svg",
            "huge.png",
            "../preview.png",
            "/preview.png",
            "assets/../preview.png",
            "missing.png",
        ] {
            assert!(load_preview(path, &assets).is_err(), "{path}");
        }
        assert!(load_template(BUILTIN_CLASSIC, Path::new("unused")).is_err());
        assert!(load_template(BUILTIN_NEXT, Path::new("unused")).is_err());
    }

    #[tokio::test]
    async fn builtin_selection_survives_scan_restart_and_external_fallback() {
        use std::time::Duration;
        let database = crate::test_database::SqliteTestDatabase::new("frontend-gallery").await;
        let repository =
            SiteSettingsRepository::new(database.pool().clone(), Duration::from_secs(5)).unwrap();
        let root =
            std::env::temp_dir().join(format!("anyflows-gallery-runtime-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("demo")).unwrap();
        fs::write(root.join("demo/template.json"), r#"{"schema_version":1,"id":"demo","name":"Demo","version":"1","api_contract":"0.2","entry":"index.html","author":"Team","description":"Demo layout"}"#).unwrap();
        fs::write(root.join("demo/index.html"), "external").unwrap();
        let classic: Arc<dyn FrontendAssetSource> = Arc::new(TemplateSnapshot {
            assets: HashMap::from([(
                "index.html".to_owned(),
                FrontendAsset::from_bytes("classic"),
            )]),
        });
        let next: Arc<dyn FrontendAssetSource> = Arc::new(TemplateSnapshot {
            assets: HashMap::from([("index.html".to_owned(), FrontendAsset::from_bytes("next"))]),
        });
        let manager = FrontendTemplateManager::new(
            root.clone(),
            Arc::clone(&classic),
            Arc::clone(&next),
            repository.clone(),
            None,
        );
        let catalog = manager.list().await.unwrap();
        assert_eq!(
            catalog
                .templates
                .iter()
                .filter(|template| template.builtin)
                .count(),
            2
        );
        assert_eq!(catalog.templates.len(), 3);
        assert!(manager.preview(BUILTIN_CLASSIC).is_ok());
        assert!(manager.preview(BUILTIN_NEXT).is_ok());
        assert_eq!(
            manager.asset("index.html").unwrap().content().as_ref(),
            b"classic"
        );
        assert_eq!(manager.active_id(), None);

        manager
            .activate(Some(BUILTIN_NEXT.to_owned()))
            .await
            .unwrap();
        assert_eq!(
            manager.asset("index.html").unwrap().content().as_ref(),
            b"next"
        );
        assert_eq!(
            repository.settings().await.unwrap().frontend_template_id(),
            Some(BUILTIN_NEXT)
        );
        manager.scan().await.unwrap();
        assert_eq!(manager.active_id().as_deref(), Some(BUILTIN_NEXT));
        let restarted = FrontendTemplateManager::new(
            root.clone(),
            classic,
            next,
            repository.clone(),
            Some(BUILTIN_NEXT.to_owned()),
        );
        assert_eq!(
            restarted.asset("index.html").unwrap().content().as_ref(),
            b"next"
        );

        manager.activate(Some("demo".to_owned())).await.unwrap();
        assert_eq!(
            manager.asset("index.html").unwrap().content().as_ref(),
            b"external"
        );
        fs::remove_file(root.join("demo/index.html")).unwrap();
        manager.scan().await.unwrap();
        assert_eq!(manager.active_id(), None);
        assert_eq!(
            manager.asset("index.html").unwrap().content().as_ref(),
            b"classic"
        );
        assert_eq!(
            repository.settings().await.unwrap().frontend_template_id(),
            None
        );
        assert!(manager.activate(Some("demo".to_owned())).await.is_err());
        assert_eq!(manager.active_id(), None);
        manager
            .activate(Some(BUILTIN_NEXT.to_owned()))
            .await
            .unwrap();
        manager
            .activate(Some(BUILTIN_CLASSIC.to_owned()))
            .await
            .unwrap();
        assert_eq!(
            repository.settings().await.unwrap().frontend_template_id(),
            None
        );
        fs::remove_dir_all(root).unwrap();
        database.close().await;
    }

    #[test]
    fn template_id_rejects_path_traversal_and_uppercase() {
        assert!(validate_template_id("dark_console").is_ok());
        assert!(validate_template_id("../escape").is_err());
        assert!(validate_template_id("Dark").is_err());
    }

    #[test]
    fn valid_template_loads_metadata_and_assets() {
        let root =
            std::env::temp_dir().join(format!("anyflows-template-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(
            root.join("template.json"),
            r#"{"schema_version":1,"id":"demo","name":"Demo","version":"1.0.0","api_contract":"0.2","entry":"index.html"}"#,
        )
        .unwrap();
        fs::write(root.join("index.html"), "<main>demo</main>").unwrap();
        fs::write(root.join("assets/app.js"), "console.log('demo')").unwrap();
        let loaded = load_template("demo", &root).unwrap();
        assert!(loaded.summary.valid);
        assert!(loaded.snapshot.asset("index.html").is_some());
        assert!(loaded.snapshot.asset("assets/app.js").is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn template_without_entry_is_rejected() {
        let root = std::env::temp_dir().join(format!(
            "anyflows-template-missing-index-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("template.json"),
            r#"{"schema_version":1,"id":"demo","name":"Demo","version":"1.0.0","api_contract":"0.2","entry":"index.html"}"#,
        )
        .unwrap();
        assert!(load_template("demo", &root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selected_template_does_not_mix_with_embedded_assets() {
        let selected_asset = FrontendAsset::from_bytes("selected");
        let embedded_asset = FrontendAsset::from_bytes("embedded");
        let mut selected_assets = HashMap::new();
        selected_assets.insert("index.html".to_owned(), selected_asset.clone());
        let selected = SelectedAssets {
            selected: Some(Arc::new(TemplateSnapshot {
                assets: selected_assets,
            })),
            embedded: Arc::new(TemplateSnapshot {
                assets: HashMap::from([(
                    String::from("assets/missing.js"),
                    FrontendAsset::from_bytes("embedded"),
                )]),
            }),
        };
        assert!(selected.asset("index.html").is_some());
        assert!(selected.asset("assets/missing.js").is_none());

        let fallback = SelectedAssets {
            selected: None,
            embedded: Arc::new(TemplateSnapshot {
                assets: HashMap::from([(String::from("index.html"), embedded_asset)]),
            }),
        };
        assert!(fallback.asset("index.html").is_some());
    }

    #[test]
    fn scan_rejects_a_file_as_the_template_root() {
        let root = std::env::temp_dir().join(format!(
            "anyflows-template-root-file-{}",
            std::process::id()
        ));
        let _ = fs::remove_file(&root);
        fs::write(&root, "not a directory").unwrap();
        assert!(scan_directory(&root).is_empty());
        fs::remove_file(root).unwrap();
    }
}

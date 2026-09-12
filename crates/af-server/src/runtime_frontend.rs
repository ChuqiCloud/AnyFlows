use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, RwLock},
};

use af_db::SiteSettingsRepository;
use af_http::{
    FrontendAsset, FrontendAssetSource, FrontendTemplateCatalog, FrontendTemplateError,
    FrontendTemplateService, FrontendTemplateSummary,
};
use serde::Deserialize;
use tracing::{info, warn};

const MAX_TEMPLATE_ID_BYTES: usize = 64;
const MAX_TEMPLATE_NAME_BYTES: usize = 160;
const MAX_TEMPLATE_VERSION_BYTES: usize = 64;
const MAX_API_CONTRACT_BYTES: usize = 64;
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
    selected: Option<Arc<TemplateSnapshot>>,
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
    templates: RwLock<HashMap<String, Arc<TemplateSnapshot>>>,
}

impl FrontendTemplateManager {
    pub(crate) fn new(
        root: PathBuf,
        embedded: Arc<dyn FrontendAssetSource>,
        repository: SiteSettingsRepository,
        initial_active_id: Option<String>,
    ) -> Arc<Self> {
        let manager = Arc::new(Self {
            root,
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
        let scanned = scan_directory(&self.root);
        let mut templates = HashMap::new();
        let mut summaries = Vec::with_capacity(scanned.len());
        for item in scanned {
            summaries.push(item.summary.clone());
            if item.summary.valid {
                templates.insert(item.summary.id.clone(), item.snapshot);
            }
        }
        summaries.sort_by(|left, right| left.id.cmp(&right.id));
        let previous_active_id = self
            .catalog
            .read()
            .ok()
            .and_then(|catalog| catalog.active_id.clone());
        let active_id = previous_active_id
            .clone()
            .filter(|id| templates.contains_key(id));
        let active_snapshot = active_id.as_ref().and_then(|id| templates.get(id)).cloned();
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
            self.scan_now();
            self.catalog()
        })
    }

    fn activate(
        &self,
        template_id: Option<String>,
    ) -> af_http::FrontendTemplateFuture<'_, FrontendTemplateCatalog> {
        Box::pin(async move {
            let template_id = template_id.filter(|id| id != "embedded");
            let current = self.active_id();
            if current == template_id {
                return self.catalog();
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
    Ok(ScannedTemplate {
        summary: FrontendTemplateSummary {
            id: id.to_owned(),
            name: metadata.name,
            version: metadata.version,
            api_contract: metadata.api_contract,
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
        summary: FrontendTemplateSummary {
            id: id.to_owned(),
            name: String::new(),
            version: String::new(),
            api_contract: String::new(),
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

use std::{collections::HashMap, sync::LazyLock};

use af_http::{FrontendAsset, FrontendAssetSource};
use include_dir::{Dir, include_dir};

static FRONTEND_DIST: Dir<'static> = include_dir!("$OUT_DIR/web-dist");
static FRONTEND_NEXT_DIST: Dir<'static> = include_dir!("$OUT_DIR/web-next-dist");
static FRONTEND_NEXT_ASSETS: LazyLock<HashMap<String, FrontendAsset>> = LazyLock::new(|| {
    let mut assets = HashMap::new();
    collect_assets(&FRONTEND_NEXT_DIST, &mut assets);
    assets
});
static FRONTEND_ASSETS: LazyLock<HashMap<String, FrontendAsset>> = LazyLock::new(|| {
    let mut assets = HashMap::new();
    collect_assets(&FRONTEND_DIST, &mut assets);
    assets
});

/// 提供编译进服务二进制的前端资源。
#[derive(Debug)]
pub(crate) struct EmbeddedFrontendAssets;

/// 新版前端与经典前端独立内嵌，不依赖运行目录中的外部模板。
#[derive(Debug)]
pub(crate) struct EmbeddedNextFrontendAssets;

impl FrontendAssetSource for EmbeddedNextFrontendAssets {
    fn asset(&self, path: &str) -> Option<FrontendAsset> {
        FRONTEND_NEXT_ASSETS.get(path).cloned()
    }
}

impl FrontendAssetSource for EmbeddedFrontendAssets {
    fn asset(&self, path: &str) -> Option<FrontendAsset> {
        FRONTEND_ASSETS.get(path).cloned()
    }
}

fn collect_assets(directory: &'static Dir<'static>, assets: &mut HashMap<String, FrontendAsset>) {
    for file in directory.files() {
        let path = file.path().to_string_lossy().replace('\\', "/");
        assets.insert(path, FrontendAsset::from_static(file.contents()));
    }
    for child in directory.dirs() {
        collect_assets(child, assets);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_frontend_always_contains_index() {
        assert!(EmbeddedFrontendAssets.asset("index.html").is_some());
        assert!(EmbeddedNextFrontendAssets.asset("index.html").is_some());
    }
}

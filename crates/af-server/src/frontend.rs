use std::{collections::HashMap, sync::LazyLock};

use af_http::{FrontendAsset, FrontendAssetSource};
use include_dir::{Dir, include_dir};

static FRONTEND_NEXT_DIST: Dir<'static> = include_dir!("$OUT_DIR/web-next-dist");
static FRONTEND_NEXT_ASSETS: LazyLock<HashMap<String, FrontendAsset>> = LazyLock::new(|| {
    let mut assets = HashMap::new();
    collect_assets(&FRONTEND_NEXT_DIST, &mut assets);
    assets
});
/// 提供编译进服务二进制的 Next 前端资源。
#[derive(Debug)]
pub(crate) struct EmbeddedNextFrontendAssets;

impl FrontendAssetSource for EmbeddedNextFrontendAssets {
    fn asset(&self, path: &str) -> Option<FrontendAsset> {
        FRONTEND_NEXT_ASSETS.get(path).cloned()
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
    fn embedded_frontend_contains_next_index() {
        assert!(EmbeddedNextFrontendAssets.asset("index.html").is_some());
    }
}

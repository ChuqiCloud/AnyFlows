//! 保证开发机与 CI 行为一致的仓库质量任务。

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand};

const PRODUCT_CRATES: [&str; 16] = [
    "af-account",
    "af-adapter",
    "af-admin",
    "af-analytics",
    "af-billing",
    "af-cache",
    "af-config",
    "af-db",
    "af-domain",
    "af-http",
    "af-httpclient",
    "af-protocol",
    "af-relay",
    "af-scheduler",
    "af-server",
    "af-telemetry",
];

const TOOLING_CRATES: [&str; 1] = ["xtask"];
const OPENAPI_OUTPUT_PATH: &str = "web/openapi/openapi.json";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    CheckDependencies,
    ExportOpenApi,
    CheckOpenApi,
}

#[derive(Debug, Clone)]
struct DeclaredDependency {
    from: String,
    to: String,
    internal: bool,
    kind: String,
    target: Option<String>,
    optional: bool,
    alias: Option<String>,
    path: Option<String>,
    path_dependency: bool,
    manifest_path: String,
}

impl DeclaredDependency {
    fn detail(&self) -> String {
        let target = self.target.as_deref().unwrap_or("all");
        let alias = self.alias.as_deref().unwrap_or("<none>");
        let path = self.path.as_deref().unwrap_or("<none>");
        format!(
            "kind={}, target={target}, optional={}, alias={alias}, path={path}, manifest={}",
            self.kind, self.optional, self.manifest_path
        )
    }
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Violation {
    from: String,
    to: String,
    rule: &'static str,
    detail: String,
}

impl Violation {
    fn render(&self) -> String {
        format!(
            "ERROR [{}] {} -> {} ({})",
            self.rule, self.from, self.to, self.detail
        )
    }
}

fn main() {
    let exit_code = match run() {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("ERROR [xtask] {error}");
            2
        }
    };
    process::exit(exit_code);
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match parse_command(&args)? {
        Command::CheckDependencies => check_dependencies(),
        Command::ExportOpenApi => export_openapi(),
        Command::CheckOpenApi => check_openapi(),
    }
}

fn parse_command(args: &[String]) -> Result<Command, io::Error> {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["check-deps"] => Ok(Command::CheckDependencies),
        ["openapi", "export"] => Ok(Command::ExportOpenApi),
        ["openapi", "check"] => Ok(Command::CheckOpenApi),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: cargo xtask <check-deps|openapi export|openapi check>",
        )),
    }
}

fn check_dependencies() -> Result<(), Box<dyn Error>> {
    let mut metadata_command = MetadataCommand::new();
    metadata_command
        .no_deps()
        .other_options(vec!["--locked".to_owned()]);
    let metadata = metadata_command.exec()?;
    let violations = validate_metadata(&metadata);
    if violations.is_empty() {
        println!("crate dependency policy passed");
        return Ok(());
    }

    for violation in &violations {
        eprintln!("{}", violation.render());
    }
    process::exit(1);
}

fn export_openapi() -> Result<(), Box<dyn Error>> {
    let output = render_openapi()?;
    let path = openapi_output_path();
    fs::write(&path, &output)?;
    let next_path = workspace_root().join("web-next/openapi/openapi.json");
    fs::create_dir_all(next_path.parent().expect("OpenAPI output has a parent"))?;
    fs::write(&next_path, &output)?;
    println!("OpenAPI 已导出到 {}", display_workspace_path(&path));
    Ok(())
}

fn check_openapi() -> Result<(), Box<dyn Error>> {
    let expected = render_openapi()?;
    let path = openapi_output_path();
    let committed = fs::read_to_string(&path)?;
    if committed == expected
        && fs::read_to_string(workspace_root().join("web-next/openapi/openapi.json"))? == expected
    {
        println!("OpenAPI 契约与 Rust 定义一致");
        return Ok(());
    }

    let difference = if committed == expected {
        "web-next/openapi/openapi.json 已过期".to_owned()
    } else {
        describe_first_openapi_difference(&committed, &expected)
    };

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "{} 已过期，请运行 `cargo xtask openapi export`\n{difference}",
            display_workspace_path(&path),
        ),
    )
    .into())
}

fn describe_first_openapi_difference(committed: &str, expected: &str) -> String {
    let committed_lines = committed.split_inclusive('\n').collect::<Vec<_>>();
    let expected_lines = expected.split_inclusive('\n').collect::<Vec<_>>();
    let line_index = (0..committed_lines.len().max(expected_lines.len()))
        .find(|&index| committed_lines.get(index) != expected_lines.get(index))
        .expect("不同的 OpenAPI 文档必须存在首个差异行");

    format!(
        "OpenAPI 首个差异位于第 {} 行：已提交={:?}，Rust 导出={:?}",
        line_index + 1,
        committed_lines.get(line_index),
        expected_lines.get(line_index)
    )
}

fn render_openapi() -> Result<String, serde_json::Error> {
    af_http::openapi_document()
        .to_pretty_json()
        .map(|document| format!("{document}\n"))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn openapi_output_path() -> PathBuf {
    workspace_root().join(OPENAPI_OUTPUT_PATH)
}

fn display_workspace_path(path: &Path) -> String {
    path.strip_prefix(workspace_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn validate_metadata(metadata: &Metadata) -> Vec<Violation> {
    let workspace_names = metadata
        .workspace_packages()
        .into_iter()
        .map(|package| package.name.to_string())
        .collect::<BTreeSet<_>>();

    let workspace_paths = metadata
        .workspace_packages()
        .into_iter()
        .filter_map(|package| {
            package
                .manifest_path
                .parent()
                .map(|directory| (normalize_path(directory.as_str()), package.name.to_string()))
        })
        .collect::<BTreeMap<_, _>>();

    let dependencies = metadata
        .workspace_packages()
        .into_iter()
        .filter(|package| is_product_crate(package.name.as_ref()))
        .flat_map(|package| {
            let from = package.name.to_string();
            let manifest_path = normalize_path(package.manifest_path.as_str());
            let workspace_paths = &workspace_paths;
            package.dependencies.iter().map(move |dependency| {
                let path = dependency
                    .path
                    .as_ref()
                    .map(|path| normalize_path(path.as_str()));
                let (to, internal) =
                    resolve_dependency_target(&dependency.name, path.as_deref(), workspace_paths);

                DeclaredDependency {
                    from: from.clone(),
                    to,
                    internal,
                    kind: dependency_kind_name(dependency.kind).to_owned(),
                    target: dependency.target.as_ref().map(ToString::to_string),
                    optional: dependency.optional,
                    alias: dependency.rename.clone(),
                    path,
                    path_dependency: dependency.path.is_some(),
                    manifest_path: manifest_path.clone(),
                }
            })
        })
        .collect::<Vec<_>>();

    validate_workspace(&workspace_names, &dependencies)
}

fn validate_workspace(
    workspace_names: &BTreeSet<String>,
    dependencies: &[DeclaredDependency],
) -> Vec<Violation> {
    let expected_names = PRODUCT_CRATES
        .iter()
        .chain(TOOLING_CRATES.iter())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut violations = Vec::new();

    for missing in expected_names
        .iter()
        .filter(|name| !workspace_names.contains(**name))
    {
        violations.push(Violation {
            from: "<workspace>".to_owned(),
            to: (*missing).to_owned(),
            rule: "required-crate-missing",
            detail: "crate is declared by the architecture policy".to_owned(),
        });
    }

    for unknown in workspace_names
        .iter()
        .filter(|name| !expected_names.contains(name.as_str()))
    {
        violations.push(Violation {
            from: unknown.clone(),
            to: "<workspace>".to_owned(),
            rule: "workspace-crate-not-declared",
            detail: "add the crate to the architecture policy before use".to_owned(),
        });
    }

    for dependency in dependencies {
        if dependency.path_dependency && !dependency.internal {
            violations.push(Violation {
                from: dependency.from.clone(),
                to: dependency.to.clone(),
                rule: "path-dependency-outside-workspace",
                detail: dependency.detail(),
            });
            continue;
        }

        if is_product_crate(&dependency.to) && !dependency.internal {
            violations.push(Violation {
                from: dependency.from.clone(),
                to: dependency.to.clone(),
                rule: "workspace-crate-from-external-source",
                detail: dependency.detail(),
            });
            continue;
        }

        if dependency.internal
            && !allowed_internal_dependencies(&dependency.from).contains(&dependency.to.as_str())
        {
            violations.push(Violation {
                from: dependency.from.clone(),
                to: dependency.to.clone(),
                rule: "internal-edge-not-allowed",
                detail: dependency.detail(),
            });
        }

        if !dependency.internal
            && dependency.to != "reqwest"
            && restricts_external_dependencies(&dependency.from)
            && !allowed_restricted_external_dependency(dependency)
        {
            violations.push(Violation {
                from: dependency.from.clone(),
                to: dependency.to.clone(),
                rule: "restricted-crate-external-dependency-not-allowed",
                detail: dependency.detail(),
            });
        }

        if dependency.to == "axum" && dependency.from != "af-http" {
            violations.push(Violation {
                from: dependency.from.clone(),
                to: dependency.to.clone(),
                rule: "http-framework-owned-by-af-http",
                detail: dependency.detail(),
            });
        }

        if dependency.to == "reqwest" && dependency.from != "af-httpclient" {
            violations.push(Violation {
                from: dependency.from.clone(),
                to: dependency.to.clone(),
                rule: "http-client-owned-by-af-httpclient",
                detail: dependency.detail(),
            });
        }

        if dependency.to == "metrics" && dependency.from != "af-telemetry" {
            violations.push(Violation {
                from: dependency.from.clone(),
                to: dependency.to.clone(),
                rule: "metrics-facade-owned-by-af-telemetry",
                detail: dependency.detail(),
            });
        }
    }

    violations.sort();
    violations.dedup();
    violations
}

fn is_product_crate(name: &str) -> bool {
    PRODUCT_CRATES.contains(&name)
}

fn restricts_external_dependencies(name: &str) -> bool {
    matches!(name, "af-adapter" | "af-domain" | "af-protocol")
}

fn allowed_restricted_external_dependency(dependency: &DeclaredDependency) -> bool {
    // 受限 crate 的例外精确到依赖种类，防止测试工具进入生产或构建依赖。
    let reviewed = matches!(
        (
            dependency.from.as_str(),
            dependency.to.as_str(),
            dependency.kind.as_str(),
        ),
        // DNS 证明领域只使用熵源和摘要算法，不引入网络或运行时依赖。
        ("af-domain", "getrandom", "normal")
            | ("af-domain", "serde", "normal")
            | ("af-domain", "serde_json", "dev")
            | ("af-domain", "regex", "normal")
            | ("af-domain", "rust_decimal", "normal")
            | ("af-domain", "sha2", "normal")
            | ("af-domain", "thiserror", "normal")
            | ("af-adapter", "async-trait", "normal")
            | ("af-adapter", "aws-credential-types", "normal")
            | ("af-adapter", "aws-sigv4", "normal")
            | ("af-adapter", "futures-core", "normal")
            | ("af-adapter", "futures-util", "normal")
            | ("af-adapter", "jsonwebtoken", "normal")
            | ("af-adapter", "percent-encoding", "normal")
            | ("af-adapter", "serde", "normal")
            | ("af-adapter", "serde_json", "normal")
            | ("af-adapter", "sha2", "normal")
            | ("af-adapter", "thiserror", "normal")
            | ("af-adapter", "tokio", "normal")
            | ("af-adapter", "url", "normal")
            | ("af-adapter", "zeroize", "normal")
            | ("af-protocol", "base64", "normal")
            | ("af-protocol", "bytes", "normal")
            | ("af-protocol", "insta", "dev")
            | ("af-protocol", "serde", "normal")
            | ("af-protocol", "serde_json", "normal")
            | ("af-protocol", "url", "normal")
    );
    reviewed && dependency.target.is_none() && !dependency.optional && dependency.alias.is_none()
}

fn allowed_internal_dependencies(name: &str) -> &'static [&'static str] {
    match name {
        "af-domain" => &[],
        "af-config" => &["af-domain"],
        "af-telemetry" => &["af-config"],
        "af-db" | "af-cache" | "af-httpclient" => &["af-config", "af-domain"],
        "af-protocol" => &["af-domain"],
        "af-adapter" => &["af-domain", "af-httpclient", "af-protocol"],
        "af-billing" => &["af-cache", "af-config", "af-db", "af-domain", "af-protocol"],
        "af-scheduler" => &["af-cache", "af-config", "af-db", "af-domain"],
        "af-account" => &[
            "af-cache",
            "af-config",
            "af-db",
            "af-domain",
            "af-httpclient",
        ],
        "af-analytics" => &["af-cache", "af-config", "af-db", "af-domain"],
        "af-admin" => &[
            "af-account",
            "af-analytics",
            "af-billing",
            "af-cache",
            "af-config",
            "af-db",
            "af-domain",
            // 企业 SSO 回调编排只能通过受控 HTTP Client 访问 Provider。
            "af-httpclient",
            "af-scheduler",
        ],
        "af-relay" => &[
            "af-account",
            "af-adapter",
            "af-analytics",
            "af-billing",
            "af-config",
            "af-domain",
            "af-protocol",
            "af-scheduler",
        ],
        "af-http" => &[
            "af-admin",
            "af-analytics",
            "af-billing",
            "af-config",
            "af-domain",
            "af-protocol",
            "af-relay",
            "af-telemetry",
        ],
        "af-server" => &PRODUCT_CRATES,
        _ => &[],
    }
}

fn dependency_kind_name(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::Normal => "normal",
        DependencyKind::Development => "dev",
        DependencyKind::Build => "build",
        _ => "unknown",
    }
}

fn resolve_dependency_target(
    package_name: &str,
    normalized_path: Option<&str>,
    workspace_paths: &BTreeMap<String, String>,
) -> (String, bool) {
    // 即使依赖设置了别名，也以 workspace 路径解析出的包名为准。
    normalized_path
        .and_then(|path| workspace_paths.get(path))
        .map_or_else(
            || (package_name.to_owned(), false),
            |workspace_name| (workspace_name.clone(), true),
        )
}

fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let normalized = normalized.trim_end_matches('/');
    if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_repository_commands() {
        assert_eq!(
            parse_command(&["check-deps".to_owned()]).unwrap(),
            Command::CheckDependencies
        );
        assert_eq!(
            parse_command(&["openapi".to_owned(), "export".to_owned()]).unwrap(),
            Command::ExportOpenApi
        );
        assert_eq!(
            parse_command(&["openapi".to_owned(), "check".to_owned()]).unwrap(),
            Command::CheckOpenApi
        );
        assert!(parse_command(&["openapi".to_owned()]).is_err());
    }

    #[test]
    fn describes_the_first_openapi_difference() {
        let difference = describe_first_openapi_difference("one\ntwo\n", "one\nthree\n");

        assert_eq!(
            difference,
            "OpenAPI 首个差异位于第 2 行：已提交=Some(\"two\\n\")，Rust 导出=Some(\"three\\n\")"
        );
    }

    fn baseline_workspace() -> BTreeSet<String> {
        PRODUCT_CRATES
            .iter()
            .chain(TOOLING_CRATES.iter())
            .map(|name| (*name).to_owned())
            .collect()
    }

    fn dependency(from: &str, to: &str, internal: bool) -> DeclaredDependency {
        DeclaredDependency {
            from: from.to_owned(),
            to: to.to_owned(),
            internal,
            kind: "normal".to_owned(),
            target: None,
            optional: false,
            alias: None,
            path: internal.then(|| format!("crates/{to}")),
            path_dependency: internal,
            manifest_path: format!("crates/{from}/Cargo.toml"),
        }
    }

    #[test]
    fn accepts_declared_internal_edge() {
        let violations = validate_workspace(
            &baseline_workspace(),
            &[dependency("af-protocol", "af-domain", true)],
        );

        assert!(violations.is_empty());
    }

    #[test]
    fn rejects_application_dependency_from_adapter() {
        let mut edge = dependency("af-adapter", "af-relay", true);
        edge.kind = "build".to_owned();
        edge.target = Some("cfg(unix)".to_owned());
        edge.optional = true;
        edge.alias = Some("relay_alias".to_owned());

        let violations = validate_workspace(&baseline_workspace(), &[edge]);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "internal-edge-not-allowed");
        assert!(violations[0].detail.contains("kind=build"));
        assert!(violations[0].detail.contains("target=cfg(unix)"));
        assert!(violations[0].detail.contains("optional=true"));
        assert!(violations[0].detail.contains("alias=relay_alias"));
    }

    #[test]
    fn accepts_reviewed_domain_dependencies() {
        let serde = dependency("af-domain", "serde", false);
        let mut serde_json = dependency("af-domain", "serde_json", false);
        serde_json.kind = "dev".to_owned();
        let regex = dependency("af-domain", "regex", false);
        let rust_decimal = dependency("af-domain", "rust_decimal", false);
        let getrandom = dependency("af-domain", "getrandom", false);
        let sha2 = dependency("af-domain", "sha2", false);
        let thiserror = dependency("af-domain", "thiserror", false);

        let violations = validate_workspace(
            &baseline_workspace(),
            &[
                serde,
                serde_json,
                regex,
                rust_decimal,
                getrandom,
                sha2,
                thiserror,
            ],
        );

        assert!(violations.is_empty());
    }

    #[test]
    fn accepts_reviewed_adapter_dependencies() {
        let dependencies = [
            dependency("af-adapter", "async-trait", false),
            dependency("af-adapter", "aws-credential-types", false),
            dependency("af-adapter", "aws-sigv4", false),
            dependency("af-adapter", "futures-core", false),
            dependency("af-adapter", "futures-util", false),
            dependency("af-adapter", "jsonwebtoken", false),
            dependency("af-adapter", "percent-encoding", false),
            dependency("af-adapter", "serde", false),
            dependency("af-adapter", "serde_json", false),
            dependency("af-adapter", "sha2", false),
            dependency("af-adapter", "thiserror", false),
            dependency("af-adapter", "tokio", false),
            dependency("af-adapter", "url", false),
            dependency("af-adapter", "zeroize", false),
        ];

        let violations = validate_workspace(&baseline_workspace(), &dependencies);

        assert!(violations.is_empty());
    }

    #[test]
    fn accepts_reviewed_protocol_dependencies() {
        let mut dependencies = ["base64", "bytes", "serde", "serde_json", "url"]
            .map(|name| dependency("af-protocol", name, false))
            .to_vec();
        let mut insta = dependency("af-protocol", "insta", false);
        insta.kind = "dev".to_owned();
        dependencies.push(insta);

        let violations = validate_workspace(&baseline_workspace(), &dependencies);

        assert!(violations.is_empty());
    }

    #[test]
    fn rejects_unreviewed_external_dependency_from_domain() {
        let violations = validate_workspace(
            &baseline_workspace(),
            &[dependency("af-domain", "tokio", false)],
        );

        assert_eq!(violations.len(), 1);
        assert_eq!(
            violations[0].rule,
            "restricted-crate-external-dependency-not-allowed"
        );
    }

    #[test]
    fn rejects_domain_serialization_dependencies_in_wrong_sections() {
        let serde_json = dependency("af-domain", "serde_json", false);
        let mut serde = dependency("af-domain", "serde", false);
        serde.kind = "build".to_owned();

        let violations = validate_workspace(&baseline_workspace(), &[serde_json, serde]);

        assert_eq!(violations.len(), 2);
        assert!(
            violations
                .iter()
                .all(|violation| violation.rule
                    == "restricted-crate-external-dependency-not-allowed")
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.detail.contains("kind=build"))
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.detail.contains("kind=normal"))
        );
    }

    #[test]
    fn rejects_unreviewed_domain_serialization_dependency_shapes() {
        let mut optional = dependency("af-domain", "serde", false);
        optional.optional = true;
        let mut targeted = dependency("af-domain", "serde", false);
        targeted.target = Some("cfg(unix)".to_owned());
        let mut aliased = dependency("af-domain", "serde", false);
        aliased.alias = Some("serde_alias".to_owned());

        let violations = validate_workspace(&baseline_workspace(), &[optional, targeted, aliased]);

        assert_eq!(violations.len(), 3);
        assert!(
            violations
                .iter()
                .all(|violation| violation.rule
                    == "restricted-crate-external-dependency-not-allowed")
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.detail.contains("optional=true"))
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.detail.contains("target=cfg(unix)"))
        );
        assert!(
            violations
                .iter()
                .any(|violation| violation.detail.contains("alias=serde_alias"))
        );
    }

    #[test]
    fn rejects_unreviewed_domain_error_dependency_shapes() {
        let mut wrong_kind = dependency("af-domain", "thiserror", false);
        wrong_kind.kind = "dev".to_owned();
        let mut optional = dependency("af-domain", "thiserror", false);
        optional.optional = true;
        let mut targeted = dependency("af-domain", "thiserror", false);
        targeted.target = Some("cfg(unix)".to_owned());
        let mut aliased = dependency("af-domain", "thiserror", false);
        aliased.alias = Some("thiserror_alias".to_owned());

        let violations = validate_workspace(
            &baseline_workspace(),
            &[wrong_kind, optional, targeted, aliased],
        );

        assert_eq!(violations.len(), 4);
        assert!(
            violations
                .iter()
                .all(|violation| violation.rule
                    == "restricted-crate-external-dependency-not-allowed")
        );
        for detail in [
            "kind=dev",
            "optional=true",
            "target=cfg(unix)",
            "alias=thiserror_alias",
        ] {
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.detail.contains(detail))
            );
        }
    }

    #[test]
    fn rejects_unreviewed_protocol_dependency_shapes() {
        let dependencies = ["base64", "bytes", "serde", "serde_json", "url"]
            .into_iter()
            .flat_map(|name| {
                let mut wrong_kind = dependency("af-protocol", name, false);
                wrong_kind.kind = "dev".to_owned();
                let mut optional = dependency("af-protocol", name, false);
                optional.optional = true;
                let mut targeted = dependency("af-protocol", name, false);
                targeted.target = Some("cfg(unix)".to_owned());
                let mut aliased = dependency("af-protocol", name, false);
                aliased.alias = Some(format!("{name}_alias"));
                [wrong_kind, optional, targeted, aliased]
            })
            .collect::<Vec<_>>();

        let violations = validate_workspace(&baseline_workspace(), &dependencies);

        assert_eq!(violations.len(), 20);
        assert!(
            violations
                .iter()
                .all(|violation| violation.rule
                    == "restricted-crate-external-dependency-not-allowed")
        );
        for detail in [
            "kind=dev",
            "optional=true",
            "target=cfg(unix)",
            "alias=base64_alias",
            "alias=bytes_alias",
            "alias=serde_alias",
            "alias=serde_json_alias",
            "alias=url_alias",
        ] {
            assert!(
                violations
                    .iter()
                    .any(|violation| violation.detail.contains(detail))
            );
        }
    }

    #[test]
    fn rejects_snapshot_dependency_outside_exact_dev_shape() {
        let normal = dependency("af-protocol", "insta", false);
        let mut optional = dependency("af-protocol", "insta", false);
        optional.kind = "dev".to_owned();
        optional.optional = true;
        let mut targeted = dependency("af-protocol", "insta", false);
        targeted.kind = "dev".to_owned();
        targeted.target = Some("cfg(unix)".to_owned());
        let mut aliased = dependency("af-protocol", "insta", false);
        aliased.kind = "dev".to_owned();
        aliased.alias = Some("insta_alias".to_owned());

        let violations = validate_workspace(
            &baseline_workspace(),
            &[normal, optional, targeted, aliased],
        );

        assert_eq!(violations.len(), 4);
        assert!(
            violations
                .iter()
                .all(|violation| violation.rule
                    == "restricted-crate-external-dependency-not-allowed")
        );
    }

    #[test]
    fn rejects_axum_outside_http_crate() {
        let violations = validate_workspace(
            &baseline_workspace(),
            &[dependency("af-server", "axum", false)],
        );

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "http-framework-owned-by-af-http");
    }

    #[test]
    fn accepts_http_dependency_on_telemetry_contracts() {
        let violations = validate_workspace(
            &baseline_workspace(),
            &[dependency("af-http", "af-telemetry", true)],
        );

        assert!(violations.is_empty());
    }

    #[test]
    fn rejects_reqwest_outside_httpclient_crate() {
        let violations = validate_workspace(
            &baseline_workspace(),
            &[dependency("af-relay", "reqwest", false)],
        );

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "http-client-owned-by-af-httpclient");
    }

    #[test]
    fn rejects_metrics_facade_outside_telemetry_crate() {
        let violations = validate_workspace(
            &baseline_workspace(),
            &[dependency("af-relay", "metrics", false)],
        );

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "metrics-facade-owned-by-af-telemetry");
    }

    #[test]
    fn rejects_external_dependency_from_protocol() {
        let violations = validate_workspace(
            &baseline_workspace(),
            &[dependency("af-protocol", "tokio", false)],
        );

        assert_eq!(violations.len(), 1);
        assert_eq!(
            violations[0].rule,
            "restricted-crate-external-dependency-not-allowed"
        );
    }

    #[test]
    fn rejects_storage_dependency_from_adapter() {
        let violations = validate_workspace(
            &baseline_workspace(),
            &[dependency("af-adapter", "sea-orm", false)],
        );

        assert_eq!(violations.len(), 1);
        assert_eq!(
            violations[0].rule,
            "restricted-crate-external-dependency-not-allowed"
        );
    }

    #[test]
    fn rejects_external_test_dependency_from_protocol() {
        let mut edge = dependency("af-protocol", "tokio", false);
        edge.kind = "dev".to_owned();

        let violations = validate_workspace(&baseline_workspace(), &[edge]);

        assert_eq!(violations.len(), 1);
        assert_eq!(
            violations[0].rule,
            "restricted-crate-external-dependency-not-allowed"
        );
    }

    #[test]
    fn rejects_path_dependency_outside_workspace() {
        let mut edge = dependency("af-relay", "local-helper", false);
        edge.path_dependency = true;
        edge.path = Some("../local-helper".to_owned());

        let violations = validate_workspace(&baseline_workspace(), &[edge]);

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "path-dependency-outside-workspace");
    }

    #[test]
    fn rejects_external_package_impersonating_workspace_crate() {
        let violations = validate_workspace(
            &baseline_workspace(),
            &[dependency("af-relay", "af-protocol", false)],
        );

        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "workspace-crate-from-external-source");
    }

    #[test]
    fn rejects_missing_and_unknown_workspace_crates() {
        let mut workspace = baseline_workspace();
        workspace.remove("af-protocol");
        workspace.insert("af-unplanned".to_owned());

        let violations = validate_workspace(&workspace, &[]);

        assert_eq!(violations.len(), 2);
        assert_eq!(violations[0].rule, "required-crate-missing");
        assert_eq!(violations[1].rule, "workspace-crate-not-declared");
    }

    #[test]
    fn resolves_workspace_dependency_by_normalized_path() {
        let path = normalize_path("C:\\repo\\crates\\af-protocol\\");
        let workspace_paths = BTreeMap::from([(path.clone(), "af-protocol".to_owned())]);

        let (name, internal) =
            resolve_dependency_target("unexpected-name", Some(&path), &workspace_paths);

        assert_eq!(name, "af-protocol");
        assert!(internal);
    }

    #[test]
    fn preserves_external_package_name_when_path_is_not_in_workspace() {
        let workspace_paths = BTreeMap::new();

        let (name, internal) =
            resolve_dependency_target("serde", Some("../vendor/serde"), &workspace_paths);

        assert_eq!(name, "serde");
        assert!(!internal);
    }
}

//! Parse defaults without modifying this process's environment or logging secrets.
use std::{collections::HashSet, fs::File, io::ErrorKind, path::Path};

pub(super) fn defaults(
    working_directory: &Path,
    explicit: Option<&Path>,
) -> Result<Vec<(String, String)>, &'static str> {
    let default = working_directory.join(".env");
    let path = explicit.unwrap_or(&default);
    let file = match File::open(path) {
        Ok(file) if file.metadata().map_err(|_| "env_file_read")?.is_file() => file,
        Ok(_) => return Err("env_file_read"),
        Err(error) if explicit.is_none() && error.kind() == ErrorKind::NotFound => {
            return Ok(vec![]);
        }
        Err(_) => return Err("env_file_read"),
    };
    let mut entries = Vec::new();
    let mut seen = HashSet::new();
    for entry in dotenvy::from_read_iter(file) {
        // dotenvy's error may include the entire line, so only return a fixed code.
        let (key, value) = entry.map_err(|error| match error {
            dotenvy::Error::Io(_) => "env_file_read",
            _ => "env_file_parse",
        })?;
        if key.is_empty() || key.contains('\0') || key.contains('=') || value.contains('\0') {
            return Err("env_file_parse");
        }
        let normalized = if cfg!(windows) {
            key.to_ascii_uppercase()
        } else {
            key.clone()
        };
        if seen.insert(normalized) {
            entries.push((key, value));
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directory() -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("af-launcher-env-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn optional_default_and_explicit_missing_file_are_distinct() {
        let root = directory();
        assert!(defaults(&root, None).unwrap().is_empty());
        assert_eq!(
            defaults(&root, Some(&root.join("missing"))),
            Err("env_file_read")
        );
        std::fs::write(
            root.join(".env"),
            "AF_TEST='quoted value'\nAF_TEST=ignored\n",
        )
        .unwrap();
        assert_eq!(
            defaults(&root, None).unwrap(),
            vec![("AF_TEST".to_owned(), "quoted value".to_owned())]
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_or_unreadable_defaults_return_only_a_fixed_code() {
        let root = directory();
        std::fs::write(root.join(".env"), "AF_TEST='secret without closing quote\n").unwrap();
        assert_eq!(defaults(&root, None), Err("env_file_parse"));
        assert_eq!(defaults(&root, Some(&root)), Err("env_file_read"));
        std::fs::remove_dir_all(root).unwrap();
    }
}

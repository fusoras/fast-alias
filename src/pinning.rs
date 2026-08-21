//! Native version-range pinning for dependency manifests.
//!
//! Removes floating range prefixes so scaffolded projects stop silently
//! drifting on the next install: Node `package.json` (`^1.2.3`/`~1.2.3`
//! become exact `1.2.3`). Future ecosystems (Cargo.toml, requirements.txt)
//! plug into the same [`pin_project`] entry point.

use std::fs;
use std::path::Path;

/// Node dependency manifest handled by this module.
pub const NODE_MANIFEST: &str = "package.json";

/// Outcome of attempting to pin one manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinStatus {
    /// Manifest rewritten: N floating versions made exact.
    Pinned(usize),
    /// Manifest found but nothing needed changing.
    Unchanged,
    /// Manifest not present for this ecosystem.
    NotFound,
    /// The manifest exists but could not be processed safely.
    Error(String),
}

/// Per-manifest result reported by [`pin_project`].
#[derive(Debug, Clone)]
pub struct PinReport {
    pub manifest: &'static str,
    pub status: PinStatus,
}

/// Pins every recognized manifest found under `project_dir`.
pub fn pin_project(project_dir: &Path) -> Vec<PinReport> {
    vec![PinReport {
        manifest: NODE_MANIFEST,
        status: pin_node_package_json(&project_dir.join(NODE_MANIFEST)),
    }]
}

/// Pinnable section names inside `package.json`.
const PINNABLE: &[&[u8]] = &[b"dependencies", b"devDependencies", b"optionalDependencies"];

/// Rewrites `package.json` in place, removing `^`/`~` prefixes from versions
/// inside the pinnable dependency sections. Everything else is preserved
/// byte-for-byte (formatting, key order, scripts, non-floating specs).
pub fn pin_node_package_json(path: &Path) -> PinStatus {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return PinStatus::NotFound,
        Err(e) => return PinStatus::Error(e.to_string()),
    };

    let trimmed = src.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return PinStatus::Error("not a JSON object".into());
    }

    let bytes = src.as_bytes();
    let len = bytes.len();
    let mut out: Vec<u8> = Vec::with_capacity(len);
    let mut pinned = 0usize;
    let mut i = 0;

    let mut in_string = false;
    let mut escaped = false;
    let mut depth: i32 = 0;

    let mut string_start: usize = 0;
    let mut last_key_at_depth1: Option<usize> = None;

    let mut in_target = false;
    let mut target_depth: i32 = 0;
    let mut expect_value = false;
    let mut skip_next = false;

    while i < len {
        let b = bytes[i];

        if in_string {
            if escaped {
                escaped = false;
                out.push(b);
                i += 1;
                continue;
            }
            if b == b'\\' {
                escaped = true;
                out.push(b);
                i += 1;
                continue;
            }
            if skip_next {
                skip_next = false;
                i += 1;
                continue;
            }
            if b == b'"' {
                in_string = false;
                if depth == 1 {
                    let content = &bytes[string_start + 1..i];
                    last_key_at_depth1 = if PINNABLE.contains(&content) {
                        Some(i)
                    } else {
                        None
                    };
                }
            }
            out.push(b);
            i += 1;
            continue;
        }

        match b {
            b'"' => {
                in_string = true;
                string_start = i;
                if in_target && expect_value && i + 2 < len {
                    let first = bytes[i + 1];
                    let second = bytes[i + 2];
                    if (first == b'^' || first == b'~') && second.is_ascii_digit() {
                        skip_next = true;
                        expect_value = false;
                        pinned += 1;
                    }
                }
                out.push(b);
                i += 1;
            }
            b'{' => {
                if in_target && expect_value {
                    expect_value = false;
                }
                depth += 1;
                if !in_target && depth == 2 && last_key_at_depth1.is_some() {
                    in_target = true;
                    target_depth = depth;
                    last_key_at_depth1 = None;
                }
                out.push(b);
                i += 1;
            }
            b'[' => {
                if in_target && expect_value {
                    expect_value = false;
                }
                out.push(b);
                i += 1;
            }
            b'}' => {
                if in_target && depth == target_depth {
                    in_target = false;
                    expect_value = false;
                }
                depth -= 1;
                out.push(b);
                i += 1;
            }
            _ => {
                if in_target && depth == target_depth && (b == b':' || b == b',') {
                    if b == b':' && last_key_at_depth1.is_none() {
                        expect_value = true;
                    } else if b == b',' {
                        expect_value = false;
                    }
                }
                out.push(b);
                i += 1;
            }
        }
    }

    if pinned > 0 {
        fs::write(path, &out).unwrap_or_else(|e| {
            eprintln!("pinning: failed to write {path:?}: {e}");
        });
        PinStatus::Pinned(pinned)
    } else {
        PinStatus::Unchanged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fa-pinning-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_manifest(label: &str, content: &str) -> std::path::PathBuf {
        let dir = temp_dir(label);
        let path = dir.join(NODE_MANIFEST);
        fs::write(&path, content).unwrap();
        path
    }

    const MIXED: &str = r#"{
  "name": "nuevote",
  "type": "module",
  "meta": "{ \"dependencies\": { \"decoy\": \"^9.9.9\" } }",
  "scripts": {
    "dev": "astro dev",
    "build": "run ^tasks --tilde ~home"
  },
  "dependencies": {
    "astro": "^5.12.3",
    "@astrojs/check": "0.9.4",
    "sharp": "~0.34.2",
    "supersvg": "^workspace:*"
  },
  "devDependencies": {
    "prettier":"^3.6.2",
    "stylelint": "16.23.1",
    "oxlint": ">=1.0.0 <2.0.0",
    "lightningcss": "catalog:"
  },
  "optionalDependencies": {
    "fsevents": "~2.3.3"
  }
}"#;

    const MIXED_EXPECTED: &str = r#"{
  "name": "nuevote",
  "type": "module",
  "meta": "{ \"dependencies\": { \"decoy\": \"^9.9.9\" } }",
  "scripts": {
    "dev": "astro dev",
    "build": "run ^tasks --tilde ~home"
  },
  "dependencies": {
    "astro": "5.12.3",
    "@astrojs/check": "0.9.4",
    "sharp": "0.34.2",
    "supersvg": "^workspace:*"
  },
  "devDependencies": {
    "prettier":"3.6.2",
    "stylelint": "16.23.1",
    "oxlint": ">=1.0.0 <2.0.0",
    "lightningcss": "catalog:"
  },
  "optionalDependencies": {
    "fsevents": "2.3.3"
  }
}"#;

    #[test]
    fn node_manifest_should_strip_caret_and_tilde_only_in_dependency_sections() {
        println!("\n🔍 [TEST] Pinning — strips ^/~ only inside dependency sections");
        let path = write_manifest("strip", MIXED);

        let status = pin_node_package_json(&path);
        assert_eq!(status, PinStatus::Pinned(4), "Exactly 4 floats pinned: {status:?}");

        let rewritten = fs::read_to_string(&path).unwrap();
        assert_eq!(rewritten, MIXED_EXPECTED, "Rewrite must match expectation byte-for-byte");
        println!("   ✓ ^/~ stripped in all three sections; exact/range/workspace/catalog/decoys untouched.\n");
    }

    #[test]
    fn node_manifest_should_be_idempotent_after_pinning() {
        println!("\n🔍 [TEST] Pinning — idempotent on already-exact manifests");
        let path = write_manifest("idem", MIXED);
        assert!(matches!(pin_node_package_json(&path), PinStatus::Pinned(4)));
        assert_eq!(
            pin_node_package_json(&path),
            PinStatus::Unchanged,
            "Second run must change nothing"
        );
        println!("   ✓ Second pass reports Unchanged.\n");
    }

    #[test]
    fn missing_manifest_should_report_not_found() {
        println!("\n🔍 [TEST] Pinning — absent manifest reports NotFound");
        let dir = temp_dir("missing");
        assert_eq!(
            pin_node_package_json(&dir.join(NODE_MANIFEST)),
            PinStatus::NotFound
        );
        println!("   ✓ NotFound without touching the filesystem beyond reads.\n");
    }

    #[test]
    fn malformed_manifest_should_report_error_without_writing() {
        println!("\n🔍 [TEST] Pinning — garbage manifest reports Error, file untouched");
        let original = "this is not json at all";
        let path = write_manifest("garbage", original);
        let status = pin_node_package_json(&path);
        assert!(matches!(status, PinStatus::Error(_)), "Expected Error, got {status:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), original, "Garbage input must not be rewritten");
        println!("   ✓ Error surfaced; corrupt input preserved.\n");
    }

    #[test]
    fn pin_project_should_report_every_recognized_manifest() {
        println!("\n🔍 [TEST] Pinning — project entry reports one row per ecosystem");
        let dir = temp_dir("project");
        let reports = pin_project(&dir);
        assert_eq!(reports.len(), 1, "v1 covers Node only");
        assert_eq!(reports[0].manifest, NODE_MANIFEST);
        assert_eq!(reports[0].status, PinStatus::NotFound);
        println!("   ✓ Project scan returned the Node row as NotFound.\n");
    }
}

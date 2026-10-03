use crate::{DzipError, Result};
use std::path::{Component, Path, PathBuf};

/// Canonical key for the path rules used by the original Windows tool.
///
/// Stored spelling is intentionally kept elsewhere. This key exists only for
/// comparisons: both separators become `\\` and ASCII case is folded without
/// applying host-locale or Unicode rules.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArchivePathKey(Vec<u8>);

impl ArchivePathKey {
    pub fn from_path(path: &Path) -> Self {
        Self::from_archive_bytes(path.to_string_lossy().as_bytes())
    }

    pub fn from_archive_str(path: &str) -> Self {
        Self::from_archive_bytes(path.as_bytes())
    }

    pub fn from_archive_bytes(path: &[u8]) -> Self {
        Self(
            path.iter()
                .map(|byte| match byte {
                    b'/' => b'\\',
                    _ => byte.to_ascii_lowercase(),
                })
                .collect(),
        )
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Sanitize a path to ensure it is safe for extraction.
/// prevent Zip Slip attacks by disallowing absolute paths and `..` components.
pub fn sanitize_path(path: &Path) -> Result<PathBuf> {
    let mut clean_path = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => clean_path.push(name),
            Component::RootDir => {
                // Skip root directory component to make path relative
            }
            Component::Prefix(_) => {
                // Skip prefix (e.g., C:)
            }
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(DzipError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid path component '..' (Zip Slip prevention)",
                )));
            }
        }
    }
    if clean_path.as_os_str().is_empty() {
        return Err(DzipError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Empty path after sanitization",
        )));
    }
    Ok(clean_path)
}

/// Convert a path to the archive format (Windows-style backslashes).
pub fn to_archive_format(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}

/// Convert a path from the archive format (any separators) to the OS native format.
/// Also sanitizes the path.
pub fn from_archive_format(path_str: &str) -> Result<PathBuf> {
    // First normalize separators to generic forward slash for easier processing
    // or just rely on sanitize_path which iterates components.
    // However, Rust's Path::components behavior depends on the OS.
    // On POSIX, `foo\bar` is a single filename, on Windows it's two components.
    // Since DZIP archives use `\` as separator (Windows style), we should probably
    // treat `\` as a separator regardless of the host OS when reading from archive.

    // Replace backslashes with forward slashes for internal processing if on unix?
    // Actually, simply replacing `\` with `/` is a good first step for normalization
    // assuming the archive doesn't allow `\` in filenames (which it shouldn't if following spec).

    let normalized = path_str.replace('\\', "/");
    let path = Path::new(&normalized);
    sanitize_path(path)
}

/// Resolve a relative path from a string that might contain mixed separators (Internet/Windows style).
/// This splits the path by both `/` and `\` and reconstructs it using the system's native separator.
/// It also performs sanitization (Zip Slip prevention).
pub fn resolve_relative_path(path_str: &str) -> Result<PathBuf> {
    let mut clean_path = PathBuf::new();

    // Split by both / and \
    let parts = path_str.split(['/', '\\']);

    for part in parts {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(DzipError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid path component '..' (Zip Slip prevention)",
            )));
        }

        // Filter out Windows drive prefixes if any (e.g. "C:")
        // Simple heuristic: if it contains ':', skip it or error?
        // Standard DZIP shouldn't have drive letters.
        if part.contains(':') {
            // For safety, just treat as invalid or skip?
            // Let's treat as invalid for now to be safe.
            return Err(DzipError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid path component containing ':'",
            )));
        }

        clean_path.push(part);
    }

    if clean_path.as_os_str().is_empty() {
        // If empty, it means it's the root directory or just "."
        // Return mostly empty path (current dir)
        return Ok(PathBuf::from("."));
    }

    Ok(clean_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_archive_format() {
        let p = Path::new("folder/file.txt");
        assert_eq!(to_archive_format(p), "folder\\file.txt");
    }

    #[test]
    fn archive_path_keys_use_original_windows_comparison_rules() {
        assert_eq!(
            ArchivePathKey::from_archive_str("Data/Images/Logo.PNG"),
            ArchivePathKey::from_archive_str("data\\images\\logo.png")
        );
        assert_ne!(
            ArchivePathKey::from_archive_str("Straße"),
            ArchivePathKey::from_archive_str("STRASSE")
        );
    }

    #[test]
    fn test_sanitize_path_valid() {
        let p = Path::new("folder/file.txt");
        assert!(sanitize_path(p).is_ok());
    }

    #[test]
    fn test_sanitize_path_parent_dir() {
        let p = Path::new("../file.txt");
        assert!(sanitize_path(p).is_err());
        let p = Path::new("folder/../file.txt");
        assert!(sanitize_path(p).is_err());
    }

    #[test]
    fn test_sanitize_path_absolute() {
        let p = Path::new("/etc/passwd");
        // Should become relative "etc/passwd"
        let sanitized = sanitize_path(p).unwrap();
        assert_eq!(sanitized, Path::new("etc/passwd"));
    }

    #[test]
    fn test_resolve_relative_path_mixed() {
        let p = "folder\\subfolder/file.txt";
        let resolved = resolve_relative_path(p).unwrap();
        let expected: PathBuf = ["folder", "subfolder", "file.txt"].iter().collect();
        assert_eq!(resolved, expected);
    }

    #[test]
    fn test_resolve_relative_path_zip_slip() {
        let p = "folder\\../file.txt";
        assert!(resolve_relative_path(p).is_err());
    }
}

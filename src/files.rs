//! Access to the files that sources refer to, e.g. with `File("data.bin")`.
//!
//! The compiler doesn't touch the file system itself: it asks a
//! [`FileProvider`] for the contents of a file by the path given in the source.
//! This lets the files come from anywhere, e.g. from memory, or from a
//! directory with `Directory` (with the `fs` feature).

use anyhow::{Result, bail};
#[cfg(feature = "fs")]
use std::path::{Component, Path, PathBuf};

/// Provides the contents of the files that sources refer to.
///
/// For example, a provider of files from memory:
///
/// ```
/// use anyhow::{Context, Result};
/// use std::collections::HashMap;
/// use swfasm::files::FileProvider;
///
/// struct Memory(HashMap<String, Vec<u8>>);
///
/// impl FileProvider for Memory {
///     fn read(&self, path: &str) -> Result<Vec<u8>> {
///         self.0.get(path).cloned().context("no such file")
///     }
/// }
///
/// let files = Memory(HashMap::from([("data.bin".to_string(), vec![1, 2, 3])]));
/// let swf = swfasm::compile_str(
///     r#"Swf(version: 10, tags: [DefineBinaryData(id: 1, data: File("data.bin"))])"#,
///     &files,
/// )?;
/// assert!(swf.ends_with(&[1, 2, 3]));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub trait FileProvider {
    /// Returns the contents of the file with the given path, exactly as it is
    /// written in the source.
    ///
    /// It is up to the provider how the path is interpreted.
    fn read(&self, path: &str) -> Result<Vec<u8>>;
}

/// Provides files from a directory on the file system, typically the one
/// containing the source, with paths relative to it.
///
/// By default, paths that lead outside of the directory are rejected: absolute
/// paths, and paths that go above it with `..`. Symbolic links in the directory
/// are followed though, even if they point outside of it.
///
/// Requires the `fs` feature.
#[cfg(feature = "fs")]
#[derive(Debug, Clone)]
pub struct Directory {
    root: PathBuf,
    allow_outside: bool,
}

#[cfg(feature = "fs")]
impl Directory {
    /// Provides files from the given directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            allow_outside: false,
        }
    }

    /// Allows paths that lead outside of the directory, so that sources can read
    /// any file that the process can.
    pub fn allow_outside(mut self) -> Self {
        self.allow_outside = true;
        self
    }

    /// Returns the path of the file with the given path relative to the
    /// directory.
    fn resolve(&self, path: &str) -> Result<PathBuf> {
        let path = Path::new(path);
        if self.allow_outside {
            return Ok(self.root.join(path));
        }

        // `..` is applied here instead of by the file system, as it would apply
        // it to the targets of symbolic links, e.g. `link/..` would be the
        // parent of the target of `link`.
        let mut components = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(name) => components.push(name),
                Component::CurDir => {}
                Component::ParentDir => {
                    if components.pop().is_none() {
                        bail!("the path leads outside of the directory");
                    }
                }
                Component::RootDir | Component::Prefix(_) => {
                    bail!("the path leads outside of the directory");
                }
            }
        }
        let mut resolved = self.root.clone();
        resolved.extend(components);
        Ok(resolved)
    }
}

#[cfg(feature = "fs")]
impl FileProvider for Directory {
    fn read(&self, path: &str) -> Result<Vec<u8>> {
        Ok(std::fs::read(self.resolve(path)?)?)
    }
}

/// Provides no files, so sources that refer to files fail to compile.
#[derive(Debug, Clone, Copy)]
pub struct NoFiles;

impl FileProvider for NoFiles {
    fn read(&self, _path: &str) -> Result<Vec<u8>> {
        bail!("files are not available");
    }
}

#[cfg(all(test, feature = "fs"))]
mod tests {
    use super::*;

    fn resolve(directory: &Directory, path: &str) -> Result<PathBuf, String> {
        directory.resolve(path).map_err(|e| e.to_string())
    }

    #[test]
    fn directory_resolves_paths_inside() {
        let directory = Directory::new("root");
        let root = Path::new("root");
        assert_eq!(resolve(&directory, "a.bin"), Ok(root.join("a.bin")));
        assert_eq!(
            resolve(&directory, "a/b.bin"),
            Ok(root.join("a").join("b.bin"))
        );
        assert_eq!(
            resolve(&directory, "./a/./b.bin"),
            Ok(root.join("a").join("b.bin"))
        );
        assert_eq!(resolve(&directory, "a/../b.bin"), Ok(root.join("b.bin")));
        assert_eq!(
            resolve(&directory, "a/b/../../c.bin"),
            Ok(root.join("c.bin"))
        );
        assert_eq!(resolve(&directory, ""), Ok(root.to_path_buf()));
    }

    #[test]
    fn directory_rejects_paths_outside() {
        let directory = Directory::new("root");
        for path in ["..", "../a.bin", "a/../../b.bin", "/a.bin", "./../a.bin"] {
            assert_eq!(
                resolve(&directory, path),
                Err("the path leads outside of the directory".to_string()),
                "{path}"
            );
        }
    }

    #[test]
    fn directory_allows_paths_outside() {
        let directory = Directory::new("root").allow_outside();
        let root = Path::new("root");
        assert_eq!(resolve(&directory, "a.bin"), Ok(root.join("a.bin")));
        assert_eq!(resolve(&directory, "../a.bin"), Ok(root.join("../a.bin")));
        assert_eq!(resolve(&directory, "/a.bin"), Ok(PathBuf::from("/a.bin")));
    }
}

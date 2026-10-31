//! Access to the files that sources refer to, e.g. with `File("data.bin")`.
//!
//! The compiler doesn't touch the file system itself: it asks a
//! [`FileProvider`] for the contents of a file by the path given in the source.
//! This lets the files come from anywhere, e.g. from memory.

use anyhow::{Result, bail};

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

/// Provides no files, so sources that refer to files fail to compile.
#[derive(Debug, Clone, Copy)]
pub struct NoFiles;

impl FileProvider for NoFiles {
    fn read(&self, _path: &str) -> Result<Vec<u8>> {
        bail!("files are not available");
    }
}

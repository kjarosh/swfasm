//! Compiles ActionScript 3 into ABC files with asc.
//!
//! asc and the playerglobal that scripts are compiled against are not part of
//! swfasm, they are given to [`Compiler::with_asc`](crate::Compiler::with_asc)
//! in an [`AscConfig`]. The `asc-vendored` feature includes a copy of both, see
//! [`AscConfig::vendored`].
//!
//! Requires the `asc` feature.

#![cfg(feature = "asc")]

use crate::files::FileProvider;
use crate::source::{Asc, ScriptSource};
use anyhow::{Context, Result, bail};
use std::borrow::Cow;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// The path of an inline script, which appears in errors of asc.
const INLINE_PATH: &str = "inline";

/// The files that asc needs to compile scripts.
///
/// ```no_run
/// use swfasm::asc::{AscConfig, AscFile};
/// use swfasm::Compiler;
///
/// let config = AscConfig {
///     jar: AscFile::Path("asc.jar".into()),
///     playerglobal: AscFile::Path("playerglobal.abc".into()),
/// };
/// let swf = Compiler::new().with_asc(config).compile_str(
///     r#"Swf(version: 10, tags: [DoAbc(Asc("trace(1);"))])"#,
/// )?;
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct AscConfig {
    /// The jar of asc, run with Java.
    pub jar: AscFile,

    /// An ABC file with the definitions that scripts are compiled against,
    /// given to asc with `-import`.
    pub playerglobal: AscFile,
}

impl AscConfig {
    /// The copies of asc and Ruffle's playerglobal included in swfasm.
    ///
    /// Requires the `asc-vendored` feature.
    #[cfg(feature = "asc-vendored")]
    pub fn vendored() -> Self {
        Self {
            jar: AscFile::Bytes(Cow::Borrowed(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/vendor/asc/asc.jar"
            )))),
            playerglobal: AscFile::Bytes(Cow::Borrowed(include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/vendor/asc/playerglobal.abc"
            )))),
        }
    }
}

/// A file that asc needs, see [`AscConfig`].
#[derive(Debug, Clone, PartialEq)]
pub enum AscFile {
    /// The contents of the file, which are written into a temporary directory
    /// for asc.
    Bytes(Cow<'static, [u8]>),

    /// The path of the file on the file system.
    ///
    /// A relative path is relative to the working directory of the process.
    Path(PathBuf),
}

impl AscFile {
    /// Returns an absolute path of the file, writing it to `path` first if it
    /// is given as bytes.
    fn to_path(&self, path: PathBuf) -> Result<PathBuf> {
        match self {
            AscFile::Bytes(bytes) => {
                std::fs::write(&path, bytes)?;
                Ok(path)
            }
            AscFile::Path(path) => Ok(std::path::absolute(path)?),
        }
    }
}

/// Compiles a script, with the other scripts it lists, into an ABC file.
///
/// The files are read from `files` and copied into a temporary directory, where
/// asc runs.
pub(crate) fn compile(
    script: &Asc,
    config: &AscConfig,
    files: &dyn FileProvider,
) -> Result<Vec<u8>> {
    let dir = tempfile::tempdir().context("failed to create a temporary directory")?;
    let asc_jar = config
        .jar
        .to_path(dir.path().join("asc.jar"))
        .context("failed to prepare the asc jar")?;
    let playerglobal = config
        .playerglobal
        .to_path(dir.path().join("playerglobal.abc"))
        .context("failed to prepare the playerglobal")?;

    // The scripts are in their own directory, so that their paths don't
    // collide with the files above.
    let src = dir.path().join("src");
    // Returns the path of the file relative to `src`.
    let write = |path: &str, contents: &[u8]| -> Result<PathBuf> {
        let relative = relative_path(path)?;
        let dest = src.join(&relative);
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::write(dest, contents)?;
        Ok(relative)
    };

    let main = match &script.source {
        ScriptSource::Inline(source) => write(INLINE_PATH, source.as_bytes())?,
        ScriptSource::File(path) => {
            let contents = files
                .read(path)
                .with_context(|| format!("failed to read file \"{path}\""))?;
            write(path, &contents)?
        }
    };

    let mut command = Command::new("java");
    command
        .current_dir(&src)
        .arg("-classpath")
        .arg(&asc_jar)
        .arg("macromedia.asc.embedding.Main")
        .args(["-AS3", "-md", "-import"])
        .arg(&playerglobal);
    if script.optimize {
        command.arg("-optimize");
    }
    if script.debug {
        command.arg("-d");
    }
    for path in &script.scripts {
        // Paths are compared as they are written to `src`, so that e.g. `a.as`
        // and `./a.as` are the same file.
        if relative_path(path)? == main {
            if let ScriptSource::Inline(_) = script.source {
                bail!("\"{path}\" is the path of the inline script");
            }
            // The main script is compiled once, even if it is listed too.
            continue;
        }
        let contents = files
            .read(path)
            .with_context(|| format!("failed to read file \"{path}\""))?;
        let relative = write(path, &contents)?;
        command.arg("-in").arg(relative);
    }
    command.arg(&main);

    let output = match command.output() {
        Ok(output) => output,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            bail!("failed to run Java, which asc needs, it might not be installed")
        }
        Err(e) => return Err(e).context("failed to run asc"),
    };

    // asc exits successfully even when the scripts have errors, but then it
    // doesn't write the ABC file. It is written next to the main script.
    let mut abc_path = src.join(&main);
    if abc_path.extension().is_some_and(|ext| ext == "as") {
        abc_path.set_extension("abc");
    } else {
        abc_path.as_mut_os_string().push(".abc");
    }
    match std::fs::read(&abc_path) {
        Ok(abc) => Ok(abc),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let mut message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            if message.is_empty() {
                message = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            }
            bail!("asc failed: {message}")
        }
        Err(e) => Err(e).context("failed to read the ABC file written by asc"),
    }
}

/// Returns a path that stays inside the directory it is joined to, so that
/// files can be written to it.
fn relative_path(path: &str) -> Result<PathBuf> {
    let mut relative = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(name) => relative.push(name),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("the path \"{path}\" must be relative and must not contain `..`")
            }
        }
    }
    if relative.as_os_str().is_empty() {
        bail!("the path \"{path}\" does not name a file");
    }
    Ok(relative)
}

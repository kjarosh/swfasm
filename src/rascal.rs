//! Compiles ActionScript 2 into AVM1 bytecode with Rascal.
//!
//! Requires the `rascal` feature.

#![cfg(feature = "rascal")]

use crate::files::FileProvider;
use crate::source::{Rascal, RascalSource};
use ::rascal::{CompileOptions, ProgramBuilder, SourceProvider, SwfOptions};
use anyhow::{Result, anyhow, bail};
use std::io;

/// The path of an inline script in Rascal, which appears in its errors.
const INLINE_PATH: &str = "<inline>";

/// Compiles a script into AVM1 bytecode for the given SWF version.
///
/// Files that the script includes, and the script itself if it is a file, are
/// read from `files`.
pub(crate) fn compile(
    script: &Rascal,
    swf_version: u8,
    files: &dyn FileProvider,
) -> Result<Vec<u8>> {
    let (path, inline) = match &script.source {
        RascalSource::Inline(source) => (INLINE_PATH, Some(source.as_str())),
        RascalSource::File(path) => (path.as_str(), None),
    };
    let mut builder = ProgramBuilder::new(Sources { inline, files })
        .with_compile_options(CompileOptions::default().with_swf_version(swf_version));
    builder.add_script(path);
    let program = builder
        .build()
        .map_err(|e| anyhow!("{}", e.to_string_plain()))?;

    // Rascal only produces whole SWFs, so the bytecode is read back from one.
    // TODO Maybe add support to Rascal?
    let swf = program.compile().to_swf(&SwfOptions::default())?;
    let swf = swf::decompress_swf(&swf[..])?;
    let swf = swf::parse_swf(&swf)?;
    let mut bytecode = None;
    for tag in &swf.tags {
        match tag {
            swf::Tag::FileAttributes(_)
            | swf::Tag::SetBackgroundColor(_)
            | swf::Tag::ShowFrame
            | swf::Tag::End => {}
            swf::Tag::DoAction(actions) => {
                if bytecode.replace(*actions).is_some() {
                    bail!("Rascal produced more than one DoAction tag");
                }
            }
            // Classes and interfaces are compiled into sprites with their own
            // actions, which a single piece of data can't hold.
            swf::Tag::DefineSprite(_)
            | swf::Tag::ExportAssets(_)
            | swf::Tag::DoInitAction { .. } => {
                bail!("classes and interfaces are not supported");
            }
            _ => bail!("Rascal produced an unexpected tag"),
        }
    }
    // There is no DoAction tag when a script has no code.
    Ok(bytecode.unwrap_or_default().to_vec())
}

/// Provides the inline script to Rascal, if there is one, and other files from
/// `files`.
struct Sources<'a> {
    inline: Option<&'a str>,
    files: &'a dyn FileProvider,
}

impl SourceProvider for Sources<'_> {
    fn load(&self, path: &str) -> io::Result<String> {
        if let Some(inline) = self.inline
            && path == INLINE_PATH
        {
            return Ok(inline.to_owned());
        }
        let bytes = self
            .files
            .read(path)
            .map_err(|e| io::Error::other(format!("{e:#}")))?;
        String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn is_file(&self, path: &str) -> bool {
        // Rascal asks this to find out whether a name refers to a class.
        // `FileProvider` can only read files, so this reads the whole file.
        (self.inline.is_some() && path == INLINE_PATH) || self.files.read(path).is_ok()
    }
}

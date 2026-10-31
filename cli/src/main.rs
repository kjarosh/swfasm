mod cli;

use anyhow::{Context, Result, bail};
use clap::Parser;
use cli::Opt;

fn main() -> Result<()> {
    let opt = Opt::parse();

    let output_path = opt
        .output
        .unwrap_or_else(|| opt.input.with_extension("swf"));
    // Compare the files rather than the paths, so that other spellings of the
    // input path, symlinks and hard links are caught too. It fails when either
    // file doesn't exist, in which case they can't be the same file.
    if same_file::is_same_file(&opt.input, &output_path).unwrap_or(false) {
        bail!(
            "Refusing to overwrite the input file {}, use --output",
            opt.input.display()
        );
    }

    let text = std::fs::read_to_string(&opt.input)
        .with_context(|| format!("Failed to read {}", opt.input.display()))?;
    let swf = swfasm::compile_str(&text)
        .with_context(|| format!("Failed to compile {}", opt.input.display()))?;

    std::fs::write(&output_path, swf)
        .with_context(|| format!("Failed to write {}", output_path.display()))?;
    Ok(())
}

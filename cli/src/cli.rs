use clap::Parser;
use std::path::PathBuf;

/// Compiles a textual (RON) representation of a SWF file into a SWF file.
#[derive(Parser, Debug)]
#[clap(name = "swfasm", author, version)]
pub struct Opt {
    /// The source file.
    pub input: PathBuf,

    /// The output file.
    ///
    /// Defaults to the input file with the `.swf` extension.
    #[clap(short, long)]
    pub output: Option<PathBuf>,

    /// The jar of asc, used to compile ActionScript 3.
    ///
    /// Defaults to the one included in swfasm.
    #[clap(long, value_name = "PATH")]
    pub asc_jar: Option<PathBuf>,

    /// The playerglobal that asc compiles ActionScript 3 against, an ABC file.
    ///
    /// Defaults to Ruffle's playerglobal, included in swfasm.
    #[clap(long, value_name = "PATH")]
    pub playerglobal: Option<PathBuf>,
}

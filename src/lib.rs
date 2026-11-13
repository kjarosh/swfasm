//! A textual (RON) representation of SWF files, with a compiler that turns it
//! into SWF files.

mod compile;
pub mod files;
mod rascal;
pub mod source;

pub use compile::Compiler;

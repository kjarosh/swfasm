# swfasm

A textual representation of SWF files, with a compiler that turns it into SWF
files.

```sh
swfasm test.swfasm         # writes test.swf
swfasm test.swfasm -o out.swf
```

The command line tool is in the `swfasm-cli` crate (in [`cli/`](cli)), and the
compiler is in the `swfasm` library crate.

```sh
cargo install swfasm-cli
```

## Format

Sources use the `.swfasm` extension and are written in
[RON](https://github.com/ron-rs/ron), with the `implicit_some` extension enabled
(optional values don't need `Some(...)`).
See [`examples/`](examples) and [`src/source.rs`](src/source.rs) for everything
that is supported.

### Tags

Tags are named after SWF tags, see `Tag` in [`src/source.rs`](src/source.rs)
for the ones that are supported. In addition, there are special tags that give
control over the raw encoding:

* `Raw(code: 26, data: InlineBytes("01 02 ff"))` emits a tag with an arbitrary
  code and a body given as [data](#data). Set `force_long: true` to use the
  long (6 byte) header form even for a short body.

### AVM1 actions

`DoAction(InlineBytes("96 02 00 05 01 26"))` takes AVM1 bytecode as
[data](#data).

### Data

Tags that hold data, like `Raw`, `DoAction` and `DefineBinaryData`, take it
either inline or from a file:

```ron
DefineBinaryData(id: 1, data: InlineBytes("de ad be ef")),
DefineBinaryData(id: 2, data: File("data.bin")),
```

Inline bytes are written as a hex string with optional whitespace
(`"01 02 ff"`), or a list of hex strings, which are concatenated. The list form
allows commenting individual parts:

```ron
Raw(code: 1, data: InlineBytes([
    "0a 00", // frame count
    "ff ff", // flags
])),
```

Paths of files are relative to the directory of the `.swfasm` file.

Data can also be compiled from ActionScript:

```ron
// AVM1 bytecode, compiled by Rascal from ActionScript 2.
DoAction(Rascal(r#"trace("hello");"#)),
// An ABC file, compiled by asc from ActionScript 3. It runs Java, which has to
// be installed.
DoAbc(Asc(file: "Main.as", scripts: ["pkg/Other.as"])),
```

See [`examples/rascal.swfasm`](examples/rascal.swfasm) and
[`examples/asc.swfasm`](examples/asc.swfasm) for their options. They are behind
the `rascal` and `asc` features of the library, enabled by default. asc and the
playerglobal that scripts are compiled against are given to the compiler, the
`asc-vendored` feature includes copies of them (see [`vendor/asc`](vendor/asc)),
which the command line tool uses unless `--asc-jar` or `--playerglobal` is
given.

## TODO

* Symbolic character IDs (e.g. `@name`) instead of numbers.
* Remaining tags (a lot of them).
* Loading other data from files (e.g. images and sounds).
* Better error messages.

## License

Licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

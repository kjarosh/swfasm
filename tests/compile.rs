use swf::{Compression, Fixed8, Rectangle, Twips};
use swfasm::files::{FileProvider, NoFiles};
use swfasm::source::{Data, Rascal, RascalSource, Tag};

// These are macros instead of functions, so that failures point to the test.

/// Compiles the given tags and returns the serialized tag stream.
///
/// Files are read from the given provider, or are not available without one.
macro_rules! compile_tags {
    ($tags:expr) => {
        compile_tags!($tags, &NoFiles)
    };
    ($tags:expr, $files:expr) => {{
        let compile = |tags: &str| {
            swfasm::compile_str(
                &format!("Swf(version: 10, num_frames: 0, tags: [{tags}])"),
                $files,
            )
        };
        let header_len = compile("").unwrap().len();
        compile(&$tags).unwrap()[header_len..].to_vec()
    }};
}

/// Compiles the given source, which must fail, and returns the error message.
///
/// Files are read from the given provider, or are not available without one.
macro_rules! compile_err {
    ($source:expr) => {
        compile_err!($source, &NoFiles)
    };
    ($source:expr, $files:expr) => {
        format!("{:#}", swfasm::compile_str(&$source, $files).unwrap_err())
    };
}

/// Provides files whose contents are their paths, so that tests can check which
/// path is read.
struct PathAsContents;

impl FileProvider for PathAsContents {
    fn read(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        Ok(path.as_bytes().to_vec())
    }
}

/// Provides no files, with an error that names the path.
struct Failing;

impl FileProvider for Failing {
    fn read(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        anyhow::bail!("no file named {path}")
    }
}

#[test]
fn header() {
    let swf = swfasm::compile_str(
        "Swf(
            version: 8,
            stage: (x: 10, y: 1.5, width: 100, height: 0.05),
            frame_rate: 12.5,
            tags: [ShowFrame, ShowFrame, ShowFrame],
        )",
        &NoFiles,
    )
    .unwrap();
    let swf_buf = swf::decompress_swf(&swf[..]).unwrap();
    let header = &swf_buf.header;
    assert_eq!(header.compression(), Compression::None);
    assert_eq!(header.version(), 8);
    assert_eq!(
        header.stage_size(),
        &Rectangle {
            x_min: Twips::new(200),
            x_max: Twips::new(2200),
            y_min: Twips::new(30),
            y_max: Twips::new(31),
        }
    );
    assert_eq!(header.frame_rate(), Fixed8::from_f64(12.5));
    assert_eq!(header.num_frames(), 3);
}

#[test]
fn stage_edges_are_added_in_twips() {
    let swf = swfasm::compile_str(
        "Swf(version: 10, stage: (x: 0.05, y: 0.05, width: 0.1, height: 0.1))",
        &NoFiles,
    )
    .unwrap();
    let swf_buf = swf::decompress_swf(&swf[..]).unwrap();
    assert_eq!(
        swf_buf.header.stage_size(),
        &Rectangle {
            x_min: Twips::new(1),
            x_max: Twips::new(3),
            y_min: Twips::new(1),
            y_max: Twips::new(3),
        }
    );
}

#[test]
fn default_num_frames() {
    let num_frames = |tags: &str| {
        let swf =
            swfasm::compile_str(&format!("Swf(version: 10, tags: [{tags}])"), &NoFiles).unwrap();
        swf::decompress_swf(&swf[..]).unwrap().header.num_frames()
    };
    assert_eq!(num_frames(""), 0);
    assert_eq!(num_frames("ShowFrame, ShowFrame"), 2);
    assert_eq!(num_frames("FileAttributes(), ShowFrame, End"), 1);
    assert_eq!(num_frames(r#"Raw(code: 1, data: InlineBytes(""))"#), 1);
    assert_eq!(
        num_frames(r#"Raw(code: 1, force_long: true, data: InlineBytes(""))"#),
        1
    );
    assert_eq!(num_frames(r#"Raw(code: 2, data: InlineBytes(""))"#), 0);
    assert_eq!(
        num_frames(r#"ShowFrame, Raw(code: 1, data: InlineBytes(""))"#),
        2
    );
    assert_eq!(num_frames(&"ShowFrame,".repeat(65535)), 65535);
    assert!(
        compile_err!(format!(
            "Swf(version: 10, tags: [{}])",
            "ShowFrame,".repeat(65536)
        ))
        .contains("too many frames")
    );
}

#[test]
fn explicit_num_frames() {
    let num_frames = |num_frames: u16, tags: &str| {
        let swf = swfasm::compile_str(
            &format!("Swf(version: 10, num_frames: {num_frames}, tags: [{tags}])"),
            &NoFiles,
        )
        .unwrap();
        swf::decompress_swf(&swf[..]).unwrap().header.num_frames()
    };
    assert_eq!(num_frames(0, "ShowFrame"), 0);
    assert_eq!(num_frames(5, "ShowFrame"), 5);
    assert_eq!(num_frames(65535, ""), 65535);
}

#[test]
fn file_attributes_flags() {
    // The tag header is followed by a 32-bit flags field, with the bits from
    // the SWF specification.
    assert_eq!(
        compile_tags!("FileAttributes()"),
        [0x44, 0x11, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        compile_tags!("FileAttributes(use_network: true)"),
        [0x44, 0x11, 0x01, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        compile_tags!("FileAttributes(action_script_3: true)"),
        [0x44, 0x11, 0x08, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        compile_tags!("FileAttributes(has_metadata: true)"),
        [0x44, 0x11, 0x10, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        compile_tags!("FileAttributes(use_gpu: true)"),
        [0x44, 0x11, 0x20, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        compile_tags!("FileAttributes(use_direct_blit: true)"),
        [0x44, 0x11, 0x40, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        compile_tags!(
            "FileAttributes(
                action_script_3: true,
                use_network: true,
                has_metadata: true,
                use_gpu: true,
                use_direct_blit: true,
            )"
        ),
        [0x44, 0x11, 0x79, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        compile_tags!("FileAttributes(use_network: false, use_gpu: false)"),
        [0x44, 0x11, 0x00, 0x00, 0x00, 0x00]
    );
}

#[test]
fn raw_tag() {
    assert_eq!(
        compile_tags!(
            r#"Raw(code: 1023, data: InlineBytes("01 02 aB")), Raw(code: 2, data: InlineBytes("0304"))"#
        ),
        [0xC3, 0xFF, 0x01, 0x02, 0xAB, 0x82, 0x00, 0x03, 0x04]
    );
    assert_eq!(
        compile_tags!(
            r#"Raw(code: 2, data: InlineBytes([
                "05 06", // comment
                "",
                " 07 ",
            ]))"#
        ),
        [0x83, 0x00, 0x05, 0x06, 0x07]
    );
    assert_eq!(
        compile_tags!(r#"Raw(code: 2, data: InlineBytes([]))"#),
        [0x80, 0x00]
    );
    assert_eq!(
        compile_tags!(r#"Raw(code: 2, force_long: true, data: InlineBytes("aa"))"#),
        [0xBF, 0x00, 0x01, 0x00, 0x00, 0x00, 0xAA]
    );
    // Bodies that do not fit in the short form use the long one anyway.
    assert_eq!(
        compile_tags!(&format!(
            r#"Raw(code: 2, data: InlineBytes("{}"))"#,
            "00".repeat(63)
        )),
        [[0xBF, 0x00, 63, 0x00, 0x00, 0x00].as_slice(), &[0; 63]].concat()
    );
}

#[test]
fn raw_tag_from_file() {
    assert_eq!(
        compile_tags!(r#"Raw(code: 2, data: File("ab"))"#, &PathAsContents),
        [0x82, 0x00, b'a', b'b']
    );
    assert_eq!(
        compile_tags!(
            r#"Raw(code: 2, force_long: true, data: File("a"))"#,
            &PathAsContents
        ),
        [0xBF, 0x00, 0x01, 0x00, 0x00, 0x00, b'a']
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 2, data: File("a.bin"))])"#)
            .contains(r#"tag #0: failed to read file "a.bin": files are not available"#)
    );
}

#[test]
fn tag_code_errors() {
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1100, data: InlineBytes(""))])"#)
            .contains("tag code 1100 does not fit in 10 bits")
    );
}

#[test]
fn unknown_fields() {
    assert!(
        compile_err!("Swf(version: 10, tags: [FileAttributes(as_3: true)])")
            .contains("Unexpected field named `as_3` in `FileAttributes`")
    );
    assert!(
        compile_err!(
            r#"Swf(version: 10, tags: [Raw(code: 1, data: InlineBytes(""), long: true)])"#
        )
        .contains("Unexpected field named `long` in `Raw`")
    );
}

#[test]
fn do_action_bytes() {
    assert_eq!(
        compile_tags!(r#"DoAction(InlineBytes("07 00"))"#),
        [0x02, 0x03, 0x07, 0x00]
    );
    assert_eq!(compile_tags!(r#"DoAction(InlineBytes(""))"#), [0x00, 0x03]);
    // Bytes are written as is: no End action is appended, and the bytecode is
    // not validated.
    assert_eq!(
        compile_tags!(r#"DoAction(InlineBytes("07"))"#),
        [0x01, 0x03, 0x07]
    );
    assert_eq!(
        compile_tags!(r#"DoAction(InlineBytes("ff ff"))"#),
        [0x02, 0x03, 0xFF, 0xFF]
    );
    assert_eq!(
        compile_tags!(r#"DoAction(InlineBytes(["07", "", "00"]))"#),
        [0x02, 0x03, 0x07, 0x00]
    );
    // The longest body that fits in the short header form.
    assert_eq!(
        compile_tags!(&format!(r#"DoAction(InlineBytes("{}"))"#, "00".repeat(62))),
        [[0x3E, 0x03].as_slice(), &[0; 62]].concat()
    );
    assert_eq!(
        compile_tags!(&format!(r#"DoAction(InlineBytes("{}"))"#, "00".repeat(63))),
        [[0x3F, 0x03, 63, 0x00, 0x00, 0x00].as_slice(), &[0; 63]].concat()
    );
    assert_eq!(
        compile_tags!(&format!(
            r#"DoAction(InlineBytes("{}"))"#,
            "00".repeat(0x1234)
        )),
        [
            [0x3F, 0x03, 0x34, 0x12, 0x00, 0x00].as_slice(),
            &[0; 0x1234]
        ]
        .concat()
    );
}

#[test]
fn do_action_errors() {
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [DoAction("07 00")])"#)
            .contains("Expected identifier")
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [DoAction(Text("07 00"))])"#)
            .contains("Unexpected variant named `Text` in enum `Data`")
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [DoAction(InlineBytes("070"))])"#)
            .contains(r#"hex string "070" has an odd number of digits"#)
    );
}

#[test]
fn do_action_from_file() {
    assert_eq!(
        compile_tags!(r#"DoAction(File("ab"))"#, &PathAsContents),
        [0x02, 0x03, b'a', b'b']
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [DoAction(File("a.bin"))])"#)
            .contains(r#"tag #0: failed to read file "a.bin": files are not available"#)
    );
}

#[test]
fn invalid_values() {
    assert!(
        compile_err!("Swf(version: 10, stage: (width: 0.01, height: 1))")
            .contains("stage width: 0.01px is not a whole number of twips")
    );
    assert!(
        compile_err!("Swf(version: 10, frame_rate: 0.001)")
            .contains("frame rate: 0.001 is not representable as an 8.8 fixed point number")
    );
    assert!(
        compile_err!("Swf(version: 10, tags: [SetBackgroundColor(0x1000000)])")
            .contains("background color 0x1000000 is not in the 0xRRGGBB format")
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1, data: InlineBytes("0"))])"#)
            .contains(r#"hex string "0" has an odd number of digits"#)
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1, data: InlineBytes(["00", "0"]))])"#)
            .contains(r#"hex string "0" has an odd number of digits"#)
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1, data: InlineBytes("0g"))])"#)
            .contains("invalid hex digit 'g'")
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1, data: InlineBytes([1, 2]))])"#)
            .contains("Expected string")
    );
}

#[test]
fn define_sprite() {
    // The body is the character ID, the frame count and the nested tags.
    assert_eq!(
        compile_tags!("DefineSprite(id: 0x1234, tags: [ShowFrame, End])"),
        [0xC8, 0x09, 0x34, 0x12, 0x01, 0x00, 0x40, 0x00, 0x00, 0x00]
    );
    // No End tag is appended.
    assert_eq!(
        compile_tags!("DefineSprite(id: 1, tags: [])"),
        [0xC4, 0x09, 0x01, 0x00, 0x00, 0x00]
    );
    // Tags after an End tag are written too.
    assert_eq!(
        compile_tags!("DefineSprite(id: 1, num_frames: 0, tags: [End, ShowFrame, End])"),
        [
            0xCA, 0x09, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00
        ]
    );
    assert_eq!(
        compile_tags!("DefineSprite(id: 1, num_frames: 0x0203, tags: [ShowFrame])"),
        [0xC6, 0x09, 0x01, 0x00, 0x03, 0x02, 0x40, 0x00]
    );
    // Nested tags keep their header form.
    assert_eq!(
        compile_tags!(
            r#"DefineSprite(id: 1, tags: [Raw(code: 1, force_long: true, data: InlineBytes(""))])"#
        ),
        [
            0xCA, 0x09, 0x01, 0x00, 0x01, 0x00, 0x7F, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
    // The longest body that fits in the short header form, and the shortest
    // one that does not.
    assert_eq!(
        compile_tags!(&format!(
            "DefineSprite(id: 1, num_frames: 0, tags: [{}])",
            "End,".repeat(29)
        )),
        [[0xFE, 0x09, 0x01, 0x00, 0x00, 0x00].as_slice(), &[0; 58]].concat()
    );
    assert_eq!(
        compile_tags!(&format!(
            "DefineSprite(id: 1, num_frames: 0, tags: [{}])",
            "End,".repeat(30)
        )),
        [
            [0xFF, 0x09, 64, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00].as_slice(),
            &[0; 60]
        ]
        .concat()
    );
}

#[test]
fn define_sprite_num_frames() {
    let num_frames = |tags: &str| {
        let swf =
            swfasm::compile_str(&format!("Swf(version: 10, tags: [{tags}])"), &NoFiles).unwrap();
        swf::decompress_swf(&swf[..]).unwrap().header.num_frames()
    };
    // Frames of sprites do not count towards the main timeline.
    assert_eq!(num_frames("DefineSprite(id: 1, tags: [ShowFrame])"), 0);
    assert_eq!(
        num_frames("DefineSprite(id: 1, tags: [ShowFrame]), ShowFrame"),
        1
    );
    assert!(
        compile_err!(format!(
            "Swf(version: 10, tags: [DefineSprite(id: 1, tags: [{}])])",
            "ShowFrame,".repeat(65536)
        ))
        .contains("tag #0: too many frames")
    );
}

#[test]
fn define_sprite_errors() {
    // Errors in nested tags point to both the sprite and the nested tag.
    assert!(
        compile_err!(
            "Swf(version: 10, tags: [
                ShowFrame,
                DefineSprite(id: 1, tags: [ShowFrame, SetBackgroundColor(0x1000000)]),
            ])"
        )
        .contains("tag #1: tag #1: background color 0x1000000 is not in the 0xRRGGBB format")
    );
    assert!(
        compile_err!("Swf(version: 10, tags: [DefineSprite(tags: [])])")
            .contains("Unexpected missing field named `id` in `DefineSprite`")
    );
    assert!(
        compile_err!("Swf(version: 10, tags: [DefineSprite(id: 1)])")
            .contains("Unexpected missing field named `tags` in `DefineSprite`")
    );
    assert!(
        compile_err!("Swf(version: 10, tags: [DefineSprite(id: 1, frames: 1)])")
            .contains("Unexpected field named `frames` in `DefineSprite`")
    );
}

#[test]
fn remove_object() {
    // The body is the character ID and the depth.
    assert_eq!(
        compile_tags!("RemoveObject(character: 0x1234, depth: 0x5678)"),
        [0x44, 0x01, 0x34, 0x12, 0x78, 0x56]
    );
    // The body is just the depth.
    assert_eq!(
        compile_tags!("RemoveObject2(depth: 0x1234)"),
        [0x02, 0x07, 0x34, 0x12]
    );
}

#[test]
fn remove_object_errors() {
    assert!(
        compile_err!("Swf(version: 10, tags: [RemoveObject(depth: 1)])")
            .contains("Unexpected missing field named `character` in `RemoveObject`")
    );
    assert!(
        compile_err!("Swf(version: 10, tags: [RemoveObject2()])")
            .contains("Unexpected missing field named `depth` in `RemoveObject2`")
    );
    assert!(
        compile_err!("Swf(version: 10, tags: [RemoveObject2(character: 1, depth: 1)])")
            .contains("Unexpected field named `character` in `RemoveObject2`")
    );
}

#[test]
fn define_binary_data() {
    // The body is the character ID, a reserved 32-bit field and the data.
    assert_eq!(
        compile_tags!(r#"DefineBinaryData(id: 0x1234, data: InlineBytes("01 02 ff"))"#),
        [
            0xC9, 0x15, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0xFF
        ]
    );
    assert_eq!(
        compile_tags!(r#"DefineBinaryData(id: 1, data: InlineBytes(["01", "", "02"]))"#),
        [0xC8, 0x15, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02]
    );
    assert_eq!(
        compile_tags!(r#"DefineBinaryData(id: 1, data: InlineBytes(""))"#),
        [0xC6, 0x15, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    // The longest data that fits in the short header form, and the shortest
    // that does not.
    assert_eq!(
        compile_tags!(&format!(
            r#"DefineBinaryData(id: 1, data: InlineBytes("{}"))"#,
            "00".repeat(56)
        )),
        [[0xFE, 0x15, 0x01, 0x00].as_slice(), &[0; 60]].concat()
    );
    assert_eq!(
        compile_tags!(&format!(
            r#"DefineBinaryData(id: 1, data: InlineBytes("{}"))"#,
            "00".repeat(57)
        )),
        [
            [0xFF, 0x15, 63, 0x00, 0x00, 0x00, 0x01, 0x00].as_slice(),
            &[0; 61]
        ]
        .concat()
    );
}

#[test]
fn define_binary_data_from_file() {
    assert_eq!(
        compile_tags!(
            r#"DefineBinaryData(id: 1, data: File("ab"))"#,
            &PathAsContents
        ),
        [0xC8, 0x15, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, b'a', b'b']
    );
    // The path is passed to the provider exactly as it is written, with
    // escapes resolved.
    assert_eq!(
        compile_tags!(
            r#"DefineBinaryData(id: 1, data: File("./a/../\u{62}"))"#,
            &PathAsContents
        ),
        [
            [0xCE, 0x15, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00].as_slice(),
            b"./a/../b"
        ]
        .concat()
    );
    assert_eq!(
        compile_tags!(
            r#"DefineBinaryData(id: 1, data: File(""))"#,
            &PathAsContents
        ),
        [0xC6, 0x15, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    // Files are read in nested tags too.
    assert_eq!(
        compile_tags!(
            r#"DefineSprite(id: 1, num_frames: 0, tags: [
                DefineBinaryData(id: 2, data: File("c")),
            ])"#,
            &PathAsContents
        ),
        [
            0xCD, 0x09, 0x01, 0x00, 0x00, 0x00, 0xC7, 0x15, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
            b'c'
        ]
    );
}

#[test]
fn define_binary_data_errors() {
    assert!(
        compile_err!(
            r#"Swf(version: 10, tags: [ShowFrame, DefineBinaryData(id: 1, data: File("a.bin"))])"#
        )
        .contains(r#"tag #1: failed to read file "a.bin": files are not available"#)
    );
    // Errors of the provider are kept, and point to the nested tag.
    assert!(
        compile_err!(
            r#"Swf(version: 10, tags: [
                DefineSprite(id: 1, tags: [DefineBinaryData(id: 2, data: File("a.bin"))]),
            ])"#,
            &Failing
        )
        .contains(r#"tag #0: tag #0: failed to read file "a.bin": no file named a.bin"#)
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [DefineBinaryData(id: 1, data: "01")])"#)
            .contains("Expected identifier")
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [DefineBinaryData(id: 1, data: Hex("01"))])"#)
            .contains("Unexpected variant named `Hex` in enum `Data`")
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [DefineBinaryData(id: 1, data: File(1))])"#)
            .contains("Expected string")
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [DefineBinaryData(data: InlineBytes(""))])"#)
            .contains("Unexpected missing field named `id` in `DefineBinaryData`")
    );
    assert!(
        compile_err!("Swf(version: 10, tags: [DefineBinaryData(id: 1)])")
            .contains("Unexpected missing field named `data` in `DefineBinaryData`")
    );
}

#[test]
fn rascal_syntax() {
    let parse = |data: &str| -> Result<Data, String> {
        let swf = swfasm::parse(&format!("Swf(version: 10, tags: [DoAction({data})])"))
            .map_err(|e| format!("{e:#}"))?;
        match swf.tags.into_iter().next() {
            Some(Tag::DoAction(data)) => Ok(data),
            tag => panic!("unexpected tag {tag:?}"),
        }
    };
    let rascal = |source| Ok(Data::Rascal(Rascal { source }));
    let inline = |text: &str| rascal(RascalSource::Inline(text.to_owned()));

    assert_eq!(parse(r#"Rascal("trace(1);")"#), inline("trace(1);"));
    assert_eq!(
        parse(r##"Rascal(r#"trace("a");"#)"##),
        inline(r#"trace("a");"#)
    );
    assert_eq!(parse(r#"Rascal(source: "trace(1);")"#), inline("trace(1);"));
    // The struct can also be written in its own parentheses.
    assert_eq!(
        parse(r#"Rascal((source: "trace(1);"))"#),
        inline("trace(1);")
    );
    assert_eq!(
        parse(r#"Rascal(file: "a.as")"#),
        rascal(RascalSource::File("a.as".to_owned()))
    );

    let parse_err = |data| parse(data).unwrap_err();
    assert!(parse_err("Rascal(source: \"a\", file: \"b\")").contains("found both"));
    assert!(parse_err("Rascal(source: None)").contains("expected `source` or `file`"));
    assert!(parse_err("Rascal(1)").contains("Expected a script or a struct"));
    assert!(parse_err("Rascal()").contains("Expected a script or a struct"));
    assert!(parse_err("Rascal(script: \"a\")").contains(
        "Unexpected field named `script` in `Rascal`, expected either `source` or `file`"
    ));
}

#[cfg(feature = "rascal")]
mod rascal {
    use super::*;

    /// Provides the given files, with their contents as text.
    struct TextFiles(&'static [(&'static str, &'static str)]);

    impl FileProvider for TextFiles {
        fn read(&self, path: &str) -> anyhow::Result<Vec<u8>> {
            let (_, contents) = self
                .0
                .iter()
                .find(|(name, _)| *name == path)
                .ok_or_else(|| anyhow::anyhow!("no file named {path}"))?;
            Ok(contents.as_bytes().to_vec())
        }
    }

    #[test]
    fn script() {
        // A constant pool with "hello", a push of the constant, Trace and End.
        let hello = [
            [0x12, 0x03, 0x88, 0x08, 0x00, 0x01, 0x00].as_slice(),
            b"hello\0",
            &[0x96, 0x02, 0x00, 0x08, 0x00, 0x26, 0x00],
        ]
        .concat();
        assert_eq!(
            compile_tags!(r##"DoAction(Rascal(r#"trace("hello");"#))"##),
            hello
        );
        assert_eq!(
            compile_tags!(r##"DoAction(Rascal(source: r#"trace("hello");"#))"##),
            hello
        );
        // A script without code compiles to no bytecode, not even End.
        assert_eq!(compile_tags!(r#"DoAction(Rascal(""))"#), [0x00, 0x03]);
        assert_eq!(
            compile_tags!(r#"DoAction(Rascal("// nothing"))"#),
            [0x00, 0x03]
        );
        // The bytecode can be used as any other data.
        assert_eq!(
            compile_tags!(r#"DefineBinaryData(id: 1, data: Rascal(""))"#),
            [0xC6, 0x15, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn pcode() {
        // An empty constant pool, a push of "hello", Trace and End.
        assert_eq!(
            compile_tags!(
                r##"DoAction(Rascal(r#"
                    @PCode {
                        Push "hello"
                        Trace
                    }
                "#))"##
            ),
            [
                [0x11, 0x03, 0x88, 0x02, 0x00, 0x00, 0x00].as_slice(),
                &[0x96, 0x07, 0x00, 0x00],
                b"hello\0",
                &[0x26, 0x00]
            ]
            .concat()
        );
    }

    #[test]
    fn swf_version() {
        // Functions are defined with DefineFunction2 (0x8E) from SWF 7, and with
        // DefineFunction (0x9B) before.
        let compile = |version| {
            swfasm::compile_str(
                &format!(
                    r#"Swf(version: {version}, tags: [
                        DoAction(Rascal("function f(a) {{ trace(a); }}")),
                    ])"#
                ),
                &NoFiles,
            )
            .unwrap()
        };
        assert!(compile(6).contains(&0x9B) && !compile(6).contains(&0x8E));
        assert!(compile(7).contains(&0x8E) && !compile(7).contains(&0x9B));
    }

    #[test]
    fn includes() {
        let files = TextFiles(&[("a/b.as", r#"trace("hello");"#)]);
        assert_eq!(
            compile_tags!(r##"DoAction(Rascal(r#"#include "a/b.as""#))"##, &files),
            compile_tags!(r##"DoAction(Rascal(r#"trace("hello");"#))"##)
        );
        // Errors of the provider are kept.
        assert!(
            compile_err!(
                r##"Swf(version: 10, tags: [DoAction(Rascal(r#"#include "c.as""#))])"##,
                &files
            )
            .contains("Could not load file 'c.as': no file named c.as")
        );
        assert!(
            compile_err!(r##"Swf(version: 10, tags: [DoAction(Rascal(r#"#include "c.as""#))])"##)
                .contains("Could not load file 'c.as': files are not available")
        );
    }

    #[test]
    fn file() {
        let files = TextFiles(&[
            ("a/b.as", r#"trace("hello");"#),
            // Included files are relative to the swfasm source, not to the
            // file that includes them.
            ("a/c.as", r#"#include "a/b.as""#),
            ("d.as", "class Foo {}"),
        ]);
        let inline = compile_tags!(r##"DoAction(Rascal(r#"trace("hello");"#))"##);
        assert_eq!(
            compile_tags!(r#"DoAction(Rascal(file: "a/b.as"))"#, &files),
            inline
        );
        assert_eq!(
            compile_tags!(r#"DoAction(Rascal(file: "a/c.as"))"#, &files),
            inline
        );
        // Errors point to the file.
        assert!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAction(Rascal(file: "d.as"))])"#,
                &files
            )
            .contains("failed to compile the ActionScript: error: Compile error\n  ╭▸ d.as:1:1")
        );
        assert!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAction(Rascal(file: "e.as"))])"#,
                &files
            )
            .contains("failed to compile the ActionScript: error: no file named e.as\n ─▸ e.as")
        );
    }

    #[test]
    fn errors() {
        assert!(
            compile_err!(r#"Swf(version: 10, tags: [ShowFrame, DoAction(Rascal("trace("))])"#)
                .contains("tag #1: failed to compile the ActionScript: error: Compile error")
        );
        assert!(
            compile_err!(r#"Swf(version: 10, tags: [DoAction(Rascal("trace(\n1 2)"))])"#)
                .contains("╭▸ <inline>:2:3")
        );
        // Classes are loaded from files, and compiled into separate tags.
        let files = TextFiles(&[("Foo.as", "class Foo {}"), ("a/Bar.as", "class a.Bar {}")]);
        assert!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAction(Rascal("var foo = new Foo();"))])"#,
                &files
            )
            .contains("classes and interfaces are not supported")
        );
        assert!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAction(Rascal("import a.Bar; var bar: Bar;"))])"#,
                &files
            )
            .contains("classes and interfaces are not supported")
        );
        assert!(
            compile_err!(r#"Swf(version: 10, tags: [DoAction(Rascal("class Foo {}"))])"#)
                .contains("Classes may only be defined in external ActionScript 2.0 class scripts")
        );
    }
}

#[cfg(not(feature = "rascal"))]
#[test]
fn rascal_not_available() {
    for data in [r#"Rascal("")"#, r#"Rascal(file: "a.as")"#] {
        let source = format!("Swf(version: 10, tags: [DoAction({data})])");
        // Sources that use Rascal are parsed, but fail to compile.
        swfasm::parse(&source).unwrap();
        assert!(compile_err!(source).contains("Rascal is not available"));
    }
}

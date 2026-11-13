use swf::{
    ClipEventFlag, ColorTransform, Compression, Fixed8, Fixed16, Matrix, PlaceObject,
    PlaceObjectAction, Rectangle, SwfStr, Twips,
};
use swfasm::Compiler;
#[cfg(feature = "asc")]
use swfasm::asc::{AscConfig, AscFile};
use swfasm::files::{FileProvider, NoFiles};
use swfasm::source::{Asc, ClipEvent, Data, Rascal, ScriptSource, Tag};

/// Compiles the given source with files from the given provider, and with asc
/// from `vendor/asc`.
fn compile_str(text: &str, files: &dyn FileProvider) -> anyhow::Result<Vec<u8>> {
    let compiler = Compiler::new().with_files(files);
    #[cfg(feature = "asc")]
    let compiler = compiler.with_asc(vendored_asc());
    compiler.compile_str(text)
}

/// The files in `vendor/asc`, by their paths.
#[cfg(feature = "asc")]
fn vendored_asc() -> AscConfig {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("vendor/asc");
    AscConfig {
        jar: AscFile::Path(dir.join("asc.jar")),
        playerglobal: AscFile::Path(dir.join("playerglobal.abc")),
    }
}

// These are macros instead of functions, so that failures point to the test.

/// Compiles the given tags and returns the serialized tag stream.
///
/// Files are read from the given provider, or are not available without one.
/// The SWF version is 10 unless given.
macro_rules! compile_tags {
    ($tags:expr) => {
        compile_tags!($tags, &NoFiles)
    };
    ($tags:expr, version: $version:expr) => {
        compile_tags!($tags, &NoFiles, version: $version)
    };
    ($tags:expr, $files:expr) => {
        compile_tags!($tags, $files, version: 10)
    };
    ($tags:expr, $files:expr, version: $version:expr) => {{
        let compile = |tags: &str| {
            compile_str(
                &format!(
                    "Swf(version: {}, num_frames: 0, tags: [{tags}])",
                    $version
                ),
                $files,
            )
        };
        let header_len = compile("").unwrap().len();
        compile(&$tags).unwrap()[header_len..].to_vec()
    }};
}

/// Compiles the given `PlaceObject` tag of any version, and reads it back with
/// the swf crate.
macro_rules! read_place_object {
    ($tag:expr) => {{
        let swf = compile_str(
            &format!("Swf(version: 10, tags: [{}, End])", $tag),
            &NoFiles,
        )
        .unwrap();
        // Leaked, so that the tag can borrow from it.
        let swf_buf = Box::leak(Box::new(swf::decompress_swf(&swf[..]).unwrap()));
        // The End tag is not included in the tags.
        match <[swf::Tag; 1]>::try_from(swf::parse_swf(swf_buf).unwrap().tags) {
            Ok([swf::Tag::PlaceObject(place_object)]) => place_object,
            tags => panic!("expected a single PlaceObject tag, got {tags:?}"),
        }
    }};
}

/// Returns a `PlaceObject` with only the version, the action and the depth set,
/// for tests to set the other fields with `..`.
fn bare_place_object(version: u8, action: PlaceObjectAction, depth: u16) -> PlaceObject<'static> {
    PlaceObject {
        version,
        action,
        depth,
        matrix: None,
        color_transform: None,
        ratio: None,
        name: None,
        clip_depth: None,
        class_name: None,
        filters: None,
        background_color: None,
        blend_mode: None,
        clip_actions: None,
        has_image: false,
        is_bitmap_cached: None,
        is_visible: None,
        amf_data: None,
    }
}

/// Compiles the given source, which must fail, and returns the error message.
///
/// Files are read from the given provider, or are not available without one.
macro_rules! compile_err {
    ($source:expr) => {
        compile_err!($source, &NoFiles)
    };
    ($source:expr, $files:expr) => {
        format!("{:#}", compile_str(&$source, $files).unwrap_err())
    };
}

/// Compiles the given tags, which must fail, and returns the error message.
///
/// The tags follow a `ShowFrame` tag, so that errors of the first one point to
/// `tag #1`. The SWF version is 10 unless given.
macro_rules! tag_err {
    ($tags:expr) => {
        tag_err!($tags, version: 10)
    };
    ($tags:expr, version: $version:expr) => {
        compile_err!(format!(
            "Swf(version: {}, tags: [ShowFrame, {}])",
            $version, $tags
        ))
    };
}

/// Asserts that the given error message contains the expected text, and shows
/// the message if it doesn't.
macro_rules! assert_err {
    ($err:expr, $expected:expr) => {{
        let err: String = $err;
        let expected: &str = &$expected;
        assert!(
            err.contains(expected),
            "expected an error containing {expected:?}, got {err:?}"
        );
    }};
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
    let swf = compile_str(
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
    let swf = compile_str(
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
        let swf = compile_str(&format!("Swf(version: 10, tags: [{tags}])"), &NoFiles).unwrap();
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
    assert_err!(
        compile_err!(format!(
            "Swf(version: 10, tags: [{}])",
            "ShowFrame,".repeat(65536)
        )),
        "too many frames"
    );
}

#[test]
fn explicit_num_frames() {
    let num_frames = |num_frames: u16, tags: &str| {
        let swf = compile_str(
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
    // SWF19.
    for (field, flags) in [
        ("", 0x00),
        ("use_network: true", 0x01),
        ("action_script_3: true", 0x08),
        ("has_metadata: true", 0x10),
        ("use_gpu: true", 0x20),
        ("use_direct_blit: true", 0x40),
        ("use_network: false, use_gpu: false", 0x00),
        (
            "action_script_3: true,
            use_network: true,
            has_metadata: true,
            use_gpu: true,
            use_direct_blit: true",
            0x79,
        ),
    ] {
        assert_eq!(
            compile_tags!(format!("FileAttributes({field})")),
            [0x44, 0x11, flags, 0x00, 0x00, 0x00],
            "{field}"
        );
    }
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
    // The longest body that fits in the short header form, and the shortest
    // that does not. Headers of all tags are written the same way.
    assert_eq!(
        compile_tags!(&format!(
            r#"Raw(code: 2, data: InlineBytes("{}"))"#,
            "00".repeat(62)
        )),
        [[0xBE, 0x00].as_slice(), &[0; 62]].concat()
    );
    assert_eq!(
        compile_tags!(&format!(
            r#"Raw(code: 2, data: InlineBytes("{}"))"#,
            "00".repeat(63)
        )),
        [[0xBF, 0x00, 63, 0x00, 0x00, 0x00].as_slice(), &[0; 63]].concat()
    );
    assert_eq!(
        compile_tags!(&format!(
            r#"Raw(code: 2, data: InlineBytes("{}"))"#,
            "00".repeat(0x1234)
        )),
        [
            [0xBF, 0x00, 0x34, 0x12, 0x00, 0x00].as_slice(),
            &[0; 0x1234]
        ]
        .concat()
    );
}

#[test]
fn tag_code_errors() {
    assert_err!(
        tag_err!(r#"Raw(code: 1100, data: InlineBytes(""))"#),
        "tag code 1100 does not fit in 10 bits"
    );
}

/// Each struct of the source, in the smallest tag that has it, with `{}` where
/// a field can be added.
const STRUCTS: &[&str] = &[
    r#"Raw(code: 2, data: InlineBytes(""){})"#,
    "PlaceObject(character: 1, depth: 1{})",
    "PlaceObject(character: 1, depth: 1, matrix: (scale_x: 1{}))",
    "PlaceObject(character: 1, depth: 1, color_transform: (red_add: 0{}))",
    "RemoveObject(character: 1, depth: 1{})",
    r#"DefineBits(id: 1, data: InlineBytes(""){})"#,
    r#"DefineButton(id: 1, records: [], actions: InlineBytes(""){})"#,
    r#"DefineButton(
        id: 1,
        records: [(states: [], character: 1, depth: 1{})],
        actions: InlineBytes(""),
    )"#,
    r#"DefineBitsJpeg2(id: 1, data: InlineBytes(""){})"#,
    r#"Protect(password: "a"{})"#,
    "PlaceObject2(depth: 1{})",
    r#"PlaceObject2(depth: 1, clip_actions: [(events: [], actions: InlineBytes(""){})])"#,
    "RemoveObject2(depth: 1{})",
    "DefineSprite(id: 1, tags: []{})",
    r#"NameCharacter(character: 1, name: "a"{})"#,
    "ProductInfo(
        product_id: 0,
        edition: 0,
        major_version: 0,
        minor_version: 0,
        build_number: 0,
        compilation_date: 0{}
    )",
    r#"FrameLabel(label: "a"{})"#,
    r#"ExportAssets([(character: 1, name: "a"{})])"#,
    r#"DoInitAction(sprite: 1, actions: InlineBytes(""){})"#,
    "DefineVideoStream(id: 1, num_frames: 0, width: 0, height: 0, codec: H263{})",
    r#"VideoFrame(stream: 1, frame: 0, data: InlineBytes(""){})"#,
    "ScriptLimits(max_recursion_depth: 0, timeout_seconds: 0{})",
    "SetTabIndex(depth: 1, tab_index: 0{})",
    "FileAttributes(action_script_3: false{})",
    "CsmTextSettings(text: 1{})",
    r#"SymbolClass([(character: 1, name: "a"{})])"#,
    "DefineScalingGrid(character: 1, splitter: (width: 1, height: 1){})",
    "DefineScalingGrid(character: 1, splitter: (width: 1, height: 1{}))",
    r#"DoAbc2(data: InlineBytes(""){})"#,
    "DefineSceneAndFrameLabelData(scenes: []{})",
    r#"DefineSceneAndFrameLabelData(scenes: [(frame: 0, name: "a"{})])"#,
    r#"DefineBinaryData(id: 1, data: InlineBytes(""){})"#,
    r#"DefineFontName(font: 1, name: "a"{})"#,
];

/// Like [`STRUCTS`], for structs that need a feature to compile, with whether
/// it is enabled.
const FEATURE_DEPENDENT_STRUCTS: &[(bool, &str)] = &[
    (
        cfg!(feature = "rascal"),
        r#"DoAction(Rascal(source: ""{}))"#,
    ),
    (cfg!(feature = "asc"), r#"DoAbc(Asc(source: ""{}))"#),
];

#[test]
fn minimal_structs() {
    // Only the fields in the table are required.
    for tag in STRUCTS {
        compile_tags!(tag.replace("{}", ""));
    }
    for (enabled, tag) in FEATURE_DEPENDENT_STRUCTS {
        if *enabled {
            compile_tags!(tag.replace("{}", ""));
        }
    }
}

#[test]
fn unknown_fields() {
    // Structs are parsed even if the feature they need is not enabled.
    let feature_dependent_structs = FEATURE_DEPENDENT_STRUCTS.iter().map(|(_, tag)| tag);
    for tag in STRUCTS.iter().chain(feature_dependent_structs) {
        assert_err!(
            tag_err!(tag.replace("{}", ", zzz: 1")),
            "Unexpected field named `zzz`"
        );
    }
    assert_err!(
        compile_err!("Swf(version: 10, zzz: 1)"),
        "Unexpected field named `zzz` in `Swf`"
    );
    assert_err!(
        compile_err!("Swf(version: 10, stage: (width: 1, height: 1, zzz: 1))"),
        "Unexpected field named `zzz` in `Rect`"
    );
}

#[test]
fn ron_errors() {
    // The messages are compared whole, as they are a part of the interface.
    let err = |tags: &str| compile_err!(format!("Swf(version: 10, tags: [{tags}])"));
    assert_eq!(
        err(r#"DefineBits(data: InlineBytes(""))"#),
        "1:57: Unexpected missing field named `id` in `DefineBits`"
    );
    assert_eq!(
        err(r#"ExportAssets([(name: "a")])"#),
        "1:49: Unexpected missing field named `character` in `ExportedAsset`"
    );
    assert_eq!(
        err("RemoveObject2(depth: 1, zzz: 1)"),
        "1:49-1:52: Unexpected field named `zzz` in `RemoveObject2`, expected `depth` instead"
    );
    assert_eq!(
        err("CsmTextSettings(text: 1, grid_fit: Grid)"),
        "1:60-1:64: Unexpected variant named `Grid` in enum `GridFit`, expected one of `None`, \
         `Pixel`, or `SubPixel` instead"
    );
    assert_eq!(
        err("RemoveObject2(depth: 65536)"),
        "1:46-1:51: Integer is out of bounds"
    );
    assert_eq!(
        err("RemoveObject2(depth: -1)"),
        "1:47-1:48: Integer is out of bounds"
    );
    assert_eq!(
        err("NameCharacter(character: 1, name: 1)"),
        "1:58-1:59: Expected string"
    );
    assert_eq!(err(r#"DoAction("00")"#), "1:34: Expected identifier");
    assert_eq!(
        compile_err!(r#"Swf(version: 10, frame_rate: "a")"#),
        "1:29-1:30: Expected float"
    );
    // Positions count lines and columns of the whole source.
    assert_eq!(
        compile_err!("Swf(\n    version: 10,\n    tags: [ShowFrame ShowFrame],\n)"),
        "3:22: Expected comma"
    );
}

/// Each field of a tag that takes data, with `{}` in place of the data.
const DATA_FIELDS: &[&str] = &[
    "Raw(code: 2, data: {})",
    "Raw(code: 2, force_long: true, data: {})",
    "PlaceObject2(depth: 1, clip_actions: [(events: [Load], actions: {})])",
    "DefineBits(id: 1, data: {})",
    "DefineButton(id: 1, records: [], actions: {})",
    "JpegTables({})",
    "DoAction({})",
    "DefineBitsJpeg2(id: 1, data: {})",
    "DoInitAction(sprite: 1, actions: {})",
    "VideoFrame(stream: 1, frame: 0, data: {})",
    "DebugId({})",
    "DoAbc({})",
    "Metadata({})",
    "DoAbc2(data: {})",
    "DefineBinaryData(id: 1, data: {})",
];

#[test]
fn data_from_file() {
    // Data from a file is written like the same data inline. The path is 16
    // bytes long, as `DebugId` takes exactly 16 bytes.
    for field in DATA_FIELDS {
        assert_eq!(
            compile_tags!(
                field.replace("{}", r#"File("0123456789abcdef")"#),
                &PathAsContents
            ),
            compile_tags!(
                field.replace("{}", r#"InlineBytes("30313233343536373839616263646566")"#)
            ),
            "{field}"
        );
    }
    // The path is passed to the provider exactly as it is written, with
    // escapes resolved.
    assert_eq!(
        compile_tags!(r#"DoAction(File("./a/../\u{62}"))"#, &PathAsContents),
        compile_tags!(r#"DoAction(InlineBytes("2e 2f 61 2f 2e 2e 2f 62"))"#)
    );
    assert_eq!(
        compile_tags!(r#"DoAction(File(""))"#, &PathAsContents),
        compile_tags!(r#"DoAction(InlineBytes(""))"#)
    );
    // Files are read in nested tags too.
    assert_eq!(
        compile_tags!(
            r#"DefineSprite(id: 1, num_frames: 0, tags: [DoAction(File("ab"))])"#,
            &PathAsContents
        ),
        compile_tags!(
            r#"DefineSprite(id: 1, num_frames: 0, tags: [DoAction(InlineBytes("61 62"))])"#
        )
    );
}

#[test]
fn data_file_errors() {
    for field in DATA_FIELDS {
        assert_err!(
            tag_err!(field.replace("{}", r#"File("a.bin")"#)),
            r#"failed to read file "a.bin": files are not available"#
        );
    }
    // Errors of the provider are kept, and point to the nested tag.
    assert_err!(
        compile_err!(
            r#"Swf(version: 10, tags: [DefineSprite(id: 1, tags: [DoAction(File("a.bin"))])])"#,
            &Failing
        ),
        r#"tag #0: tag #0: failed to read file "a.bin": no file named a.bin"#
    );
}

#[test]
fn data_errors() {
    let err = |data: &str| tag_err!(format!("DoAction({data})"));
    assert_err!(err(r#""07 00""#), "Expected identifier");
    assert_err!(
        err(r#"Hex("07 00")"#),
        "Unexpected variant named `Hex` in enum `Data`"
    );
    assert_err!(err("File(1)"), "Expected string");
    assert_err!(
        err(r#"InlineBytes("070")"#),
        r#"hex string "070" has an odd number of digits"#
    );
    assert_err!(
        err(r#"InlineBytes(["00", "0"])"#),
        r#"hex string "0" has an odd number of digits"#
    );
    assert_err!(err(r#"InlineBytes("0g")"#), "invalid hex digit 'g'");
    assert_err!(err("InlineBytes([1, 2])"), "Expected string");
}

/// Each string field of a tag, with `{}` in place of the string, and the
/// context of its errors.
const STRING_FIELDS: &[(&str, &str)] = &[
    ("Protect(password: {})", "password"),
    ("PlaceObject2(depth: 1, name: {})", "name"),
    ("NameCharacter(character: 1, name: {})", "name"),
    ("FrameLabel({})", "label"),
    ("ExportAssets([(character: 1, name: {})])", "asset #0: name"),
    ("SymbolClass([(character: 1, name: {})])", "link #0: name"),
    (r#"DoAbc2(name: {}, data: InlineBytes(""))"#, "name"),
    (
        "DefineSceneAndFrameLabelData(scenes: [(frame: 0, name: {})])",
        "scenes: frame #0: name",
    ),
    (
        "DefineSceneAndFrameLabelData(frame_labels: [(frame: 0, name: {})])",
        "frame_labels: frame #0: name",
    ),
    ("DefineFontName(font: 1, name: {})", "name"),
    (
        r#"DefineFontName(font: 1, name: "", copyright: {})"#,
        "copyright",
    ),
];

#[test]
fn strings_as_utf8() {
    // Strings are written as UTF-8 in all SWF versions: like an ASCII string of
    // the same length, with its bytes replaced.
    let utf8 = "żółw";
    let ascii = "a".repeat(utf8.len());
    for (field, _) in STRING_FIELDS {
        for version in [5, 10] {
            let mut expected = compile_tags!(
                field.replace("{}", &format!("{ascii:?}")),
                version: version
            );
            let offset = expected
                .windows(ascii.len())
                .position(|window| window == ascii.as_bytes())
                .unwrap();
            expected.splice(offset..offset + ascii.len(), utf8.bytes());
            assert_eq!(
                compile_tags!(field.replace("{}", &format!("{utf8:?}")), version: version),
                expected,
                "{field}, version {version}"
            );
        }
    }
}

#[test]
fn string_errors() {
    // Strings are null-terminated, so they can't contain null characters.
    for (field, context) in STRING_FIELDS {
        assert_err!(
            tag_err!(field.replace("{}", r#""a\0b""#)),
            format!(r#"tag #1: {context}: "a\0b" contains a null character"#)
        );
    }
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
}

#[test]
fn invalid_values() {
    assert_err!(
        compile_err!("Swf(version: 10, stage: (width: 0.01, height: 1))"),
        "stage: width: 0.01px is not a whole number of twips"
    );
    assert_err!(
        compile_err!("Swf(version: 10, stage: (x: 53687091, width: 0.2, height: 1))"),
        "stage: width: 53687091.2px is out of range"
    );
    assert_err!(
        compile_err!("Swf(version: 10, frame_rate: 0.001)"),
        "frame rate: 0.001 is not representable as an 8.8 fixed point number"
    );
    assert_err!(
        tag_err!("SetBackgroundColor(0x1000000)"),
        "background color 0x1000000 is not in the 0xRRGGBB format"
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
}

#[test]
fn define_sprite_num_frames() {
    let num_frames = |tags: &str| {
        let swf = compile_str(&format!("Swf(version: 10, tags: [{tags}])"), &NoFiles).unwrap();
        swf::decompress_swf(&swf[..]).unwrap().header.num_frames()
    };
    // Frames of sprites do not count towards the main timeline.
    assert_eq!(num_frames("DefineSprite(id: 1, tags: [ShowFrame])"), 0);
    assert_eq!(
        num_frames("DefineSprite(id: 1, tags: [ShowFrame]), ShowFrame"),
        1
    );
    assert_err!(
        compile_err!(format!(
            "Swf(version: 10, tags: [DefineSprite(id: 1, tags: [{}])])",
            "ShowFrame,".repeat(65536)
        )),
        "tag #0: too many frames"
    );
}

#[test]
fn define_sprite_errors() {
    // Errors in nested tags point to both the sprite and the nested tag.
    assert_err!(
        compile_err!(
            "Swf(version: 10, tags: [
                ShowFrame,
                DefineSprite(id: 1, tags: [ShowFrame, SetBackgroundColor(0x1000000)]),
            ])"
        ),
        "tag #1: tag #1: background color 0x1000000 is not in the 0xRRGGBB format"
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
fn place_object() {
    // The body is the character ID, the depth and the matrix, which is the
    // identity matrix by default.
    assert_eq!(
        compile_tags!("PlaceObject(character: 0x1234, depth: 0x5678)"),
        [0x05, 0x01, 0x34, 0x12, 0x78, 0x56, 0x00]
    );
    assert_eq!(
        compile_tags!(
            "PlaceObject(character: 1, depth: 2, matrix: (translate_x: 1, translate_y: -1))"
        ),
        [0x07, 0x01, 0x01, 0x00, 0x02, 0x00, 0x0C, 0xA5, 0x80]
    );
    // The color transform has no alpha terms.
    assert_eq!(
        compile_tags!("PlaceObject(character: 1, depth: 2, color_transform: (red_add: 255))"),
        [
            0x0A, 0x01, 0x01, 0x00, 0x02, 0x00, 0x00, 0xA5, 0xFE, 0x00, 0x00, 0x00
        ]
    );
    assert_eq!(
        compile_tags!("PlaceObject(character: 1, depth: 2, color_transform: ())"),
        [0x06, 0x01, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00]
    );

    assert_eq!(
        *read_place_object!(
            "PlaceObject(
                character: 1,
                depth: 2,
                matrix: (scale_x: 2),
                color_transform: (red_mult: 0.5, green_add: -10, blue_add: 255),
            )"
        ),
        PlaceObject {
            matrix: Some(Matrix::scale(Fixed16::from_f64(2.0), Fixed16::ONE)),
            color_transform: Some(ColorTransform {
                r_multiply: Fixed8::from_f64(0.5),
                g_add: -10,
                b_add: 255,
                ..Default::default()
            }),
            ..bare_place_object(1, PlaceObjectAction::Place(1), 2)
        }
    );
}

#[test]
fn place_object_matrix() {
    let matrix = "matrix: (
        scale_x: 2,
        scale_y: -0.5,
        rotate_skew0: 0.25,
        rotate_skew1: -1,
        translate_x: 10.5,
        translate_y: -3,
    )";
    assert_eq!(
        compile_tags!(format!("PlaceObject(character: 1, depth: 2, {matrix})")),
        [
            0x12, 0x01, 0x01, 0x00, 0x02, 0x00, 0xCD, 0x00, 0x00, 0x78, 0x00, 0x0C, 0x48, 0x00,
            0x10, 0x00, 0x04, 0xB4, 0xB8, 0x80
        ]
    );
    assert_eq!(
        *read_place_object!(format!("PlaceObject(character: 2, depth: 1, {matrix})")),
        PlaceObject {
            matrix: Some(Matrix {
                a: Fixed16::from_f64(2.0),
                b: Fixed16::from_f64(0.25),
                c: Fixed16::from_f64(-1.0),
                d: Fixed16::from_f64(-0.5),
                tx: Twips::new(210),
                ty: Twips::new(-60),
            }),
            ..bare_place_object(1, PlaceObjectAction::Place(2), 1)
        }
    );

    // The extremes of the ranges, which use all 31 bits.
    assert_eq!(
        *read_place_object!(
            "PlaceObject(character: 2, depth: 1, matrix: (
                scale_x: -16384,
                scale_y: 16383.9999847412109375,
                rotate_skew0: 16383.9999847412109375,
                rotate_skew1: -16384,
                translate_x: -53687091.2,
                translate_y: 53687091.15,
            ))"
        ),
        PlaceObject {
            matrix: Some(Matrix {
                a: Fixed16::from_bits(-1 << 30),
                b: Fixed16::from_bits((1 << 30) - 1),
                c: Fixed16::from_bits(-1 << 30),
                d: Fixed16::from_bits((1 << 30) - 1),
                tx: Twips::new(-1 << 30),
                ty: Twips::new((1 << 30) - 1),
            }),
            ..bare_place_object(1, PlaceObjectAction::Place(2), 1)
        }
    );
}

#[test]
fn place_object_errors() {
    assert_err!(
        tag_err!("PlaceObject(character: 1, depth: 1, color_transform: (alpha_mult: 0.5))"),
        "tag #1: color_transform: PlaceObject cannot encode non-default alpha terms"
    );
    assert_err!(
        tag_err!("PlaceObject(character: 1, depth: 1, color_transform: (alpha_add: 1))"),
        "tag #1: color_transform: PlaceObject cannot encode non-default alpha terms"
    );
}

#[test]
fn place_object_invalid_values() {
    let matrix_err = |matrix: &str| {
        tag_err!(&format!(
            "PlaceObject(character: 1, depth: 1, matrix: ({matrix}))"
        ))
    };
    assert_err!(
        matrix_err("scale_x: 0.1"),
        "matrix: scale_x: 0.1 is not representable as a 16.16 fixed point number"
    );
    assert_err!(
        matrix_err("scale_y: 16384"),
        "matrix: scale_y: 16384 is out of range"
    );
    assert_err!(
        matrix_err("rotate_skew0: -16384.0000152587890625"),
        "matrix: rotate_skew0: -16384.00001525879 is out of range"
    );
    assert_err!(
        matrix_err("rotate_skew1: 1e10"),
        "matrix: rotate_skew1: 10000000000 is not representable"
    );
    assert_err!(
        matrix_err("translate_x: 0.01"),
        "matrix: translate_x: 0.01px is not a whole number of twips"
    );
    assert_err!(
        matrix_err("translate_y: 53687091.2"),
        "matrix: translate_y: 53687091.2px is out of range"
    );
    assert_err!(
        tag_err!("PlaceObject(character: 1, depth: 1, matrix: (translate_x: -53687091.25))"),
        "matrix: translate_x: -53687091.25px is out of range"
    );

    let color_err = |color_transform: &str| {
        tag_err!(&format!(
            "PlaceObject(character: 1, depth: 1, color_transform: ({color_transform}))"
        ))
    };
    assert_err!(
        color_err("red_mult: 0.001"),
        "color_transform: red_mult: 0.001 is not representable as an 8.8 fixed point number"
    );
    assert_err!(
        color_err("green_mult: 64"),
        "color_transform: green_mult: 64 is out of range"
    );
    assert_err!(
        color_err("blue_add: 16384"),
        "color_transform: blue_add: 16384 is out of range"
    );
}

#[test]
fn place_object_2() {
    // The body is the flags, the depth, and the fields that are set.
    assert_eq!(
        compile_tags!("PlaceObject2(depth: 0x1234, character: 0x5678)"),
        [0x85, 0x06, 0x02, 0x34, 0x12, 0x78, 0x56]
    );
    assert_eq!(
        compile_tags!("PlaceObject2(depth: 0x1234, move: true)"),
        [0x83, 0x06, 0x01, 0x34, 0x12]
    );
    assert_eq!(
        compile_tags!("PlaceObject2(depth: 0x1234, move: true, character: 0x5678)"),
        [0x85, 0x06, 0x03, 0x34, 0x12, 0x78, 0x56]
    );
    // Neither a character nor the move flag is written as is.
    assert_eq!(
        compile_tags!("PlaceObject2(depth: 0x1234)"),
        [0x83, 0x06, 0x00, 0x34, 0x12]
    );
    assert_eq!(
        compile_tags!(
            r#"PlaceObject2(
                depth: 1,
                move: true,
                character: 2,
                matrix: (),
                color_transform: (),
                ratio: 0x0304,
                name: "ab",
                clip_depth: 5,
                clip_actions: [],
            )"#
        ),
        [
            0x98, 0x06, 0xFF, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x04, 0x03, 0x61, 0x62, 0x00,
            0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );

    assert_eq!(
        *read_place_object!(
            r#"PlaceObject2(
                depth: 1,
                move: true,
                character: 2,
                matrix: (translate_x: 1),
                color_transform: (alpha_mult: 0.5),
                ratio: 3,
                name: "instance",
                clip_depth: 4,
            )"#
        ),
        PlaceObject {
            matrix: Some(Matrix::translate(Twips::new(20), Twips::ZERO)),
            color_transform: Some(ColorTransform {
                a_multiply: Fixed8::from_f64(0.5),
                ..Default::default()
            }),
            ratio: Some(3),
            name: Some(SwfStr::from_utf8_str("instance")),
            clip_depth: Some(4),
            ..bare_place_object(2, PlaceObjectAction::Replace(2), 1)
        }
    );
}

#[test]
fn place_object_2_color_transform() {
    // Unlike in PlaceObject, the color transform has alpha terms.
    assert_eq!(
        compile_tags!("PlaceObject2(depth: 1, color_transform: (alpha_mult: 0.5))"),
        [
            0x89, 0x06, 0x08, 0x01, 0x00, 0x69, 0x00, 0x40, 0x10, 0x02, 0x00
        ]
    );
    assert_eq!(
        compile_tags!("PlaceObject2(depth: 1, color_transform: (red_mult: 0.5, blue_add: -1))"),
        [
            0x8E, 0x06, 0x08, 0x01, 0x00, 0xE8, 0x80, 0x40, 0x10, 0x04, 0x00, 0x00, 0x00, 0x3F,
            0xF0, 0x00
        ]
    );

    // The extremes of the ranges, which use all 15 bits.
    assert_eq!(
        *read_place_object!(
            "PlaceObject2(depth: 1, character: 2, color_transform: (
                red_mult: -64,
                green_mult: 63.99609375,
                blue_mult: 0,
                alpha_mult: -0.00390625,
                red_add: 16383,
                green_add: -16384,
                blue_add: 1,
                alpha_add: -1,
            ))"
        ),
        PlaceObject {
            color_transform: Some(ColorTransform {
                r_multiply: Fixed8::from_bits(-1 << 14),
                g_multiply: Fixed8::from_bits((1 << 14) - 1),
                b_multiply: Fixed8::ZERO,
                a_multiply: Fixed8::from_bits(-1),
                r_add: (1 << 14) - 1,
                g_add: -1 << 14,
                b_add: 1,
                a_add: -1,
            }),
            ..bare_place_object(2, PlaceObjectAction::Place(2), 1)
        }
    );
}

#[test]
fn place_object_2_clip_actions() {
    let tag = r#"PlaceObject2(depth: 1, clip_actions: [
        (events: [Load, EnterFrame], actions: InlineBytes("00")),
        (events: [KeyPress, Data], key_code: 0x41, actions: InlineBytes("07 00")),
    ])"#;
    // Since SWF 6, event flags have 32 bits.
    assert_eq!(
        compile_tags!(tag),
        [
            0xA1, 0x06, 0x80, 0x01, 0x00, // header, flags, depth
            0x00, 0x00, // reserved
            0x03, 0x01, 0x02, 0x00, // all events
            0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, // Load, EnterFrame, size
            0x00, // actions
            0x00, 0x01, 0x02, 0x00, 0x03, 0x00, 0x00, 0x00, // KeyPress, Data, size
            0x41, 0x07, 0x00, // key code, actions
            0x00, 0x00, 0x00, 0x00, // end
        ]
    );
    // Before SWF 6, event flags have 16 bits.
    assert_eq!(
        compile_tags!(
            r#"PlaceObject2(depth: 1, clip_actions: [
                (events: [KeyUp, Data], actions: InlineBytes("00")),
            ])"#,
            version: 5
        ),
        [
            0x90, 0x06, 0x80, 0x01, 0x00, // header, flags, depth
            0x00, 0x00, // reserved
            0x80, 0x01, // all events
            0x80, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, // KeyUp, Data
            0x00, 0x00, // end
        ]
    );
    // Actions are written as is, and clip actions can be empty.
    assert_eq!(
        compile_tags!(
            r#"PlaceObject2(depth: 1, clip_actions: [(events: [], actions: InlineBytes(""))])"#
        ),
        [
            0x95, 0x06, 0x80, 0x01, 0x00, // header, flags, depth
            0x00, 0x00, // reserved
            0x00, 0x00, 0x00, 0x00, // all events
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // no events
            0x00, 0x00, 0x00, 0x00, // end
        ]
    );
    // The key code is written when it is set, even if it doesn't match the
    // KeyPress event.
    assert_eq!(
        compile_tags!(
            r#"PlaceObject2(depth: 1, clip_actions: [(events: [KeyPress], actions: InlineBytes("00"))])"#
        ),
        [
            0x96, 0x06, 0x80, 0x01, 0x00, // header, flags, depth
            0x00, 0x00, // reserved
            0x00, 0x00, 0x02, 0x00, // all events
            0x00, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, // KeyPress, size
            0x00, // actions
            0x00, 0x00, 0x00, 0x00, // end
        ]
    );
    assert_eq!(
        compile_tags!(
            r#"PlaceObject2(depth: 1, clip_actions: [(events: [Load], key_code: 0x41, actions: InlineBytes("00"))])"#
        ),
        [
            0x97, 0x06, 0x80, 0x01, 0x00, // header, flags, depth
            0x00, 0x00, // reserved
            0x01, 0x00, 0x00, 0x00, // all events
            0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, // Load, size
            0x41, // key code
            0x00, // actions
            0x00, 0x00, 0x00, 0x00, // end
        ]
    );
}

#[test]
fn clip_event_flags() {
    for (event, flag) in [
        (ClipEvent::Load, ClipEventFlag::LOAD),
        (ClipEvent::EnterFrame, ClipEventFlag::ENTER_FRAME),
        (ClipEvent::Unload, ClipEventFlag::UNLOAD),
        (ClipEvent::MouseMove, ClipEventFlag::MOUSE_MOVE),
        (ClipEvent::MouseDown, ClipEventFlag::MOUSE_DOWN),
        (ClipEvent::MouseUp, ClipEventFlag::MOUSE_UP),
        (ClipEvent::KeyDown, ClipEventFlag::KEY_DOWN),
        (ClipEvent::KeyUp, ClipEventFlag::KEY_UP),
        (ClipEvent::Data, ClipEventFlag::DATA),
        (ClipEvent::Initialize, ClipEventFlag::INITIALIZE),
        (ClipEvent::Press, ClipEventFlag::PRESS),
        (ClipEvent::Release, ClipEventFlag::RELEASE),
        (ClipEvent::ReleaseOutside, ClipEventFlag::RELEASE_OUTSIDE),
        (ClipEvent::RollOver, ClipEventFlag::ROLL_OVER),
        (ClipEvent::RollOut, ClipEventFlag::ROLL_OUT),
        (ClipEvent::DragOver, ClipEventFlag::DRAG_OVER),
        (ClipEvent::DragOut, ClipEventFlag::DRAG_OUT),
        (ClipEvent::KeyPress, ClipEventFlag::KEY_PRESS),
        (ClipEvent::Construct, ClipEventFlag::CONSTRUCT),
    ] {
        assert_eq!(event as u32, flag.bits(), "{event:?}");
    }
}

#[test]
fn all_clip_events() {
    // The events of all the clip actions can be overridden.
    let clip_actions = r#"clip_actions: [(events: [Load], actions: InlineBytes("00"))]"#;
    assert_eq!(
        compile_tags!(format!(
            "PlaceObject2(depth: 1, move: true, all_clip_events: [Load, Press], {clip_actions})"
        )),
        [
            0x96, 0x06, 0x81, 0x01, 0x00, // header, flags, depth
            0x00, 0x00, // reserved
            0x01, 0x04, 0x00, 0x00, // all events
            0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, // Load, size
            0x00, // actions
            0x00, 0x00, 0x00, 0x00, // end
        ]
    );
    assert_eq!(
        compile_tags!(format!(
            "PlaceObject2(depth: 1, move: true, all_clip_events: [], {clip_actions})"
        )),
        [
            0x96, 0x06, 0x81, 0x01, 0x00, // header, flags, depth
            0x00, 0x00, // reserved
            0x00, 0x00, 0x00, 0x00, // all events
            0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, // Load, size
            0x00, // actions
            0x00, 0x00, 0x00, 0x00, // end
        ]
    );
    // Without clip actions, the tag has clip actions with no event handlers.
    assert_eq!(
        compile_tags!("PlaceObject2(depth: 1, move: true, all_clip_events: [Load])"),
        [
            0x8D, 0x06, 0x81, 0x01, 0x00, // header, flags, depth
            0x00, 0x00, // reserved
            0x01, 0x00, 0x00, 0x00, // all events
            0x00, 0x00, 0x00, 0x00, // end
        ]
    );
    // Before SWF 6, event flags have 16 bits.
    assert_eq!(
        compile_tags!(
            format!("PlaceObject2(depth: 1, move: true, all_clip_events: [Data], {clip_actions})"),
            version: 5
        ),
        [
            0x90, 0x06, 0x81, 0x01, 0x00, // header, flags, depth
            0x00, 0x00, // reserved
            0x00, 0x01, // all events
            0x01, 0x00, 0x01, 0x00, 0x00, 0x00, // Load, size
            0x00, // actions
            0x00, 0x00, // end
        ]
    );

    let err = |tag: &str, version: u8| tag_err!(tag, version: version);
    assert_err!(
        err(
            &format!("PlaceObject2(depth: 1, all_clip_events: [KeyPress], {clip_actions})"),
            5
        ),
        "tag #1: all_clip_events: KeyPress events are not supported before SWF 6"
    );
}

#[test]
fn place_object_2_errors() {
    // Unlike in PlaceObject, alpha terms are checked like the others.
    assert_err!(
        tag_err!("PlaceObject2(depth: 1, color_transform: (alpha_mult: -64.00390625))"),
        "tag #1: color_transform: alpha_mult: -64.00390625 is out of range"
    );
    assert_err!(
        tag_err!("PlaceObject2(depth: 1, color_transform: (alpha_add: -16385))"),
        "tag #1: color_transform: alpha_add: -16385 is out of range"
    );
}

#[test]
fn clip_action_errors() {
    let err = |clip_actions: &str, version: u8| {
        tag_err!(
            format!("PlaceObject2(depth: 1, clip_actions: {clip_actions})"),
            version: version
        )
    };
    for event in ["DragOut", "KeyPress", "Construct"] {
        assert_err!(
            err(
                &format!(r#"[(events: [{event}], key_code: 1, actions: InlineBytes(""))]"#),
                5
            ),
            &format!("clip action #0: {event} events are not supported before SWF 6")
        );
    }
    assert_err!(
        compile_err!(
            r#"Swf(version: 10, tags: [PlaceObject2(depth: 1, clip_actions: [
                (events: [Load], actions: InlineBytes("")),
                (events: [Load], actions: File("x")),
            ])])"#,
            &Failing
        ),
        r#"tag #0: clip_actions: clip action #1: failed to read file "x": no file named x"#
    );
}

#[test]
fn define_button() {
    // The body is the character ID, the records terminated by a 0 byte, and
    // the actions.
    assert_eq!(
        compile_tags!(r#"DefineButton(id: 0x1234, records: [], actions: InlineBytes("00"))"#),
        [
            0xC4, 0x01, // header
            0x34, 0x12, // ID
            0x00, // end of records
            0x00, // actions
        ]
    );
    // A record is its flags, the character ID, the depth and the matrix, which
    // is the identity matrix by default.
    assert_eq!(
        compile_tags!(
            r#"DefineButton(
                id: 1,
                records: [(states: [Up, Over, Down, HitTest], character: 2, depth: 3)],
                actions: InlineBytes("07 00"),
            )"#
        ),
        [
            0xCB, 0x01, // header
            0x01, 0x00, // ID
            0x0F, 0x02, 0x00, 0x03, 0x00, 0x00, // all states, character, depth, matrix
            0x00, // end of records
            0x07, 0x00, // actions
        ]
    );
    assert_eq!(
        compile_tags!(
            r#"DefineButton(
                id: 1,
                records: [
                    (states: [Up], character: 5, depth: 1, matrix: (translate_x: 1, translate_y: -1)),
                    (states: [HitTest], character: 6, depth: 2),
                ],
                actions: InlineBytes("00"),
            )"#
        ),
        [
            0xD2, 0x01, // header
            0x01, 0x00, // ID
            0x01, 0x05, 0x00, 0x01, 0x00, 0x0C, 0xA5, 0x80, // Up, character, depth, matrix
            0x08, 0x06, 0x00, 0x02, 0x00, 0x00, // HitTest, character, depth, matrix
            0x00, // end of records
            0x00, // actions
        ]
    );
}

#[test]
fn define_button_malformed() {
    // Repeated states, states in any order, and characters that don't exist.
    assert_eq!(
        compile_tags!(
            r#"DefineButton(
                id: 1,
                records: [
                    (states: [HitTest, Up, Up], character: 0xFFFF, depth: 1),
                    (states: [Down], character: 0xFFFF, depth: 1),
                ],
                actions: InlineBytes("00"),
            )"#
        ),
        [
            0xD0, 0x01, // header
            0x01, 0x00, // ID
            0x09, 0xFF, 0xFF, 0x01, 0x00, 0x00, // HitTest and Up, character, depth, matrix
            0x04, 0xFF, 0xFF, 0x01, 0x00, 0x00, // Down, character, depth, matrix
            0x00, // end of records
            0x00, // actions
        ]
    );
    // A record without states.
    assert_eq!(
        compile_tags!(
            r#"DefineButton(id: 1, records: [(states: [], character: 1, depth: 1)], actions: InlineBytes("00"))"#
        ),
        [
            0xCA, 0x01, // header
            0x01, 0x00, // ID
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, // no states, character, depth, matrix
            0x00, // end of records
            0x00, // actions
        ]
    );
    // Actions are written as is: no End action is appended.
    assert_eq!(
        compile_tags!(r#"DefineButton(id: 1, records: [], actions: InlineBytes(""))"#),
        [
            0xC3, 0x01, // header
            0x01, 0x00, // ID
            0x00, // end of records
        ]
    );
}

#[test]
fn define_button_errors() {
    let record_err = |record: &str| {
        tag_err!(&format!(
            r#"DefineButton(
                id: 1,
                records: [(states: [Up], character: 1, depth: 1), {record}],
                actions: InlineBytes("00"),
            )"#
        ))
    };
    assert_err!(
        record_err("(states: [Up], character: 1, depth: 2, matrix: (scale_x: 0.1))"),
        "tag #1: records: record #1: matrix: scale_x: 0.1 is not representable as a 16.16 fixed point number"
    );
    assert_err!(
        compile_err!(
            r#"Swf(version: 10, tags: [DefineButton(id: 1, records: [], actions: File("x"))])"#,
            &Failing
        ),
        r#"tag #0: actions: failed to read file "x": no file named x"#
    );
}

#[test]
fn define_scene_and_frame_label_data() {
    // The body is the count of the scenes and their frames and names, then
    // the same for the labeled frames.
    assert_eq!(
        compile_tags!(
            r#"DefineSceneAndFrameLabelData(
                scenes: [(frame: 0, name: "ab"), (frame: 5, name: "c")],
                frame_labels: [(frame: 7, name: "d")],
            )"#
        ),
        [
            0x8C, 0x15, // header: code 86, length 12
            0x02, // two scenes
            0x00, b'a', b'b', 0x00, // scene #0
            0x05, b'c', 0x00, // scene #1
            0x01, // one labeled frame
            0x07, b'd', 0x00, // frame #0
        ]
    );
    // Both lists default to empty, and the counts are still written.
    assert_eq!(
        compile_tags!("DefineSceneAndFrameLabelData()"),
        [0x82, 0x15, 0x00, 0x00]
    );
    // Frame numbers above 127 take more than one byte.
    assert_eq!(
        compile_tags!(r#"DefineSceneAndFrameLabelData(scenes: [(frame: 128, name: "")])"#),
        [
            0x85, 0x15, // header: code 86, length 5
            0x01, // one scene
            0x80, 0x01, 0x00, // scene #0: frame 128, an empty name
            0x00, // no labeled frames
        ]
    );
}

#[test]
fn define_scene_and_frame_label_data_errors() {
    let err = |fields: &str| tag_err!(format!("DefineSceneAndFrameLabelData({fields})"));
    assert_err!(
        err(r#"scenes: [(frame: 1, name: "a"), (frame: 2, name: "b\0c")]"#),
        r#"scenes: frame #1: name: "b\0c" contains a null character"#
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
}

#[test]
fn define_bits() {
    // The body is the character ID and the JPEG data, which is written as is.
    assert_eq!(
        compile_tags!(r#"DefineBits(id: 0x1234, data: InlineBytes("ff d8"))"#),
        [
            0x84, 0x01, // header: code 6, length 4
            0x34, 0x12, // ID
            0xFF, 0xD8, // data
        ]
    );
    assert_eq!(
        compile_tags!(r#"DefineBits(id: 1, data: InlineBytes(""))"#),
        [0x82, 0x01, 0x01, 0x00]
    );
}

#[test]
fn define_bits_jpeg_2() {
    // The body is the character ID and the JPEG data, which is written as is.
    assert_eq!(
        compile_tags!(r#"DefineBitsJpeg2(id: 0x1234, data: InlineBytes("ff d8"))"#),
        [
            0x44, 0x05, // header: code 21, length 4
            0x34, 0x12, // ID
            0xFF, 0xD8, // data
        ]
    );
    assert_eq!(
        compile_tags!(r#"DefineBitsJpeg2(id: 1, data: InlineBytes(""))"#),
        [0x42, 0x05, 0x01, 0x00]
    );
}

#[test]
fn jpeg_tables() {
    // The body is the tables, written as is.
    assert_eq!(
        compile_tags!(r#"JpegTables(InlineBytes("ff d8 ff d9"))"#),
        [
            0x04, 0x02, // header: code 8, length 4
            0xFF, 0xD8, 0xFF, 0xD9, // tables
        ]
    );
    // The tag is written even with no tables at all.
    assert_eq!(
        compile_tags!(r#"JpegTables(InlineBytes(""))"#),
        [0x00, 0x02]
    );
}

#[test]
fn protect() {
    // Without a password, the body is empty.
    assert_eq!(compile_tags!("Protect()"), [0x00, 0x06]);
    // With one, it is two reserved bytes and the null-terminated hash.
    assert_eq!(
        compile_tags!(r#"Protect(password: "ab")"#),
        [
            0x05, 0x06, // header: code 24, length 5
            0x00, 0x00, // reserved
            b'a', b'b', 0x00, // password
        ]
    );
    assert_eq!(
        compile_tags!(r#"Protect(password: "")"#),
        [0x03, 0x06, 0x00, 0x00, 0x00]
    );
}

#[test]
fn name_character() {
    // The body is the character ID and the null-terminated name.
    assert_eq!(
        compile_tags!(r#"NameCharacter(character: 0x1234, name: "ab")"#),
        [
            0x05, 0x0A, // header: code 40, length 5
            0x34, 0x12, // character
            b'a', b'b', 0x00, // name
        ]
    );
    // The name can be empty.
    assert_eq!(
        compile_tags!(r#"NameCharacter(character: 0xFFFF, name: "")"#),
        [0x03, 0x0A, 0xFF, 0xFF, 0x00]
    );
}

#[test]
fn product_info() {
    assert_eq!(
        compile_tags!(
            "ProductInfo(
                product_id: 0x01020304,
                edition: 0x05060708,
                major_version: 0x09,
                minor_version: 0x0A,
                build_number: 0x0B0C0D0E0F101112,
                compilation_date: 0x131415161718191A,
            )"
        ),
        [
            0x5A, 0x0A, // header: code 41, length 26
            0x04, 0x03, 0x02, 0x01, // product ID
            0x08, 0x07, 0x06, 0x05, // edition
            0x09, // major version
            0x0A, // minor version
            0x12, 0x11, 0x10, 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, // build number
            0x1A, 0x19, 0x18, 0x17, 0x16, 0x15, 0x14, 0x13, // compilation date
        ]
    );
}

#[test]
fn frame_label() {
    // The body is the null-terminated label.
    assert_eq!(
        compile_tags!(r#"FrameLabel("start")"#),
        [
            0xC6, 0x0A, // header
            0x73, 0x74, 0x61, 0x72, 0x74, 0x00, // label
        ]
    );
    assert_eq!(
        compile_tags!(r#"FrameLabel(label: "start")"#),
        compile_tags!(r#"FrameLabel("start")"#)
    );
    // The anchor flag is a byte of 1 after the label.
    assert_eq!(
        compile_tags!(r#"FrameLabel(label: "start", anchor: true)"#),
        [
            0xC7, 0x0A, // header
            0x73, 0x74, 0x61, 0x72, 0x74, 0x00, // label
            0x01, // anchor flag
        ]
    );
    assert_eq!(
        compile_tags!(r#"FrameLabel(label: "start", anchor: false)"#),
        compile_tags!(r#"FrameLabel("start")"#)
    );
    // The anchor flag is written from SWF 6.
    assert_eq!(
        compile_tags!(r#"FrameLabel(label: "a", anchor: true)"#, version: 6),
        [
            0xC3, 0x0A, // header
            0x61, 0x00, // label
            0x01, // anchor flag
        ]
    );
    assert_eq!(
        compile_tags!(r#"FrameLabel(label: "a")"#, version: 5),
        [
            0xC2, 0x0A, // header
            0x61, 0x00, // label
        ]
    );
    // Labels can be empty.
    assert_eq!(
        compile_tags!(r#"FrameLabel(label: "")"#),
        [
            0xC1, 0x0A, // header
            0x00, // label
        ]
    );
    assert_eq!(
        compile_tags!(r#"FrameLabel(label: "", anchor: true)"#),
        [
            0xC2, 0x0A, // header
            0x00, // label
            0x01, // anchor flag
        ]
    );
    // A frame can have several labels, and labels can repeat.
    assert_eq!(
        compile_tags!(
            r#"
                FrameLabel(label: "a"),
                FrameLabel(label: "a"),
                ShowFrame,
                FrameLabel(label: "a", anchor: true),
            "#
        ),
        [
            0xC2, 0x0A, 0x61, 0x00, // FrameLabel
            0xC2, 0x0A, 0x61, 0x00, // FrameLabel
            0x40, 0x00, // ShowFrame
            0xC3, 0x0A, 0x61, 0x00, 0x01, // FrameLabel
        ]
    );
}

#[test]
fn frame_label_errors() {
    assert_err!(
        tag_err!(r#"FrameLabel(label: "a", anchor: true)"#, version: 5),
        "tag #1: anchor: named anchors are not supported before SWF 6"
    );
    // The version of the SWF applies in sprites too.
    assert_err!(
        tag_err!(
            r#"DefineSprite(id: 1, tags: [FrameLabel(label: "a", anchor: true)])"#,
            version: 5
        ),
        "tag #1: tag #0: anchor: named anchors are not supported before SWF 6"
    );
    assert_err!(tag_err!("FrameLabel(1)"), "Expected a label or a struct");
    assert_err!(tag_err!("FrameLabel()"), "Expected a label or a struct");
}

#[test]
fn export_assets() {
    // The body is the count, and the character ID and the null-terminated name
    // of each asset.
    assert_eq!(
        compile_tags!(r#"ExportAssets([(character: 0x1234, name: "ab")])"#),
        [
            0x07, 0x0E, // header
            0x01, 0x00, // count
            0x34, 0x12, b'a', b'b', 0x00, // asset #0
        ]
    );
    // Assets are written as is, even when they are repeated, the characters are
    // not defined, or the names are empty.
    assert_eq!(
        compile_tags!(
            r#"ExportAssets([
                (character: 1, name: "a"),
                (character: 1, name: "a"),
                (character: 2, name: ""),
            ])"#
        ),
        [
            0x0D, 0x0E, // header
            0x03, 0x00, // count
            0x01, 0x00, b'a', 0x00, // asset #0
            0x01, 0x00, b'a', 0x00, // asset #1
            0x02, 0x00, 0x00, // asset #2
        ]
    );
    assert_eq!(
        compile_tags!("ExportAssets([])"),
        [
            0x02, 0x0E, // header
            0x00, 0x00, // count
        ]
    );
}

#[test]
fn export_assets_max_count() {
    // The most assets that the count can hold.
    assert_eq!(
        compile_tags!(&format!(
            "ExportAssets([{}])",
            r#"(character: 1, name: ""),"#.repeat(0xFFFF)
        )),
        [
            [
                0x3F, 0x0E, 0xFF, 0xFF, 0x02, 0x00, // long header
                0xFF, 0xFF, // count
            ]
            .as_slice(),
            &[0x01, 0x00, 0x00].repeat(0xFFFF), // assets
        ]
        .concat()
    );
}

#[test]
fn export_assets_errors() {
    assert_err!(
        tag_err!(r#"ExportAssets([(character: 1, name: "a"), (character: 2, name: "b\0c")])"#),
        r#"tag #1: asset #1: name: "b\0c" contains a null character"#
    );
    assert_err!(
        tag_err!(&format!(
            "ExportAssets([{}])",
            r#"(character: 1, name: ""),"#.repeat(0x10000)
        )),
        "tag #1: too many assets: 65536, at most 65535"
    );
}

#[test]
fn do_init_action() {
    assert_eq!(
        compile_tags!(r#"DoInitAction(sprite: 0x1234, actions: InlineBytes("07 00"))"#),
        [
            0xC4, 0x0E, // code 59, length 4
            0x34, 0x12, // sprite
            0x07, 0x00, // actions
        ]
    );
    assert_eq!(
        compile_tags!(r#"DoInitAction(sprite: 1, actions: InlineBytes(""))"#),
        [0xC2, 0x0E, 0x01, 0x00]
    );
    // Bytes are written as is: no End action is appended, and the bytecode is
    // not validated.
    assert_eq!(
        compile_tags!(r#"DoInitAction(sprite: 1, actions: InlineBytes("ff ff"))"#),
        [0xC4, 0x0E, 0x01, 0x00, 0xFF, 0xFF]
    );
    // The sprite doesn't have to be defined, and any ID is written.
    assert_eq!(
        compile_tags!(r#"DoInitAction(sprite: 0, actions: InlineBytes("00"))"#),
        [0xC3, 0x0E, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        compile_tags!(r#"DoInitAction(sprite: 0xFFFF, actions: InlineBytes("00"))"#),
        [0xC3, 0x0E, 0xFF, 0xFF, 0x00]
    );
    // The tag is not rejected in sprites.
    assert_eq!(
        compile_tags!(
            r#"DefineSprite(id: 1, tags: [DoInitAction(sprite: 1, actions: InlineBytes("00"))])"#
        ),
        [
            0xC9, 0x09, // DefineSprite, length 9
            0x01, 0x00, // id
            0x00, 0x00, // num_frames
            0xC3, 0x0E, // DoInitAction, length 3
            0x01, 0x00, // sprite
            0x00, // actions
        ]
    );
}

#[test]
fn define_video_stream() {
    // The body is the character ID, the frame count, the width, the height,
    // the flags and the codec.
    assert_eq!(
        compile_tags!(
            "DefineVideoStream(
                id: 0x1234,
                num_frames: 0x5678,
                width: 0x9ABC,
                height: 0xDEF0,
                codec: H263,
            )"
        ),
        [
            0x0A, 0x0F, // header: code 60, length 10
            0x34, 0x12, // ID
            0x78, 0x56, // frame count
            0xBC, 0x9A, // width
            0xF0, 0xDE, // height
            0x00, // flags: no smoothing, the deblocking of the video packets
            0x02, // codec: H263
        ]
    );
    // The frame count doesn't have to match the number of `VideoFrame` tags, of
    // which there are none here.
    assert_eq!(
        compile_tags!(
            "DefineVideoStream(
                id: 1,
                num_frames: 0xFFFF,
                width: 0,
                height: 0,
                smoothing: true,
                deblocking: Level4,
                codec: H264,
            )"
        ),
        [
            0x0A, 0x0F, // header: code 60, length 10
            0x01, 0x00, // ID
            0xFF, 0xFF, // frame count
            0x00, 0x00, // width
            0x00, 0x00, // height
            0x0B, // flags: smoothing, deblocking level 4
            0x07, // codec: H264
        ]
    );
    // The tag is written in SWFs older than SWF 6, which introduced it.
    assert_eq!(
        compile_tags!(
            "DefineVideoStream(id: 1, num_frames: 1, width: 320, height: 240, codec: H263)",
            version: 1
        ),
        [
            0x0A, 0x0F, // header: code 60, length 10
            0x01, 0x00, // ID
            0x01, 0x00, // frame count
            0x40, 0x01, // width: 320
            0xF0, 0x00, // height: 240
            0x00, // flags
            0x02, // codec: H263
        ]
    );
}

#[test]
fn define_video_stream_flags() {
    // Smoothing is bit 0 of the flags and deblocking is bits 1 to 3; the other
    // 4 bits are reserved and written as 0.
    for (deblocking, value) in [
        ("UseVideoPacketValue", 0),
        ("Off", 1),
        ("Level1", 2),
        ("Level2", 3),
        ("Level3", 4),
        ("Level4", 5),
    ] {
        for (smoothing, bit) in [("false", 0), ("true", 1)] {
            let flags = value << 1 | bit;
            assert_eq!(
                compile_tags!(format!(
                    "DefineVideoStream(
                        id: 1,
                        num_frames: 0,
                        width: 0,
                        height: 0,
                        smoothing: {smoothing},
                        deblocking: {deblocking},
                        codec: H263,
                    )"
                )),
                [
                    0x0A, 0x0F, // header: code 60, length 10
                    0x01, 0x00, // ID
                    0x00, 0x00, // frame count
                    0x00, 0x00, // width
                    0x00, 0x00,  // height
                    flags, // flags
                    0x02,  // codec: H263
                ],
                "deblocking: {deblocking}, smoothing: {smoothing}"
            );
        }
    }
}

#[test]
fn define_video_stream_codecs() {
    for (codec, id) in [
        ("None", 0),
        ("H263", 2),
        ("ScreenVideo", 3),
        ("Vp6", 4),
        ("Vp6WithAlpha", 5),
        ("ScreenVideoV2", 6),
        ("H264", 7),
    ] {
        assert_eq!(
            compile_tags!(format!(
                "DefineVideoStream(
                    id: 1,
                    num_frames: 0,
                    width: 0,
                    height: 0,
                    codec: {codec},
                )"
            )),
            [
                0x0A, 0x0F, // header: code 60, length 10
                0x01, 0x00, // ID
                0x00, 0x00, // frame count
                0x00, 0x00, // width
                0x00, 0x00, // height
                0x00, // flags
                id,   // codec
            ],
            "codec: {codec}"
        );
    }
}

#[test]
fn video_frame() {
    // The body is the character ID of the stream, the number of the frame and
    // the data of the frame.
    assert_eq!(
        compile_tags!(r#"VideoFrame(stream: 0x1234, frame: 0x5678, data: InlineBytes("11 22"))"#),
        [
            0x46, 0x0F, // header: code 61, length 6
            0x34, 0x12, // stream
            0x78, 0x56, // frame number
            0x11, 0x22, // data
        ]
    );
    // The data is not checked against the codec of the stream.
    assert_eq!(
        compile_tags!(r#"VideoFrame(stream: 1, frame: 0, data: InlineBytes(""))"#),
        [
            0x44, 0x0F, // header: code 61, length 4
            0x01, 0x00, // stream
            0x00, 0x00, // frame number
        ]
    );
    assert_eq!(
        compile_tags!(r#"VideoFrame(stream: 1, frame: 0, data: InlineBytes("ff ff"))"#),
        [0x46, 0x0F, 0x01, 0x00, 0x00, 0x00, 0xFF, 0xFF]
    );
    // The stream doesn't have to be defined, and frames can repeat or come out
    // of order.
    assert_eq!(
        compile_tags!(
            r#"VideoFrame(stream: 0, frame: 1, data: InlineBytes("00")),
               VideoFrame(stream: 0xFFFF, frame: 1, data: InlineBytes("00"))"#
        ),
        [
            0x45, 0x0F, 0x00, 0x00, 0x01, 0x00, 0x00, // frame #0
            0x45, 0x0F, 0xFF, 0xFF, 0x01, 0x00, 0x00, // frame #1
        ]
    );
}

#[test]
fn debug_id() {
    assert_eq!(
        compile_tags!(r#"DebugId(InlineBytes("000102030405060708090a0b0c0d0e0f"))"#),
        [
            [0xD0, 0x0F].as_slice(), // header: code 63, length 16
            &[
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D,
                0x0E, 0x0F,
            ],
        ]
        .concat()
    );
}

#[test]
fn debug_id_errors() {
    assert_err!(
        tag_err!(r#"DebugId(InlineBytes("00"))"#),
        "expected 16 bytes, got 1"
    );
    assert_err!(
        tag_err!(r#"DebugId(InlineBytes("000102030405060708090a0b0c0d0e0f10"))"#),
        "expected 16 bytes, got 17"
    );
    assert_err!(
        tag_err!(r#"DebugId(InlineBytes(""))"#),
        "expected 16 bytes, got 0"
    );
}

#[test]
fn set_tab_index() {
    assert_eq!(
        compile_tags!("SetTabIndex(depth: 0x1234, tab_index: 0x5678)"),
        [
            0x84, 0x10, // header: code 66, length 4
            0x34, 0x12, // depth
            0x78, 0x56, // tab index
        ]
    );
    // The depth doesn't have to hold a character.
    assert_eq!(
        compile_tags!("SetTabIndex(depth: 0, tab_index: 0xFFFF)"),
        [0x84, 0x10, 0x00, 0x00, 0xFF, 0xFF]
    );
}

#[test]
fn script_limits() {
    assert_eq!(
        compile_tags!("ScriptLimits(max_recursion_depth: 0x1234, timeout_seconds: 0x5678)"),
        [
            0x44, 0x10, // header: code 65, length 4
            0x34, 0x12, // max recursion depth
            0x78, 0x56, // timeout seconds
        ]
    );
    // The tag is written in SWFs older than SWF 7, which introduced it.
    assert_eq!(
        compile_tags!(
            "ScriptLimits(max_recursion_depth: 0, timeout_seconds: 65535)",
            version: 1
        ),
        [
            0x44, 0x10, // header: code 65, length 4
            0x00, 0x00, // max recursion depth: 0
            0xFF, 0xFF, // timeout seconds: 65535
        ]
    );
}

#[test]
fn do_abc() {
    // The body is the ABC data, written as is.
    assert_eq!(
        compile_tags!(r#"DoAbc(InlineBytes("10 00 2e 00"))"#),
        [
            0x04, 0x12, // header
            0x10, 0x00, 0x2E, 0x00, // data
        ]
    );
    // The data is not validated.
    assert_eq!(
        compile_tags!(r#"DoAbc(InlineBytes("ff"))"#),
        [
            0x01, 0x12, // header
            0xFF, // data
        ]
    );
    assert_eq!(
        compile_tags!(r#"DoAbc(InlineBytes(""))"#),
        [
            0x00, 0x12, // header
        ]
    );
}

#[test]
fn csm_text_settings() {
    // The body is the character ID, the flags, the thickness, the sharpness
    // and a reserved byte.
    assert_eq!(
        compile_tags!(
            "CsmTextSettings(
                text: 0x1234,
                advanced_rendering: true,
                grid_fit: Pixel,
                thickness: 1.0,
                sharpness: -2.5,
            )"
        ),
        [
            0x8C, 0x12, // header: code 74, length 12
            0x34, 0x12, // text
            0x48, // advanced rendering, grid fit 1
            0x00, 0x00, 0x80, 0x3F, // thickness: 1.0
            0x00, 0x00, 0x20, 0xC0, // sharpness: -2.5
            0x00, // reserved
        ]
    );
    assert_eq!(
        compile_tags!("CsmTextSettings(text: 1)"),
        [
            0x8C, 0x12, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]
    );
}

#[test]
fn csm_text_settings_grid_fit() {
    // Grid fitting is bits 3 to 5 of the flags.
    for (grid_fit, flags) in [("None", 0x00), ("Pixel", 0x08), ("SubPixel", 0x10)] {
        assert_eq!(
            compile_tags!(format!("CsmTextSettings(text: 1, grid_fit: {grid_fit})")),
            [
                0x8C, 0x12, 0x01, 0x00, flags, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00,
            ],
            "grid_fit: {grid_fit}"
        );
    }
}

#[test]
fn symbol_class() {
    // The body is the count, and the character ID and the null-terminated class
    // name of each link.
    assert_eq!(
        compile_tags!(r#"SymbolClass([(character: 0x1234, name: "ab")])"#),
        [
            0x07, 0x13, // header
            0x01, 0x00, // count
            0x34, 0x12, b'a', b'b', 0x00, // link #0
        ]
    );
    // Links are written as is, even when they are repeated, the characters are
    // not defined, or the names are empty.
    assert_eq!(
        compile_tags!(
            r#"SymbolClass([
                (character: 0, name: "a.B"),
                (character: 0, name: "a.B"),
                (character: 0xFFFF, name: ""),
            ])"#
        ),
        [
            0x11, 0x13, // header
            0x03, 0x00, // count
            0x00, 0x00, b'a', b'.', b'B', 0x00, // link #0
            0x00, 0x00, b'a', b'.', b'B', 0x00, // link #1
            0xFF, 0xFF, 0x00, // link #2
        ]
    );
    assert_eq!(
        compile_tags!("SymbolClass([])"),
        [
            0x02, 0x13, // header
            0x00, 0x00, // count
        ]
    );
}

#[test]
fn symbol_class_max_count() {
    // The most links that the count can hold.
    assert_eq!(
        compile_tags!(&format!(
            "SymbolClass([{}])",
            r#"(character: 1, name: ""),"#.repeat(0xFFFF)
        )),
        [
            [
                0x3F, 0x13, 0xFF, 0xFF, 0x02, 0x00, // long header
                0xFF, 0xFF, // count
            ]
            .as_slice(),
            &[0x01, 0x00, 0x00].repeat(0xFFFF), // links
        ]
        .concat()
    );
}

#[test]
fn symbol_class_errors() {
    assert_err!(
        tag_err!(r#"SymbolClass([(character: 1, name: "a"), (character: 2, name: "b\0c")])"#),
        r#"tag #1: link #1: name: "b\0c" contains a null character"#
    );
    assert_err!(
        tag_err!(&format!(
            "SymbolClass([{}])",
            r#"(character: 1, name: ""),"#.repeat(0x10000)
        )),
        "tag #1: too many links: 65536, at most 65535"
    );
}

#[test]
fn metadata() {
    assert_eq!(
        compile_tags!(r#"Metadata(InlineBytes("3c 61 2f 3e"))"#),
        [
            0x45, 0x13, // header: code 77, length 5
            b'<', b'a', b'/', b'>', // metadata
            0x00, // null terminator
        ]
    );
    // Bytes that aren't valid UTF-8, several tags, an old SWF, and no
    // `has_metadata` flag.
    assert_eq!(
        compile_tags!(
            r#"FileAttributes(), Metadata(InlineBytes("")), Metadata(InlineBytes("ff fe"))"#,
            version: 1
        ),
        [
            0x44, 0x11, // header: code 69, length 4
            0x00, 0x00, 0x00, 0x00, // flags
            0x41, 0x13, // header: code 77, length 1
            0x00, // null terminator
            0x43, 0x13, // header: code 77, length 3
            0xFF, 0xFE, // metadata
            0x00, // null terminator
        ]
    );
}

#[test]
fn metadata_errors() {
    // The metadata is null-terminated, so it can't contain null bytes.
    assert_err!(
        tag_err!(r#"Metadata(InlineBytes("61 00 62"))"#),
        "tag #1: null byte at offset 1, which would end the string early"
    );
}

#[test]
fn define_scaling_grid() {
    // The body is the character ID and the rectangle, in twips.
    assert_eq!(
        compile_tags!(
            "DefineScalingGrid(character: 0x1234, splitter: (x: 10, y: 10, width: 80, height: 20))"
        ),
        [
            0x89, 0x13, // header: code 78, length 9
            0x34, 0x12, // character
            0x60, 0x64, 0x38, 0x40, 0x64, 0x12, 0xC0, // 200..1800, 200..600 twips
        ]
    );
    // x and y default to 0.
    assert_eq!(
        compile_tags!("DefineScalingGrid(character: 1, splitter: (width: 0, height: 0))"),
        [
            0x83, 0x13, // header: code 78, length 3
            0x01, 0x00, // character
            0x00, // an empty rectangle: no bits per edge
        ]
    );
}

#[test]
fn define_scaling_grid_errors() {
    let err = |fields: &str| tag_err!(format!("DefineScalingGrid({fields})"));
    // The edges are whole numbers of twips, within 31 signed bits.
    assert_err!(
        err("character: 1, splitter: (x: 0.01, width: 1, height: 1)"),
        "splitter: x: 0.01px is not a whole number of twips"
    );
    assert_err!(
        err("character: 1, splitter: (width: 53687091.2, height: 1)"),
        "splitter: width: 53687091.2px is out of range"
    );
}

#[test]
fn do_abc_2() {
    // The body is the flags, the null-terminated name and the ABC data.
    assert_eq!(
        compile_tags!(r#"DoAbc2(name: "ab", data: InlineBytes("10 00"))"#),
        [
            0x89, 0x14, // header
            0x00, 0x00, 0x00, 0x00, // flags
            b'a', b'b', 0x00, // name
            0x10, 0x00, // data
        ]
    );
    // The flags default to 0, and the name to an empty one.
    assert_eq!(
        compile_tags!(r#"DoAbc2(data: InlineBytes(""))"#),
        [
            0x85, 0x14, // header
            0x00, 0x00, 0x00, 0x00, // flags
            0x00, // name
        ]
    );
    // The data is not validated.
    assert_eq!(
        compile_tags!(r#"DoAbc2(lazy_initialize: true, data: InlineBytes("ff"))"#),
        [
            0x86, 0x14, // header
            0x01, 0x00, 0x00, 0x00, // flags
            0x00, // name
            0xFF, // data
        ]
    );
}

#[test]
fn define_font_name() {
    // The body is the character ID of the font, the null-terminated name and
    // the null-terminated copyright notice.
    assert_eq!(
        compile_tags!(r#"DefineFontName(font: 0x1234, name: "ab", copyright: "c")"#),
        [
            0x07, 0x16, // header: code 88, length 7
            0x34, 0x12, // font
            b'a', b'b', 0x00, // name
            b'c', 0x00, // copyright
        ]
    );
    // The copyright notice defaults to an empty string, which is still
    // written.
    assert_eq!(
        compile_tags!(r#"DefineFontName(font: 1, name: "a")"#),
        [0x05, 0x16, 0x01, 0x00, b'a', 0x00, 0x00]
    );
}

#[test]
fn rascal_syntax() {
    let parse = |data: &str| -> Result<Data, String> {
        let swf = Compiler::new()
            .parse(&format!("Swf(version: 10, tags: [DoAction({data})])"))
            .map_err(|e| format!("{e:#}"))?;
        match swf.tags.into_iter().next() {
            Some(Tag::DoAction(data)) => Ok(data),
            tag => panic!("unexpected tag {tag:?}"),
        }
    };
    let rascal = |script| Ok(Data::Rascal(script));
    let inline = |text: &str| Rascal {
        source: ScriptSource::Inline(text.to_owned()),
        swf_version: None,
        fold_constants: true,
        promote_variables_to_registers: true,
    };
    let file = |path: &str| Rascal {
        source: ScriptSource::File(path.to_owned()),
        swf_version: None,
        fold_constants: true,
        promote_variables_to_registers: true,
    };

    assert_eq!(parse(r#"Rascal("trace(1);")"#), rascal(inline("trace(1);")));
    assert_eq!(
        parse(r##"Rascal(r#"trace("a");"#)"##),
        rascal(inline(r#"trace("a");"#))
    );
    assert_eq!(
        parse(r#"Rascal(source: "trace(1);")"#),
        rascal(inline("trace(1);"))
    );
    // The struct can also be written in its own parentheses.
    assert_eq!(
        parse(r#"Rascal((source: "trace(1);"))"#),
        rascal(inline("trace(1);"))
    );
    assert_eq!(parse(r#"Rascal(file: "a.as")"#), rascal(file("a.as")));
    assert_eq!(
        parse(r#"Rascal(source: "trace(1);", swf_version: 6)"#),
        rascal(Rascal {
            swf_version: Some(6),
            ..inline("trace(1);")
        })
    );
    assert_eq!(
        parse(r#"Rascal(swf_version: 6, file: "a.as")"#),
        rascal(Rascal {
            swf_version: Some(6),
            ..file("a.as")
        })
    );
    assert_eq!(
        parse(r#"Rascal(file: "a.as", fold_constants: false)"#),
        rascal(Rascal {
            fold_constants: false,
            ..file("a.as")
        })
    );
    assert_eq!(
        parse(r#"Rascal(file: "a.as", fold_constants: true)"#),
        rascal(file("a.as"))
    );
    assert_eq!(
        parse(r#"Rascal(file: "a.as", promote_variables_to_registers: false)"#),
        rascal(Rascal {
            promote_variables_to_registers: false,
            ..file("a.as")
        })
    );

    let parse_err = |data| parse(data).unwrap_err();
    assert_err!(
        parse_err("Rascal(source: \"a\", file: \"b\")"),
        "found both"
    );
    assert_err!(
        parse_err("Rascal(source: None)"),
        "expected `source` or `file`"
    );
    assert_err!(
        parse_err("Rascal(swf_version: 6)"),
        "expected `source` or `file`"
    );
    assert_err!(
        parse_err("Rascal(source: \"\", swf_version: 256)"),
        "Integer is out of bounds"
    );
    assert_err!(parse_err("Rascal(1)"), "Expected a script or a struct");
    assert_err!(parse_err("Rascal()"), "Expected a script or a struct");
    assert_err!(
        parse_err("Rascal(script: \"a\")"),
        "Unexpected field named `script` in `Rascal`, expected one of `source`, `file`, \
        `swf_version`, `fold_constants`, or `promote_variables_to_registers`"
    );
}

/// Provides the given files, with their contents as text.
#[cfg(any(feature = "rascal", feature = "asc"))]
struct TextFiles(&'static [(&'static str, &'static str)]);

#[cfg(any(feature = "rascal", feature = "asc"))]
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

#[cfg(feature = "rascal")]
mod rascal {
    use super::*;

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
        // Init actions are the same bytecode as frame actions.
        assert_eq!(
            compile_tags!(r##"DoInitAction(sprite: 1, actions: Rascal(r#"trace("hello");"#))"##),
            [
                [0xD4, 0x0E].as_slice(),         // code 59, length 20
                &[0x01, 0x00],                   // sprite
                &[0x88, 0x08, 0x00, 0x01, 0x00], // ConstantPool with
                b"hello\0",                      // "hello"
                &[0x96, 0x02, 0x00, 0x08, 0x00], // Push the constant
                &[0x26],                         // Trace
                &[0x00],                         // End
            ]
            .concat()
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
        let compile = |version, options| {
            compile_str(
                &format!(
                    r#"Swf(version: {version}, tags: [
                        DoAction(Rascal(source: "function f(a) {{ trace(a); }}"{options})),
                    ])"#
                ),
                &NoFiles,
            )
            .unwrap()
        };
        let swf6 = compile(6, "");
        let swf7 = compile(7, "");
        assert!(swf6.contains(&0x9B) && !swf6.contains(&0x8E));
        assert!(swf7.contains(&0x8E) && !swf7.contains(&0x9B));
        // The version can be overridden, which changes only the bytecode.
        let swf6_as_7 = compile(6, ", swf_version: 7");
        let swf7_as_6 = compile(7, ", swf_version: 6");
        assert!(swf6_as_7.contains(&0x8E) && !swf6_as_7.contains(&0x9B));
        assert!(swf7_as_6.contains(&0x9B) && !swf7_as_6.contains(&0x8E));
        assert_eq!(swf6_as_7[3], 6);
        assert_eq!(swf7_as_6[3], 7);
    }

    #[test]
    fn fold_constants() {
        // An empty constant pool, a push of the integer 3 and Trace.
        assert_eq!(
            compile_tags!(r#"DoAction(Rascal("trace(1 + 2);"))"#),
            [
                0x0F, 0x03, 0x88, 0x02, 0x00, 0x00, 0x00, 0x96, 0x05, 0x00, 0x07, 0x03, 0x00, 0x00,
                0x00, 0x26, 0x00
            ]
        );
        // A push of the integers 1 and 2, Add2 and Trace.
        assert_eq!(
            compile_tags!(r#"DoAction(Rascal(source: "trace(1 + 2);", fold_constants: false))"#),
            [
                0x15, 0x03, 0x88, 0x02, 0x00, 0x00, 0x00, 0x96, 0x0A, 0x00, 0x07, 0x01, 0x00, 0x00,
                0x00, 0x07, 0x02, 0x00, 0x00, 0x00, 0x47, 0x26, 0x00
            ]
        );
    }

    #[test]
    fn promote_variables_to_registers() {
        // A local variable is stored with StoreRegister (0x87) when it is
        // promoted to a register, and with DefineLocal (0x3C) otherwise.
        let promoted =
            compile_tags!(r#"DoAction(Rascal("function f() { var x = 1; trace(x); }"))"#);
        let not_promoted = compile_tags!(
            r#"DoAction(Rascal(
                source: "function f() { var x = 1; trace(x); }",
                promote_variables_to_registers: false,
            ))"#
        );
        assert!(promoted.contains(&0x87) && !promoted.contains(&0x3C));
        assert!(not_promoted.contains(&0x3C) && !not_promoted.contains(&0x87));

        // Before SWF 7, there are no registers to promote to.
        let compile = |options| {
            compile_str(
                &format!(
                    r#"Swf(version: 6, tags: [
                        DoAction(Rascal(source: "function f() {{ var x = 1; trace(x); }}"{options})),
                    ])"#
                ),
                &NoFiles,
            )
            .unwrap()
        };
        assert_eq!(
            compile(""),
            compile(", promote_variables_to_registers: false")
        );
    }

    #[test]
    fn includes() {
        let files = TextFiles(&[("a/b.as", r#"trace("hello");"#)]);
        assert_eq!(
            compile_tags!(r##"DoAction(Rascal(r#"#include "a/b.as""#))"##, &files),
            compile_tags!(r##"DoAction(Rascal(r#"trace("hello");"#))"##)
        );
        // Errors of the provider are kept.
        assert_err!(
            compile_err!(
                r##"Swf(version: 10, tags: [DoAction(Rascal(r#"#include "c.as""#))])"##,
                &files
            ),
            "Could not load file 'c.as': no file named c.as"
        );
        assert_err!(
            tag_err!(r##"DoAction(Rascal(r#"#include "c.as""#))"##),
            "Could not load file 'c.as': files are not available"
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
        assert_err!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAction(Rascal(file: "d.as"))])"#,
                &files
            ),
            "failed to compile the ActionScript: error: Compile error\n  ╭▸ d.as:1:1"
        );
        assert_err!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAction(Rascal(file: "e.as"))])"#,
                &files
            ),
            "failed to compile the ActionScript: error: no file named e.as\n ─▸ e.as"
        );
    }

    #[test]
    fn errors() {
        assert_err!(
            tag_err!(r#"DoAction(Rascal("trace("))"#),
            "tag #1: failed to compile the ActionScript: error: Compile error"
        );
        assert_err!(
            tag_err!(r#"DoAction(Rascal("trace(\n1 2)"))"#),
            "╭▸ <inline>:2:3"
        );
        // Classes are loaded from files, and compiled into separate tags.
        let files = TextFiles(&[("Foo.as", "class Foo {}"), ("a/Bar.as", "class a.Bar {}")]);
        assert_err!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAction(Rascal("var foo = new Foo();"))])"#,
                &files
            ),
            "classes and interfaces are not supported"
        );
        assert_err!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAction(Rascal("import a.Bar; var bar: Bar;"))])"#,
                &files
            ),
            "classes and interfaces are not supported"
        );
        assert_err!(
            tag_err!(r#"DoAction(Rascal("class Foo {}"))"#),
            "Classes may only be defined in external ActionScript 2.0 class scripts"
        );
    }
}

#[cfg(not(feature = "rascal"))]
#[test]
fn rascal_not_available() {
    for data in [r#"Rascal("")"#, r#"Rascal(file: "a.as")"#] {
        let source = format!("Swf(version: 10, tags: [DoAction({data})])");
        // Sources that use Rascal are parsed, but fail to compile.
        Compiler::new().parse(&source).unwrap();
        assert_err!(compile_err!(source), "Rascal is not available");
    }
}

#[test]
fn asc_syntax() {
    let parse = |data: &str| -> Result<Data, String> {
        let swf = Compiler::new()
            .parse(&format!("Swf(version: 10, tags: [DoAbc({data})])"))
            .map_err(|e| format!("{e:#}"))?;
        match swf.tags.into_iter().next() {
            Some(Tag::DoAbc(data)) => Ok(data),
            tag => panic!("unexpected tag {tag:?}"),
        }
    };
    let asc = |script| Ok(Data::Asc(script));
    let inline = |text: &str| Asc {
        source: ScriptSource::Inline(text.to_owned()),
        scripts: vec![],
        optimize: true,
        debug: false,
    };
    let file = |path: &str| Asc {
        source: ScriptSource::File(path.to_owned()),
        scripts: vec![],
        optimize: true,
        debug: false,
    };

    assert_eq!(parse(r#"Asc("trace(1);")"#), asc(inline("trace(1);")));
    assert_eq!(
        parse(r#"Asc(source: "trace(1);")"#),
        asc(inline("trace(1);"))
    );
    assert_eq!(parse(r#"Asc(file: "a.as")"#), asc(file("a.as")));
    assert_eq!(
        parse(r#"Asc(file: "a.as", scripts: ["b.as", "c/d.as"])"#),
        asc(Asc {
            scripts: vec!["b.as".to_owned(), "c/d.as".to_owned()],
            ..file("a.as")
        })
    );
    assert_eq!(
        parse(r#"Asc(source: "", optimize: false, debug: true)"#),
        asc(Asc {
            optimize: false,
            debug: true,
            ..inline("")
        })
    );
    assert_eq!(
        parse(r#"Asc(source: "", optimize: true, debug: false)"#),
        asc(inline(""))
    );

    let parse_err = |data| parse(data).unwrap_err();
    assert_err!(parse_err("Asc(source: \"a\", file: \"b\")"), "found both");
    assert_err!(
        parse_err("Asc(scripts: [\"a.as\"])"),
        "expected `source` or `file`"
    );
    assert_err!(parse_err("Asc(1)"), "Expected a script or a struct");
    assert_err!(
        parse_err("Asc(script: \"a\")"),
        "Unexpected field named `script` in `Asc`, expected one of `source`, `file`, \
        `scripts`, `optimize`, or `debug`"
    );
}

#[cfg(feature = "asc")]
mod asc {
    use super::*;

    #[test]
    fn script() {
        assert_eq!(
            compile_tags!(r#"DoAbc(Asc(""))"#),
            [
                [0x22, 0x12].as_slice(),               // code 72, length 34
                &[0x10, 0x00, 0x2E, 0x00],             // minor version 16, major version 46
                &[0x00, 0x00, 0x00],                   // no ints, uints and doubles
                &[0x02, 0x00],                         // the string ""
                &[0x00, 0x00, 0x00],                   // no namespaces, sets and multinames
                &[0x01, 0x00, 0x00, 0x01, 0x00],       // a method named ""
                &[0x00, 0x00],                         // no metadata and classes
                &[0x01, 0x00, 0x00],                   // a script with the method as initializer
                &[0x01, 0x00, 0x01, 0x01, 0x01, 0x02], // the body of the method
                &[0x03, 0xD0, 0x30, 0x47],             // GetLocal0, PushScope, ReturnVoid
                &[0x00, 0x00],                         // no exceptions and traits
            ]
            .concat()
        );
        let hello = compile_tags!(r##"DoAbc(Asc(r#"trace("hello");"#))"##);
        assert_eq!(
            compile_tags!(r##"DoAbc(Asc(source: r#"trace("hello");"#))"##),
            hello
        );
        // The ABC file can be used as any other data.
        assert_eq!(
            compile_tags!(r#"DefineBinaryData(id: 1, data: Asc(""))"#)[8..],
            compile_tags!(r#"DoAbc(Asc(""))"#)[2..]
        );
    }

    #[test]
    fn file() {
        // asc writes the name of the file into the ABC file, so the files have
        // the name of the inline script.
        let files = TextFiles(&[
            ("inline", r#"trace("hello");"#),
            ("a/inline", r#"trace("hello");"#),
            ("./a/inline", r#"trace("hello");"#),
            ("b.as", r#"trace("hello");"#),
        ]);
        let inline = compile_tags!(r##"DoAbc(Asc(r#"trace("hello");"#))"##);
        assert_eq!(
            compile_tags!(r#"DoAbc(Asc(file: "inline"))"#, &files),
            inline
        );
        assert_eq!(
            compile_tags!(r#"DoAbc(Asc(file: "a/inline"))"#, &files),
            inline
        );
        assert_eq!(
            compile_tags!(r#"DoAbc(Asc(file: "./a/inline"))"#, &files),
            inline
        );
        assert_ne!(compile_tags!(r#"DoAbc(Asc(file: "b.as"))"#, &files), inline);
        assert_err!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAbc(Asc(file: "d.as"))])"#,
                &files
            ),
            "failed to compile the ActionScript: failed to read file \"d.as\": no file named d.as"
        );
    }

    #[test]
    fn scripts() {
        let files = TextFiles(&[
            (
                "Main.as",
                "package { import pkg.Other; public class Main { new Other(); } }",
            ),
            ("pkg/Other.as", "package pkg { public class Other {} }"),
        ]);
        let main = compile_tags!(r#"DoAbc(Asc(file: "Main.as"))"#, &files);
        let with_other = compile_tags!(
            r#"DoAbc(Asc(file: "Main.as", scripts: ["pkg/Other.as"]))"#,
            &files
        );
        assert_ne!(main, with_other);
        // The main script is compiled once, even if it is listed too, with any
        // spelling of its path.
        for main in ["Main.as", "./Main.as"] {
            assert_eq!(
                compile_tags!(
                    format!(r#"DoAbc(Asc(file: "Main.as", scripts: ["{main}", "pkg/Other.as"]))"#),
                    &files
                ),
                with_other
            );
        }
        assert_err!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAbc(Asc(file: "Main.as", scripts: ["e.as"]))])"#,
                &files
            ),
            "failed to read file \"e.as\": no file named e.as"
        );
    }

    #[test]
    fn options() {
        let script = r#"source: "var a = 1; var b = a; trace(b);""#;
        let compile = |options: &str| compile_tags!(format!("DoAbc(Asc({script}{options}))"));
        let default = compile("");
        assert_eq!(compile(", optimize: true, debug: false"), default);
        // The optimizer of asc doesn't change small scripts like this one, so
        // this only checks that asc accepts the option.
        assert_eq!(compile(", optimize: false"), default);
        assert_ne!(compile(", debug: true"), default);
    }

    #[test]
    fn errors() {
        assert_err!(
            tag_err!(r#"DoAbc(Asc("trace("))"#),
            "tag #1: failed to compile the ActionScript: asc failed: [Compiler] Error #1084"
        );
        assert_err!(tag_err!(r#"DoAbc(Asc("trace("))"#), "inline, Ln 2, Col 1");
        // Files are copied into a temporary directory, so their paths must stay
        // inside it.
        for path in ["../a.as", "/a.as", "a/../../b.as"] {
            assert_err!(
                compile_err!(
                    format!(r#"Swf(version: 10, tags: [DoAbc(Asc(file: "{path}"))])"#),
                    &PathAsContents
                ),
                format!("the path \"{path}\" must be relative and must not contain `..`")
            );
        }
        assert_err!(
            compile_err!(
                r#"Swf(version: 10, tags: [DoAbc(Asc(file: "."))])"#,
                &PathAsContents
            ),
            "the path \".\" does not name a file"
        );
        for path in ["inline", "./inline"] {
            assert_err!(
                tag_err!(format!(r#"DoAbc(Asc(source: "", scripts: ["{path}"]))"#)),
                format!("\"{path}\" is the path of the inline script")
            );
        }
    }
}

#[cfg(not(feature = "asc"))]
#[test]
fn asc_not_available() {
    for data in [r#"Asc("")"#, r#"Asc(file: "a.as")"#] {
        let source = format!("Swf(version: 10, tags: [DoAbc({data})])");
        // Sources that use asc are parsed, but fail to compile.
        Compiler::new().parse(&source).unwrap();
        assert_err!(compile_err!(source), "asc is not available");
    }
}

use swf::{Compression, Fixed8, Rectangle, Twips};

// These are macros instead of functions, so that failures point to the test.

/// Compiles the given tags and returns the serialized tag stream.
macro_rules! compile_tags {
    ($tags:expr) => {{
        let compile = |tags: &str| {
            swfasm::compile_str(&format!("Swf(version: 10, num_frames: 0, tags: [{tags}])"))
        };
        let header_len = compile("").unwrap().len();
        compile(&$tags).unwrap()[header_len..].to_vec()
    }};
}

/// Compiles the given source, which must fail, and returns the error message.
macro_rules! compile_err {
    ($source:expr) => {
        format!("{:#}", swfasm::compile_str(&$source).unwrap_err())
    };
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
    let swf =
        swfasm::compile_str("Swf(version: 10, stage: (x: 0.05, y: 0.05, width: 0.1, height: 0.1))")
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
        let swf = swfasm::compile_str(&format!("Swf(version: 10, tags: [{tags}])")).unwrap();
        swf::decompress_swf(&swf[..]).unwrap().header.num_frames()
    };
    assert_eq!(num_frames(""), 0);
    assert_eq!(num_frames("ShowFrame, ShowFrame"), 2);
    assert_eq!(num_frames("FileAttributes(), ShowFrame, End"), 1);
    assert_eq!(num_frames(r#"Raw(code: 1, data: "")"#), 1);
    assert_eq!(num_frames(r#"Raw(code: 1, force_long: true, data: "")"#), 1);
    assert_eq!(num_frames(r#"Raw(code: 2, data: "")"#), 0);
    assert_eq!(num_frames(r#"ShowFrame, Raw(code: 1, data: "")"#), 2);
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
        let swf = swfasm::compile_str(&format!(
            "Swf(version: 10, num_frames: {num_frames}, tags: [{tags}])"
        ))
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
        compile_tags!(r#"Raw(code: 1023, data: "01 02 aB"), Raw(code: 2, data: "0304")"#),
        [0xC3, 0xFF, 0x01, 0x02, 0xAB, 0x82, 0x00, 0x03, 0x04]
    );
    assert_eq!(
        compile_tags!(
            r#"Raw(code: 2, data: [
                "05 06", // comment
                "",
                " 07 ",
            ])"#
        ),
        [0x83, 0x00, 0x05, 0x06, 0x07]
    );
    assert_eq!(compile_tags!(r#"Raw(code: 2, data: [])"#), [0x80, 0x00]);
    assert_eq!(
        compile_tags!(r#"Raw(code: 2, force_long: true, data: "aa")"#),
        [0xBF, 0x00, 0x01, 0x00, 0x00, 0x00, 0xAA]
    );
    // Bodies that do not fit in the short form use the long one anyway.
    assert_eq!(
        compile_tags!(&format!(r#"Raw(code: 2, data: "{}")"#, "00".repeat(63))),
        [[0xBF, 0x00, 63, 0x00, 0x00, 0x00].as_slice(), &[0; 63]].concat()
    );
}

#[test]
fn tag_code_errors() {
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1100, data: "")])"#)
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
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1, data: "", long: true)])"#)
            .contains("Unexpected field named `long` in `Raw`")
    );
}

#[test]
fn do_action_bytes() {
    assert_eq!(
        compile_tags!(r#"DoAction(Bytes("07 00"))"#),
        [0x02, 0x03, 0x07, 0x00]
    );
    assert_eq!(compile_tags!(r#"DoAction(Bytes(""))"#), [0x00, 0x03]);
    // Bytes are written as is: no End action is appended, and the bytecode is
    // not validated.
    assert_eq!(
        compile_tags!(r#"DoAction(Bytes("07"))"#),
        [0x01, 0x03, 0x07]
    );
    assert_eq!(
        compile_tags!(r#"DoAction(Bytes("ff ff"))"#),
        [0x02, 0x03, 0xFF, 0xFF]
    );
    assert_eq!(
        compile_tags!(r#"DoAction(Bytes(["07", "", "00"]))"#),
        [0x02, 0x03, 0x07, 0x00]
    );
    // The longest body that fits in the short header form.
    assert_eq!(
        compile_tags!(&format!(r#"DoAction(Bytes("{}"))"#, "00".repeat(62))),
        [[0x3E, 0x03].as_slice(), &[0; 62]].concat()
    );
    assert_eq!(
        compile_tags!(&format!(r#"DoAction(Bytes("{}"))"#, "00".repeat(63))),
        [[0x3F, 0x03, 63, 0x00, 0x00, 0x00].as_slice(), &[0; 63]].concat()
    );
    assert_eq!(
        compile_tags!(&format!(r#"DoAction(Bytes("{}"))"#, "00".repeat(0x1234))),
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
            .contains("Unexpected variant named `Text` in enum `DoActionContent`")
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [DoAction(Bytes("070"))])"#)
            .contains(r#"hex string "070" has an odd number of digits"#)
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
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1, data: "0")])"#)
            .contains(r#"hex string "0" has an odd number of digits"#)
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1, data: ["00", "0"])])"#)
            .contains(r#"hex string "0" has an odd number of digits"#)
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1, data: "0g")])"#)
            .contains("invalid hex digit 'g'")
    );
    assert!(
        compile_err!(r#"Swf(version: 10, tags: [Raw(code: 1, data: [1, 2])])"#)
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
        compile_tags!(r#"DefineSprite(id: 1, tags: [Raw(code: 1, force_long: true, data: "")])"#),
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
        let swf = swfasm::compile_str(&format!("Swf(version: 10, tags: [{tags}])")).unwrap();
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

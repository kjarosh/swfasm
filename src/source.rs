//! The textual (RON) representation of a SWF file.
//!
//! These types are designed to be written by hand, so most fields have defaults
//! and values use convenient units (e.g. pixels instead of twips).
//!
//! They are lowered into `swf` crate types by [`crate::compile`].

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
use std::fmt;
use swf::TagCode;

/// A SWF file: its header and tags.
///
/// This is the root of a `.swfasm` source, written as `Swf(...)`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Swf {
    /// The SWF version, e.g. `10` for Flash Player 10.
    pub version: u8,

    /// The stage rectangle.
    ///
    /// Defaults to 550x400 pixels at the origin.
    #[serde(default)]
    pub stage: Stage,

    /// Frame rate in frames per second.
    ///
    /// It is stored as an 8.8 fixed point number, so it must be a multiple of
    /// 1/256: e.g. `29.97` is not representable, but `29.96875` is. Values that
    /// are not representable are an error.
    ///
    /// Defaults to 24.
    #[serde(default = "default_frame_rate")]
    pub frame_rate: f64,

    /// The frame count stored in the header.
    ///
    /// Defaults to the number of tags with the `ShowFrame` code on the main
    /// timeline.
    #[serde(default)]
    pub num_frames: Option<u16>,

    /// The tags of the main timeline, in order.
    ///
    /// The list is written as is: an `End` tag is not appended automatically.
    #[serde(default)]
    pub tags: Vec<Tag>,
}

fn default_frame_rate() -> f64 {
    24.0
}

/// The stage rectangle, in pixels.
///
/// It is stored in twips (1/20 of a pixel), so all values must be multiples of
/// 0.05: e.g. `10.05` is representable, but `10.01` is not. The edges of the
/// rectangle (`x`, `x + width`, `y` and `y + height`) are stored as 31-bit
/// signed numbers of twips, so they must be between -53687091.2 and
/// 53687091.15. Values that are not representable are an error.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    /// The left edge.
    ///
    /// Defaults to 0.
    #[serde(default)]
    pub x: f64,

    /// The top edge.
    ///
    /// Defaults to 0.
    #[serde(default)]
    pub y: f64,

    /// The width, which is added to `x` to get the right edge.
    pub width: f64,

    /// The height, which is added to `y` to get the bottom edge.
    pub height: f64,
}

impl Default for Stage {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 550.0,
            height: 400.0,
        }
    }
}

/// A tag of a SWF file.
///
/// Variants are named after the SWF tags they write, except for `Raw`, which
/// writes a tag with an arbitrary code and body.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Tag {
    /// Marks the end of the tag list (code 0).
    End,

    /// Displays the current frame and starts the next one (code 1).
    ShowFrame,

    /// Places a character on the display list (code 4).
    PlaceObject {
        /// The ID of the character to place.
        character: u16,

        /// The depth to place the character at.
        depth: u16,

        /// The transformation matrix.
        ///
        /// Defaults to the identity matrix. The tag always contains a matrix.
        #[serde(default)]
        matrix: Matrix,

        /// The color transform, written only when set.
        ///
        /// The tag stores no alpha terms, so `alpha_mult` must be 1 and
        /// `alpha_add` must be 0.
        #[serde(default)]
        color_transform: Option<ColorTransform>,
    },

    /// Removes a character from the display list (code 5).
    ///
    /// Superseded by `RemoveObject2`, which needs only the depth.
    RemoveObject {
        /// The ID of the character to remove.
        character: u16,

        /// The depth of the character to remove.
        depth: u16,
    },

    /// Defines a character with a JPEG image whose encoding tables are in a
    /// `JpegTables` tag (code 6).
    DefineBits {
        /// The character ID.
        id: u16,

        /// The JPEG image, without its encoding tables.
        data: Data,
    },

    /// Defines a button (code 7).
    DefineButton {
        /// The character ID.
        id: u16,

        /// The records of the button.
        ///
        /// The list is terminated automatically by the end flag (a 0 byte).
        records: Vec<ButtonRecord>,

        /// The AVM1 bytecode, written as is: no End action is appended.
        actions: Data,
    },

    /// The JPEG encoding tables shared by the images of the `DefineBits`
    /// characters (code 8).
    JpegTables(Data),

    /// Sets the background color of the stage (code 9).
    ///
    /// The color is in the `0xRRGGBB` format, so it must be at most
    /// `0xFFFFFF`. Larger values are an error.
    SetBackgroundColor(u32),

    /// AVM1 actions run when the current frame is displayed (code 12).
    ///
    /// The data is AVM1 bytecode.
    DoAction(Data),

    /// Defines a character with a JPEG image that carries its own encoding
    /// tables (code 21).
    DefineBitsJpeg2 {
        /// The character ID.
        id: u16,

        /// The JPEG image, with its encoding tables.
        data: Data,
    },

    /// Places a character on the display list, or modifies one placed earlier
    /// (code 26).
    ///
    /// The optional fields are written only when set, along with the flags
    /// saying that they are present.
    PlaceObject2 {
        /// The depth of the character.
        depth: u16,

        /// Whether to modify the character at the depth (the `PlaceFlagMove`
        /// flag).
        #[serde(default)]
        r#move: bool,

        /// The ID of the character to place.
        #[serde(default)]
        character: Option<u16>,

        /// The transformation matrix.
        #[serde(default)]
        matrix: Option<Matrix>,

        /// The color transform.
        #[serde(default)]
        color_transform: Option<ColorTransform>,

        /// The morph ratio, from 0 (the start shape) to 65535 (the end shape).
        #[serde(default)]
        ratio: Option<u16>,

        /// The instance name, written as UTF-8.
        ///
        /// It must not contain null characters.
        #[serde(default)]
        name: Option<String>,

        /// Makes the character a clipping mask for the characters up to this
        /// depth.
        #[serde(default)]
        clip_depth: Option<u16>,

        /// Event handlers of a sprite, in AVM1.
        #[serde(default)]
        clip_actions: Option<Vec<ClipAction>>,

        /// The events of all the clip actions (`AllEventFlags`).
        ///
        /// Defaults to the events of the clip actions combined.
        #[serde(default)]
        all_clip_events: Option<Vec<ClipEvent>>,
    },

    /// Removes the character at a depth from the display list (code 28).
    RemoveObject2 {
        /// The depth of the character to remove.
        depth: u16,
    },

    /// Defines a sprite (movie clip) with its own timeline (code 39).
    DefineSprite {
        /// The character ID.
        id: u16,

        /// The frame count of the sprite.
        ///
        /// Defaults to the number of tags with the `ShowFrame` code on the
        /// sprite's timeline.
        #[serde(default)]
        num_frames: Option<u16>,

        /// The tags of the sprite's timeline, in order.
        ///
        /// Like on the main timeline, the list is written as is: an `End` tag
        /// is not appended automatically. Tags that are not allowed in sprites
        /// are not rejected.
        tags: Vec<Tag>,
    },

    /// Gives a character a name (code 40).
    ///
    /// The SWF specification doesn't document the tag, and players appear to
    /// ignore it.
    NameCharacter {
        /// The ID of the character to name.
        character: u16,

        /// The name, written as UTF-8.
        ///
        /// It must not contain null characters.
        name: String,
    },

    /// Gives the current frame a label (code 43).
    FrameLabel(FrameLabel),

    /// Exports characters under names, for other SWFs to import (code 56).
    ///
    /// The count of assets is stored in 16 bits, so more than 65535 assets are
    /// an error.
    ///
    /// ```ron
    /// ExportAssets([
    ///     (character: 1, name: "clip"),
    ///     (character: 2, name: "data"),
    /// ]),
    /// ```
    ExportAssets(Vec<ExportedAsset>),

    /// AVM1 actions that initialize a sprite (code 59).
    DoInitAction {
        /// The character ID of the sprite.
        ///
        /// It is written as is: it doesn't have to refer to a sprite, or to any
        /// character at all.
        sprite: u16,

        /// The AVM1 bytecode.
        actions: Data,
    },

    /// Defines a character that plays a video stream (code 60).
    ///
    /// The tag defines only the stream: its frames are written in `VideoFrame`
    /// tags.
    DefineVideoStream(DefineVideoStream),

    /// A frame of a video stream (code 61).
    VideoFrame(VideoFrame),

    /// Overrides the default limits of ActionScript (code 65).
    ScriptLimits {
        /// The maximum depth of nested function calls (`MaxRecursionDepth`).
        max_recursion_depth: u16,

        /// The number of seconds that the player runs ActionScript for before
        /// asking whether to stop it (`ScriptTimeoutSeconds`).
        timeout_seconds: u16,
    },

    /// Sets the tab order of the character at a depth (code 66).
    SetTabIndex {
        /// The depth of the character.
        depth: u16,

        /// The position of the character in the tab order (`TabIndex`).
        tab_index: u16,
    },

    /// Describes the SWF to the player (code 69).
    ///
    /// All flags default to false.
    FileAttributes {
        /// The SWF uses AVM2 (ActionScript 3) instead of AVM1.
        #[serde(default)]
        action_script_3: bool,

        /// When run locally, the SWF gets network access instead of local file
        /// access.
        #[serde(default)]
        use_network: bool,

        /// The SWF contains a `Metadata` tag.
        #[serde(default)]
        has_metadata: bool,

        /// Requests GPU compositing.
        #[serde(default)]
        use_gpu: bool,

        /// Requests hardware acceleration.
        #[serde(default)]
        use_direct_blit: bool,
    },

    /// An ABC file (code 72).
    DoAbc(Data),

    /// Character IDs with the names of classes (code 76).
    ///
    /// The count of links is stored in 16 bits, so more than 65535 links are an
    /// error.
    ///
    /// ```ron
    /// SymbolClass([
    ///     (character: 0, name: "Main"),
    ///     (character: 1, name: "com.example.Clip"),
    /// ]),
    /// ```
    SymbolClass(Vec<SymbolClassLink>),

    /// Describes the SWF with XML metadata, usually RDF (code 77).
    ///
    /// The metadata is followed by a null terminator, so it must not contain
    /// null bytes.
    Metadata(Data),

    /// An ABC file with a name and flags (code 82).
    DoAbc2 {
        /// The `kDoAbcLazyInitializeFlag` flag (bit 0).
        ///
        /// Defaults to false.
        #[serde(default)]
        lazy_initialize: bool,

        /// The name, written as UTF-8.
        ///
        /// It must not contain null characters. Defaults to an empty name.
        #[serde(default)]
        name: String,

        /// The ABC file.
        data: Data,
    },

    /// Defines a character holding arbitrary binary data (code 87).
    DefineBinaryData {
        /// The character ID.
        id: u16,

        /// The data.
        data: Data,
    },

    /// Gives a font character its name and copyright notice (code 88).
    DefineFontName {
        /// The character ID of the font.
        font: u16,

        /// The name of the font, written as UTF-8.
        ///
        /// It must not contain null characters.
        name: String,

        /// The copyright notice, written as UTF-8.
        ///
        /// It must not contain null characters. Defaults to an empty notice.
        #[serde(default)]
        copyright: String,
    },

    /// A tag with an arbitrary code and body.
    ///
    /// The body is written as is, with the short (2 byte) header form when it
    /// is shorter than 63 bytes, and the long (6 byte) one otherwise.
    Raw {
        /// The tag code.
        ///
        /// It is stored in 10 bits, so it must be at most 1023. Larger values
        /// are an error.
        code: u16,

        /// The tag body, without the header.
        data: Data,

        /// Whether to use the long (6 byte) header form even when the body is
        /// short enough for the short one.
        #[serde(default)]
        force_long: bool,
    },
}

impl Tag {
    /// Whether this tag is written with the `ShowFrame` tag code.
    pub fn is_show_frame(&self) -> bool {
        match self {
            Self::ShowFrame => true,
            Self::Raw { code, .. } => *code == TagCode::ShowFrame as u16,
            _ => false,
        }
    }
}

/// A transformation matrix (`MATRIX`), which transforms a point `(x, y)` into
/// `(x * scale_x + y * rotate_skew1 + translate_x, x * rotate_skew0 + y *
/// scale_y + translate_y)`.
///
/// Scale and rotate/skew values are stored as 16.16 fixed point numbers in at
/// most 31 bits, so they must be multiples of 1/65536 between -16384 and
/// 16383.9999847412109375. Translation is stored in twips in at most 31 bits,
/// so it must be a multiple of 0.05 between -53687091.2 and 53687091.15. Values
/// that are not representable are an error.
///
/// Scale and rotate/skew values are written only when they differ from the
/// identity matrix.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Matrix {
    /// The horizontal scale.
    ///
    /// Defaults to 1.
    #[serde(default = "one")]
    pub scale_x: f64,

    /// The vertical scale.
    ///
    /// Defaults to 1.
    #[serde(default = "one")]
    pub scale_y: f64,

    /// The first rotate/skew value, which adds `x * rotate_skew0` to `y`.
    ///
    /// Defaults to 0.
    #[serde(default)]
    pub rotate_skew0: f64,

    /// The second rotate/skew value, which adds `y * rotate_skew1` to `x`.
    ///
    /// Defaults to 0.
    #[serde(default)]
    pub rotate_skew1: f64,

    /// The horizontal translation, in pixels.
    ///
    /// Defaults to 0.
    #[serde(default)]
    pub translate_x: f64,

    /// The vertical translation, in pixels.
    ///
    /// Defaults to 0.
    #[serde(default)]
    pub translate_y: f64,
}

impl Default for Matrix {
    fn default() -> Self {
        Self {
            scale_x: 1.0,
            scale_y: 1.0,
            rotate_skew0: 0.0,
            rotate_skew1: 0.0,
            translate_x: 0.0,
            translate_y: 0.0,
        }
    }
}

/// A color transform (`CXFORM` or `CXFORMWITHALPHA`): each color component is
/// multiplied by its `_mult` term, and then its `_add` term is added to it.
///
/// Values are stored in at most 15 bits. Multiplication terms are stored as
/// 8.8 fixed point numbers, so they must be multiples of 1/256 between -64 and
/// 63.99609375. Addition terms must be between -16384 and 16383. Values that
/// are not representable are an error.
///
/// Multiplication terms are written only when one of them is not 1, and
/// addition terms only when one of them is not 0.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorTransform {
    /// Defaults to 1.
    #[serde(default = "one")]
    pub red_mult: f64,

    /// Defaults to 1.
    #[serde(default = "one")]
    pub green_mult: f64,

    /// Defaults to 1.
    #[serde(default = "one")]
    pub blue_mult: f64,

    /// Defaults to 1.
    #[serde(default = "one")]
    pub alpha_mult: f64,

    /// Defaults to 0.
    #[serde(default)]
    pub red_add: i16,

    /// Defaults to 0.
    #[serde(default)]
    pub green_add: i16,

    /// Defaults to 0.
    #[serde(default)]
    pub blue_add: i16,

    /// Defaults to 0.
    #[serde(default)]
    pub alpha_add: i16,
}

fn one() -> f64 {
    1.0
}

/// An event handler of a sprite (`CLIPACTIONRECORD`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipAction {
    /// The events that run the actions.
    ///
    /// Before SWF 6, event flags are stored in 16 bits, so `DragOut`,
    /// `KeyPress` and `Construct` are an error.
    pub events: Vec<ClipEvent>,

    /// The key that runs the actions on `KeyPress`.
    #[serde(default)]
    pub key_code: Option<u8>,

    /// The AVM1 bytecode.
    pub actions: Data,
}

/// An event of a sprite, with the value of its flag in `CLIPEVENTFLAGS`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum ClipEvent {
    Load = 1 << 0,
    EnterFrame = 1 << 1,
    Unload = 1 << 2,
    MouseMove = 1 << 3,
    MouseDown = 1 << 4,
    MouseUp = 1 << 5,
    KeyDown = 1 << 6,
    KeyUp = 1 << 7,
    Data = 1 << 8,
    Initialize = 1 << 9,
    Press = 1 << 10,
    Release = 1 << 11,
    ReleaseOutside = 1 << 12,
    RollOver = 1 << 13,
    RollOut = 1 << 14,
    DragOver = 1 << 15,
    DragOut = 1 << 16,
    KeyPress = 1 << 17,
    Construct = 1 << 18,
}

/// A record of a button (`BUTTONRECORD`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ButtonRecord {
    /// The states of the record, written as its flags.
    pub states: Vec<ButtonState>,

    /// The ID of the character.
    pub character: u16,

    /// The depth.
    pub depth: u16,

    /// The transformation matrix.
    ///
    /// Defaults to the identity matrix. The record always contains a matrix.
    #[serde(default)]
    pub matrix: Matrix,
}

/// A state of a button, with the value of its flag in `BUTTONRECORD`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum ButtonState {
    /// `ButtonStateUp`.
    Up = 1 << 0,

    /// `ButtonStateOver`.
    Over = 1 << 1,

    /// `ButtonStateDown`.
    Down = 1 << 2,

    /// `ButtonStateHitTest`.
    HitTest = 1 << 3,
}

/// A video stream, from a `DefineVideoStream` tag.
///
/// ```ron
/// DefineVideoStream(
///     id: 1,
///     num_frames: 2,
///     width: 16,
///     height: 16,
///     codec: ScreenVideo,
/// ),
/// ```
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefineVideoStream {
    /// The character ID.
    pub id: u16,

    /// The number of video frames (`NumFrames`).
    pub num_frames: u16,

    /// The width of the video, in pixels.
    pub width: u16,

    /// The height of the video, in pixels.
    pub height: u16,

    /// Whether the player smooths the video (`VideoFlagsSmoothing`).
    ///
    /// Defaults to false.
    #[serde(default)]
    pub smoothing: bool,

    /// The deblocking filter that the player applies to the video
    /// (`VideoFlagsDeblocking`).
    ///
    /// Defaults to `UseVideoPacketValue`.
    #[serde(default)]
    pub deblocking: VideoDeblocking,

    /// The codec of the video frames (`CodecID`).
    pub codec: VideoCodec,
}

/// A frame of a video stream, from a `VideoFrame` tag.
///
/// ```ron
/// VideoFrame(stream: 1, frame: 0, data: File("frame0.bin")),
/// ```
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoFrame {
    /// The character ID of the video stream (`StreamID`).
    pub stream: u16,

    /// The number of the frame in the stream (`FrameNum`), counted from 0.
    pub frame: u16,

    /// The frame, encoded in the codec of the stream.
    pub data: Data,
}

/// The deblocking filter that the player applies to a video stream, from
/// `VideoFlagsDeblocking` in `DefineVideoStream`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub enum VideoDeblocking {
    /// Use the filter of the video packets (0).
    #[default]
    UseVideoPacketValue,

    /// No filter (1).
    Off,

    /// Level 1, the fastest filter (2).
    Level1,

    /// Level 2 (3).
    Level2,

    /// Level 3 (4).
    Level3,

    /// Level 4, the slowest filter (5).
    Level4,
}

/// The codec of the frames of a video stream, from `CodecID` in
/// `DefineVideoStream`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum VideoCodec {
    /// No codec (0), which the SWF specification doesn't define.
    None,

    /// Sorenson H.263 (2).
    H263,

    /// Screen video (3).
    ScreenVideo,

    /// On2 VP6 (4).
    Vp6,

    /// On2 VP6 with an alpha channel (5).
    Vp6WithAlpha,

    /// Screen video version 2 (6).
    ScreenVideoV2,

    /// H.264, also known as AVC (7).
    H264,
}

/// The label of a frame, from a `FrameLabel` tag.
///
/// ```ron
/// // The label itself.
/// FrameLabel("start"),
///
/// // The same, written as a struct.
/// FrameLabel(label: "start"),
///
/// // A named anchor.
/// FrameLabel(label: "start", anchor: true),
/// ```
#[derive(Debug, PartialEq)]
pub struct FrameLabel {
    /// The label, written as UTF-8.
    ///
    /// It must not contain null characters.
    pub label: String,

    /// Whether the label is a named anchor, which the player can navigate to
    /// with the URL fragment (`NamedAnchorFlag`, from SWF 6).
    ///
    /// It is written as a byte of 1 after the label. Setting it before SWF 6 is
    /// an error.
    ///
    /// Defaults to false, in which case the byte is not written.
    pub anchor: bool,
}

impl<'de> Deserialize<'de> for FrameLabel {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct FrameLabelVisitor;

        impl<'de> Visitor<'de> for FrameLabelVisitor {
            type Value = FrameLabel;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a label or a struct")
            }

            fn visit_str<E: de::Error>(self, label: &str) -> Result<FrameLabel, E> {
                Ok(FrameLabel {
                    label: label.to_owned(),
                    anchor: false,
                })
            }

            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<FrameLabel, A::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields {
                    label: String,
                    #[serde(default)]
                    anchor: bool,
                }

                let fields = Fields::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(FrameLabel {
                    label: fields.label,
                    anchor: fields.anchor,
                })
            }
        }

        deserializer.deserialize_any(FrameLabelVisitor)
    }
}

/// A character exported under a name, in `ExportAssets`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportedAsset {
    /// The ID of the character to export.
    pub character: u16,

    /// The name to export the character under, written as UTF-8.
    ///
    /// It must not contain null characters.
    pub name: String,
}

/// A character with the name of a class, in `SymbolClass`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SymbolClassLink {
    /// The ID of the character.
    pub character: u16,

    /// The name of the class, written as UTF-8.
    ///
    /// It must not contain null characters.
    pub name: String,
}

/// Binary data.
#[derive(Debug, PartialEq, Deserialize)]
pub enum Data {
    /// Inline bytes.
    InlineBytes(Bytes),

    /// The contents of a file, written as `File("data.bin")`.
    ///
    /// Files are relative to the swfasm source.
    File(String),

    /// AVM1 bytecode compiled by Rascal from an ActionScript 2 script.
    ///
    /// Requires the `rascal` feature: without it, sources that use it fail to
    /// compile.
    Rascal(Rascal),
}

/// Binary data, written as a hex string with optional whitespace (`"01 02 ff"`),
/// or a list of such strings, which are concatenated.
///
/// The list form allows commenting individual parts of the data:
///
/// ```ron
/// data: InlineBytes([
///     "0a 00", // frame count
///     "ff ff", // flags
/// ]),
/// ```
#[derive(Debug, Default, PartialEq)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
    fn parse_hex(hex: &str) -> Result<Vec<u8>, String> {
        let digits = hex
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .map(|c| {
                c.to_digit(16)
                    .map(|d| d as u8)
                    .ok_or_else(|| format!("invalid hex digit '{c}'"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !digits.len().is_multiple_of(2) {
            return Err(format!("hex string \"{hex}\" has an odd number of digits"));
        }
        Ok(digits
            .chunks(2)
            .map(|pair| pair[0] << 4 | pair[1])
            .collect())
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct BytesVisitor;

        impl<'de> Visitor<'de> for BytesVisitor {
            type Value = Bytes;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a hex string or a list of hex strings")
            }

            fn visit_str<E: de::Error>(self, hex: &str) -> Result<Bytes, E> {
                Bytes::parse_hex(hex).map(Bytes).map_err(E::custom)
            }

            fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Bytes, A::Error> {
                /// A string borrowed from the input when possible. Strings with
                /// escapes can't be, so they are owned.
                ///
                /// `Cow<str>` alone is always owned: it borrows only with
                /// `#[serde(borrow)]`.
                #[derive(Deserialize)]
                #[serde(transparent)]
                struct Hex<'a>(#[serde(borrow)] Cow<'a, str>);

                let mut bytes = Vec::new();
                while let Some(Hex(hex)) = seq.next_element()? {
                    bytes.extend(Bytes::parse_hex(&hex).map_err(de::Error::custom)?);
                }
                Ok(Bytes(bytes))
            }
        }

        deserializer.deserialize_any(BytesVisitor)
    }
}

/// An ActionScript 2 script, compiled by Rascal.
///
/// ```ron
/// // The script itself.
/// data: Rascal(r#"trace("hello");"#),
///
/// // The same, written as a struct.
/// data: Rascal(source: r#"trace("hello");"#),
///
/// // A file with the script, relative to the swfasm source.
/// data: Rascal(file: "script.as"),
///
/// // Compiled for SWF 6, instead of the version of the file.
/// data: Rascal(file: "script.as", swf_version: 6),
/// ```
///
/// - The script is loose code, like a frame script. Classes and interfaces are
///   not supported.
/// - It is compiled for the SWF version of the file, unless `swf_version` says
///   otherwise.
/// - Files included with `#include` are relative to the swfasm source too.
/// - Pcode can be written in `@PCode { ... }` blocks.
#[derive(Debug, PartialEq)]
pub struct Rascal {
    /// The script.
    pub source: RascalSource,

    /// The SWF version to compile the script for, instead of the version of
    /// the SWF file.
    pub swf_version: Option<u8>,

    /// Whether constant expressions are evaluated at compile time, e.g.
    /// `trace(1 + 2)` pushes `3` instead of adding `1` and `2`.
    ///
    /// Defaults to true.
    pub fold_constants: bool,

    /// Whether local variables of functions are stored in registers instead of
    /// by name. It has an effect only from SWF 7, which has registers.
    ///
    /// Defaults to true.
    pub promote_variables_to_registers: bool,
}

/// Where the script of [`Rascal`] comes from.
#[derive(Debug, PartialEq)]
pub enum RascalSource {
    /// The text of the script, from `source`.
    Inline(String),

    /// The path of a file with the script, from `file`.
    File(String),
}

impl<'de> Deserialize<'de> for Rascal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RascalVisitor;

        impl<'de> Visitor<'de> for RascalVisitor {
            type Value = Rascal;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a script or a struct")
            }

            fn visit_str<E: de::Error>(self, source: &str) -> Result<Rascal, E> {
                Ok(Rascal {
                    source: RascalSource::Inline(source.to_owned()),
                    swf_version: None,
                    fold_constants: true,
                    promote_variables_to_registers: true,
                })
            }

            fn visit_map<A: de::MapAccess<'de>>(self, map: A) -> Result<Rascal, A::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields {
                    source: Option<String>,
                    file: Option<String>,
                    swf_version: Option<u8>,
                    fold_constants: Option<bool>,
                    promote_variables_to_registers: Option<bool>,
                }

                let fields = Fields::deserialize(de::value::MapAccessDeserializer::new(map))?;
                let source = match (fields.source, fields.file) {
                    (Some(source), None) => RascalSource::Inline(source),
                    (None, Some(path)) => RascalSource::File(path),
                    (None, None) => return Err(de::Error::custom("expected `source` or `file`")),
                    (Some(_), Some(_)) => {
                        return Err(de::Error::custom(
                            "expected `source` or `file`, but found both",
                        ));
                    }
                };
                Ok(Rascal {
                    source,
                    swf_version: fields.swf_version,
                    fold_constants: fields.fold_constants.unwrap_or(true),
                    promote_variables_to_registers: fields
                        .promote_variables_to_registers
                        .unwrap_or(true),
                })
            }
        }

        deserializer.deserialize_any(RascalVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_bytes(text: &str) -> Result<Vec<u8>, String> {
        ron::from_str::<Bytes>(text)
            .map(|bytes| bytes.0)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn deserialize_bytes_from_string() {
        assert_eq!(parse_bytes(r#""""#), Ok(vec![]));
        assert_eq!(parse_bytes(r#""01 02 ff""#), Ok(vec![0x01, 0x02, 0xff]));
        assert_eq!(parse_bytes(r#""0102ff""#), Ok(vec![0x01, 0x02, 0xff]));
        assert_eq!(parse_bytes(r#""AbCd""#), Ok(vec![0xab, 0xcd]));
        assert_eq!(
            parse_bytes("\" 0 1\t02\n\r f f \""),
            Ok(vec![0x01, 0x02, 0xff])
        );
    }

    #[test]
    fn deserialize_bytes_from_list() {
        assert_eq!(parse_bytes("[]"), Ok(vec![]));
        assert_eq!(parse_bytes(r#"["01"]"#), Ok(vec![0x01]));
        assert_eq!(
            parse_bytes(r#"["01 02", "", "ff",]"#),
            Ok(vec![0x01, 0x02, 0xff])
        );
        assert_eq!(
            parse_bytes("[\n    \"01\", // one\n    \"02\", /* two */\n]"),
            Ok(vec![0x01, 0x02])
        );
        assert_eq!(
            parse_bytes(r#"["01\t02", "\u{66}f"]"#),
            Ok(vec![0x01, 0x02, 0xff])
        );
    }

    #[test]
    fn deserialize_bytes_errors() {
        let parse_err = |text| parse_bytes(text).unwrap_err();
        assert!(parse_err(r#""0""#).contains("odd number of digits"));
        assert!(parse_err(r#""01 2""#).contains("odd number of digits"));
        assert!(parse_err(r#"["0", "1"]"#).contains("odd number of digits"));
        assert!(parse_err(r#""0g""#).contains("invalid hex digit 'g'"));
        assert!(parse_err(r#""0x01""#).contains("invalid hex digit 'x'"));
        assert!(parse_err(r#"["01", "zz"]"#).contains("invalid hex digit 'z'"));
        assert!(parse_err("1").contains("a hex string or a list of hex strings"));
        assert!(parse_bytes("[1]").is_err());
    }
}

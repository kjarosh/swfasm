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

    /// Removes a character from the display list (code 5).
    ///
    /// Superseded by `RemoveObject2`, which needs only the depth.
    RemoveObject {
        /// The ID of the character to remove.
        character: u16,

        /// The depth of the character to remove.
        depth: u16,
    },

    /// Sets the background color of the stage (code 9).
    ///
    /// The color is in the `0xRRGGBB` format, so it must be at most
    /// `0xFFFFFF`. Larger values are an error.
    SetBackgroundColor(u32),

    /// AVM1 actions run when the current frame is displayed (code 12).
    ///
    /// The data is AVM1 bytecode.
    DoAction(Data),

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

/// Binary data.
#[derive(Debug, PartialEq, Deserialize)]
pub enum Data {
    /// Inline bytes.
    InlineBytes(Bytes),
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

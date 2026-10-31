//! Compiles the textual (RON) representation of a SWF file into a SWF file.

use crate::source::{DoActionContent, Swf, Tag};
use anyhow::{Context, Result, bail};
use ron::extensions::Extensions;
use swf::{Color, FileAttributes, Fixed8, Header, Rectangle, TagCode, Twips};

/// Parses the textual representation of a SWF file.
///
/// ```
/// let swf = swfasm::parse("Swf(
///     version: 10,
///     num_frames: 2,
///     tags: [ShowFrame],
/// )")?;
/// assert_eq!(swf.version, 10);
/// assert_eq!(swf.num_frames, Some(2));
/// assert_eq!((swf.stage.width, swf.stage.height), (550.0, 400.0));
/// assert_eq!(swf.frame_rate, 24.0);
/// assert_eq!(swf.tags.len(), 1);
///
/// // Syntax errors point to the line and column.
/// let err = swfasm::parse("Swf(
///     version: 10
///     tags: [ShowFrame],
/// )").unwrap_err();
/// assert_eq!(format!("{err:#}"), "3:5: Expected comma");
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn parse(text: &str) -> Result<Swf> {
    Ok(ron::Options::default()
        .with_default_extension(Extensions::IMPLICIT_SOME)
        .from_str(text)?)
}

/// Compiles the textual representation of a SWF file into SWF bytes.
///
/// This is [`parse`] followed by [`compile`], so it fails if either of them
/// does.
pub fn compile_str(text: &str) -> Result<Vec<u8>> {
    compile(&parse(text)?)
}

/// Compiles a SWF file into SWF bytes.
///
/// Unless set explicitly, the frame count in the header is the number of
/// `ShowFrame` tags.
///
/// ```
/// use swfasm::source::Tag;
///
/// let mut swf = swfasm::parse("Swf(version: 10)")?;
/// swf.tags.extend([Tag::ShowFrame, Tag::ShowFrame, Tag::End]);
///
/// let bytes = swfasm::compile(&swf)?;
/// assert_eq!(&bytes[..4], b"FWS\x0a"); // uncompressed, version 10
///
/// swf.version = 11;
/// let bytes = swfasm::compile(&swf)?;
/// assert_eq!(&bytes[..4], b"FWS\x0b"); // uncompressed, version 11
///
/// // Values that parse but can't be encoded result in errors.
/// let swf = swfasm::parse("Swf(
///     version: 10,
///     tags: [SetBackgroundColor(0x1000000)],
/// )")?;
/// let err = swfasm::compile(&swf).unwrap_err();
/// assert!(format!("{err:#}").contains("not in the 0xRRGGBB format"));
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn compile(swf: &Swf) -> Result<Vec<u8>> {
    let mut tags = Vec::new();
    write_tags(&swf.tags, swf.version, &mut tags)?;
    let num_frames = num_frames(swf.num_frames, &swf.tags)?;

    let x = twips(swf.stage.x).context("stage x")?;
    let y = twips(swf.stage.y).context("stage y")?;
    let width = twips(swf.stage.width).context("stage width")?;
    let height = twips(swf.stage.height).context("stage height")?;
    let header = Header {
        compression: swf::Compression::None,
        version: swf.version,
        stage_size: Rectangle {
            x_min: x,
            x_max: add_twips(x, width).context("stage width")?,
            y_min: y,
            y_max: add_twips(y, height).context("stage height")?,
        },
        frame_rate: fixed8(swf.frame_rate).context("frame rate")?,
        num_frames,
    };

    let mut output = Vec::new();
    swf::write::write_swf_raw_tags(&header, &tags, &mut output)?;
    Ok(output)
}

/// Converts pixels to twips, failing if the value is not a whole number of twips.
fn twips(pixels: f64) -> Result<Twips> {
    let twips = Twips::from_pixels(pixels);
    if twips.to_pixels() != pixels {
        bail!("{pixels}px is not a whole number of twips");
    }
    Ok(twips)
}

fn add_twips(a: Twips, b: Twips) -> Result<Twips> {
    let sum = a.get().checked_add(b.get()).context("value too large")?;
    Ok(Twips::new(sum))
}

/// Converts a number to 8.8 fixed point, failing if it is not exactly representable.
fn fixed8(value: f64) -> Result<Fixed8> {
    let fixed = Fixed8::from_f64(value);
    if fixed.to_f64() != value {
        bail!("{value} is not representable as an 8.8 fixed point number");
    }
    Ok(fixed)
}

/// Returns the explicit frame count, or else the number of `ShowFrame` tags.
fn num_frames(explicit: Option<u16>, tags: &[Tag]) -> Result<u16> {
    match explicit {
        Some(num_frames) => Ok(num_frames),
        None => {
            let count = tags.iter().filter(|tag| tag.is_show_frame()).count();
            u16::try_from(count).context("too many frames")
        }
    }
}

fn write_tags(tags: &[Tag], swf_version: u8, output: &mut Vec<u8>) -> Result<()> {
    for (i, tag) in tags.iter().enumerate() {
        write_tag(tag, swf_version, output).with_context(|| format!("tag #{i}"))?;
    }
    Ok(())
}

fn write_tag(tag: &Tag, swf_version: u8, output: &mut Vec<u8>) -> Result<()> {
    let swf_tag = match tag {
        Tag::ShowFrame => swf::Tag::ShowFrame,
        Tag::End => swf::Tag::End,
        Tag::FileAttributes {
            action_script_3,
            use_network,
            has_metadata,
            use_gpu,
            use_direct_blit,
        } => {
            let mut attributes = FileAttributes::empty();
            attributes.set(FileAttributes::IS_ACTION_SCRIPT_3, *action_script_3);
            attributes.set(FileAttributes::USE_NETWORK_SANDBOX, *use_network);
            attributes.set(FileAttributes::HAS_METADATA, *has_metadata);
            attributes.set(FileAttributes::USE_GPU, *use_gpu);
            attributes.set(FileAttributes::USE_DIRECT_BLIT, *use_direct_blit);
            swf::Tag::FileAttributes(attributes)
        }
        Tag::SetBackgroundColor(rgb) => {
            if *rgb > 0xFFFFFF {
                bail!("background color {rgb:#x} is not in the 0xRRGGBB format");
            }
            swf::Tag::SetBackgroundColor(Color::from_rgb(*rgb, 255))
        }
        Tag::DoAction(DoActionContent::Bytes(bytes)) => swf::Tag::DoAction(&bytes.0),
        Tag::DefineSprite {
            id,
            num_frames: explicit_num_frames,
            tags,
        } => {
            // The nested tags are written by hand instead of with
            // `swf::Tag::DefineSprite` (which appends an `End` tag), so that
            // they are written as is, and can include raw tags with the long
            // header form.
            // TODO Maybe add support to the swf crate?
            let mut body = Vec::new();
            body.extend_from_slice(&id.to_le_bytes());
            body.extend_from_slice(&num_frames(*explicit_num_frames, tags)?.to_le_bytes());
            write_tags(tags, swf_version, &mut body)?;
            swf::write::write_tag(
                &swf::Tag::Unknown {
                    tag_code: TagCode::DefineSprite as u16,
                    data: &body,
                },
                swf_version,
                output,
            )?;
            return Ok(());
        }
        Tag::Raw {
            code,
            data,
            force_long: false,
        } => {
            check_tag_code(*code)?;
            swf::Tag::Unknown {
                tag_code: *code,
                data: &data.0,
            }
        }
        Tag::Raw {
            code,
            data,
            force_long: true,
        } => {
            check_tag_code(*code)?;

            // TODO Maybe add support to the swf crate?
            let length = u32::try_from(data.0.len()).context("tag body too long")?;
            output.extend_from_slice(&(code << 6 | 0b111111).to_le_bytes());
            output.extend_from_slice(&length.to_le_bytes());
            output.extend_from_slice(&data.0);
            return Ok(());
        }
    };

    swf::write::write_tag(&swf_tag, swf_version, output)?;
    Ok(())
}

/// Fails if the tag code does not fit in the 10 bits of a tag header.
fn check_tag_code(code: u16) -> Result<()> {
    if code >= 1 << 10 {
        bail!("tag code {code} does not fit in 10 bits");
    }
    Ok(())
}

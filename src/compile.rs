//! Compiles the textual (RON) representation of a SWF file into a SWF file.

mod button;
mod matrix;
mod place_object;
mod video;

use crate::files::{FileProvider, NoFiles};
use crate::source::{Data, GridFit, NamedFrame, Rect, Swf, Tag};
use anyhow::{Context, Result, anyhow, bail};
use place_object::PlaceObjectData;
use ron::extensions::Extensions;
use std::borrow::Cow;
use std::fmt::Display;
use swf::{
    Color, CsmTextSettings, DefineBinaryData, DefineSceneAndFrameLabelData, DoAbc2, DoAbc2Flag,
    ExportedAsset, FileAttributes, Fixed8, Fixed16, FrameLabel, FrameLabelData, Header,
    NameCharacter, ProductInfo, Rectangle, RemoveObject, SwfStr, SymbolClassLink, TagCode,
    TextGridFit, Twips,
};

/// Compiles SWF files, with the settings that compiling depends on.
///
/// ```
/// use swfasm::Compiler;
/// use swfasm::source::Tag;
///
/// let compiler = Compiler::new();
/// let mut swf = compiler.parse("Swf(version: 10)")?;
/// swf.tags.extend([Tag::ShowFrame, Tag::ShowFrame, Tag::End]);
///
/// let bytes = compiler.compile(&swf)?;
/// assert_eq!(&bytes[..4], b"FWS\x0a"); // uncompressed, version 10
///
/// swf.version = 11;
/// let bytes = compiler.compile(&swf)?;
/// assert_eq!(&bytes[..4], b"FWS\x0b"); // uncompressed, version 11
///
/// // Values that parse but can't be encoded result in errors.
/// let err = compiler.compile_str("Swf(
///     version: 10,
///     tags: [SetBackgroundColor(0x1000000)],
/// )").unwrap_err();
/// assert!(format!("{err:#}").contains("not in the 0xRRGGBB format"));
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Clone)]
pub struct Compiler<'a> {
    files: &'a dyn FileProvider,
    #[cfg(feature = "asc")]
    asc: Option<crate::asc::AscConfig>,
}

impl Default for Compiler<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Compiler<'a> {
    /// A compiler with the default settings.
    pub fn new() -> Self {
        Self {
            files: &NoFiles,
            #[cfg(feature = "asc")]
            asc: None,
        }
    }

    /// Reads the files that sources refer to from `files`.
    ///
    /// Without it, sources that refer to files fail to compile.
    pub fn with_files(mut self, files: &'a dyn FileProvider) -> Self {
        self.files = files;
        self
    }

    /// Compiles ActionScript 3 with asc, from the given files.
    ///
    /// Without it, sources that use asc fail to compile.
    ///
    /// Requires the `asc` feature.
    #[cfg(feature = "asc")]
    pub fn with_asc(mut self, config: crate::asc::AscConfig) -> Self {
        self.asc = Some(config);
        self
    }

    /// Parses the textual representation of a SWF file.
    ///
    /// ```
    /// let compiler = swfasm::Compiler::new();
    /// let swf = compiler.parse("Swf(
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
    /// let err = compiler.parse("Swf(
    ///     version: 10
    ///     tags: [ShowFrame],
    /// )").unwrap_err();
    /// assert_eq!(format!("{err:#}"), "3:5: Expected comma");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn parse(&self, text: &str) -> Result<Swf> {
        Ok(ron::Options::default()
            .with_default_extension(Extensions::IMPLICIT_SOME | Extensions::UNWRAP_VARIANT_NEWTYPES)
            .from_str(text)?)
    }

    /// Compiles the textual representation of a SWF file into SWF bytes.
    ///
    /// This is [`parse`](Self::parse) followed by [`compile`](Self::compile),
    /// so it fails if either of them does.
    pub fn compile_str(&self, text: &str) -> Result<Vec<u8>> {
        self.compile(&self.parse(text)?)
    }

    /// Compiles a SWF file into SWF bytes.
    pub fn compile(&self, swf: &Swf) -> Result<Vec<u8>> {
        let mut tags = Vec::new();
        write_tags(&swf.tags, swf.version, self, &mut tags)?;
        let num_frames = num_frames(swf.num_frames, &swf.tags)?;

        let header = Header {
            compression: swf::Compression::None,
            version: swf.version,
            stage_size: rectangle(&swf.stage).context("stage")?,
            frame_rate: fixed8(swf.frame_rate).context("frame rate")?,
            num_frames,
        };

        let mut output = Vec::new();
        swf::write::write_swf_raw_tags(&header, &tags, &mut output)?;
        Ok(output)
    }
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

/// Converts a number to 16.16 fixed point, failing if it is not exactly representable.
fn fixed16(value: f64) -> Result<Fixed16> {
    let fixed = Fixed16::from_f64(value);
    if fixed.to_f64() != value {
        bail!("{value} is not representable as a 16.16 fixed point number");
    }
    Ok(fixed)
}

/// Converts a string to a null-terminated `SwfStr`, failing if it contains a
/// null character.
fn null_terminated_str(string: &str) -> Result<&SwfStr> {
    if string.contains('\0') {
        bail!("{string:?} contains a null character");
    }
    Ok(SwfStr::from_utf8_str(string))
}

/// Converts bytes to a null-terminated `SwfStr`, failing if they contain a null
/// byte.
fn null_terminated_bytes(bytes: &[u8]) -> Result<&SwfStr> {
    if let Some(offset) = bytes.iter().position(|&byte| byte == 0) {
        bail!("null byte at offset {offset}, which would end the string early");
    }
    Ok(SwfStr::from_bytes(bytes))
}

/// Fails if `raw`, the encoding of `value`, does not fit in `num_bits` signed
/// bits.
fn check_sbits(raw: i32, num_bits: u32, value: impl Display) -> Result<()> {
    let limit = 1 << (num_bits - 1);
    if !(-limit..limit).contains(&raw) {
        bail!("{value} is out of range");
    }
    Ok(())
}

/// Converts named frames for the swf crate.
fn frame_label_data(frames: &[NamedFrame]) -> Result<Vec<FrameLabelData<'_>>> {
    frames
        .iter()
        .enumerate()
        .map(|(i, frame)| {
            Ok(FrameLabelData {
                frame_num: frame.frame,
                label: null_terminated_str(&frame.name)
                    .with_context(|| format!("frame #{i}: name"))?,
            })
        })
        .collect()
}

/// Converts a rectangle in pixels into one in twips, failing if an edge is not
/// representable.
fn rectangle(rect: &Rect) -> Result<Rectangle<Twips>> {
    let x_min = twips(rect.x).context("x")?;
    let y_min = twips(rect.y).context("y")?;
    let x_max = add_twips(x_min, twips(rect.width).context("width")?).context("width")?;
    let y_max = add_twips(y_min, twips(rect.height).context("height")?).context("height")?;
    for (edge, name) in [
        (x_min, "x"),
        (x_max, "width"),
        (y_min, "y"),
        (y_max, "height"),
    ] {
        check_sbits(edge.get(), 31, format!("{}px", edge.to_pixels())).context(name)?;
    }
    Ok(Rectangle {
        x_min,
        x_max,
        y_min,
        y_max,
    })
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

fn write_tags(
    tags: &[Tag],
    swf_version: u8,
    compiler: &Compiler,
    output: &mut Vec<u8>,
) -> Result<()> {
    for (i, tag) in tags.iter().enumerate() {
        write_tag(tag, swf_version, compiler, output).with_context(|| format!("tag #{i}"))?;
    }
    Ok(())
}

fn write_tag(tag: &Tag, swf_version: u8, compiler: &Compiler, output: &mut Vec<u8>) -> Result<()> {
    // The data of the tag, if it has any, declared here so that `swf_tag` can
    // borrow it.
    let data;
    let mut place_object_data;
    let swf_tag = match tag {
        Tag::ShowFrame => swf::Tag::ShowFrame,
        Tag::End => swf::Tag::End,
        Tag::PlaceObject { .. } | Tag::PlaceObject2 { .. } => {
            place_object_data = PlaceObjectData::default();
            let place_object =
                place_object::convert(tag, swf_version, compiler, &mut place_object_data)?;
            swf::Tag::PlaceObject(Box::new(place_object))
        }
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
        Tag::Metadata(metadata) => {
            data = read_data(metadata, swf_version, compiler)?;
            swf::Tag::Metadata(null_terminated_bytes(&data)?)
        }
        Tag::JpegTables(tables) => {
            data = read_data(tables, swf_version, compiler)?;
            swf::Tag::JpegTables(&data)
        }
        Tag::SetBackgroundColor(rgb) => {
            if *rgb > 0xFFFFFF {
                bail!("background color {rgb:#x} is not in the 0xRRGGBB format");
            }
            swf::Tag::SetBackgroundColor(Color::from_rgb(*rgb, 255))
        }
        Tag::RemoveObject { character, depth } => swf::Tag::RemoveObject(RemoveObject {
            depth: *depth,
            character_id: Some(*character),
        }),
        Tag::DefineBits { id, data: jpeg } => {
            data = read_data(jpeg, swf_version, compiler)?;
            swf::Tag::DefineBits {
                id: *id,
                jpeg_data: &data,
            }
        }
        Tag::DefineButton {
            id,
            records,
            actions,
        } => {
            data = read_data(actions, swf_version, compiler).context("actions")?;
            swf::Tag::DefineButton(Box::new(button::convert_define_button(
                *id, records, &data,
            )?))
        }
        Tag::DoAction(action_data) => {
            data = read_data(action_data, swf_version, compiler)?;
            swf::Tag::DoAction(&data)
        }
        Tag::DefineBitsJpeg2 { id, data: jpeg } => {
            data = read_data(jpeg, swf_version, compiler)?;
            swf::Tag::DefineBitsJpeg2 {
                id: *id,
                jpeg_data: &data,
            }
        }
        Tag::Protect { password } => swf::Tag::Protect(
            password
                .as_deref()
                .map(null_terminated_str)
                .transpose()
                .context("password")?,
        ),
        Tag::RemoveObject2 { depth } => swf::Tag::RemoveObject(RemoveObject {
            depth: *depth,
            character_id: None,
        }),
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
            write_tags(tags, swf_version, compiler, &mut body)?;
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
        Tag::NameCharacter { character, name } => swf::Tag::NameCharacter(NameCharacter {
            id: *character,
            name: null_terminated_str(name).context("name")?,
        }),
        Tag::ProductInfo {
            product_id,
            edition,
            major_version,
            minor_version,
            build_number,
            compilation_date,
        } => swf::Tag::ProductInfo(ProductInfo {
            product_id: *product_id,
            edition: *edition,
            major_version: *major_version,
            minor_version: *minor_version,
            build_number: *build_number,
            compilation_date: *compilation_date,
        }),
        Tag::FrameLabel(frame_label) => {
            // TODO Maybe add support to the swf crate? It silently drops the
            // anchor flag before SWF 6, and can only write it as 1.
            if frame_label.anchor && swf_version < 6 {
                bail!("anchor: named anchors are not supported before SWF 6");
            }
            swf::Tag::FrameLabel(FrameLabel {
                label: null_terminated_str(&frame_label.label).context("label")?,
                is_anchor: frame_label.anchor,
            })
        }
        Tag::ExportAssets(assets) => {
            // TODO Maybe add support to the swf crate? It silently truncates
            // the count, and can't write a count that doesn't match the assets.
            if u16::try_from(assets.len()).is_err() {
                bail!("too many assets: {}, at most 65535", assets.len());
            }
            let assets = assets
                .iter()
                .enumerate()
                .map(|(i, asset)| {
                    Ok(ExportedAsset {
                        id: asset.character,
                        name: null_terminated_str(&asset.name)
                            .with_context(|| format!("asset #{i}: name"))?,
                    })
                })
                .collect::<Result<_>>()?;
            swf::Tag::ExportAssets(assets)
        }
        Tag::DoInitAction { sprite, actions } => {
            data = read_data(actions, swf_version, compiler)?;
            swf::Tag::DoInitAction {
                id: *sprite,
                action_data: &data,
            }
        }
        Tag::DefineVideoStream(stream) => {
            swf::Tag::DefineVideoStream(video::convert_define_video_stream(stream))
        }
        Tag::VideoFrame(frame) => {
            data = read_data(&frame.data, swf_version, compiler)?;
            swf::Tag::VideoFrame(video::convert_video_frame(frame, &data))
        }
        Tag::ScriptLimits {
            max_recursion_depth,
            timeout_seconds,
        } => swf::Tag::ScriptLimits {
            max_recursion_depth: *max_recursion_depth,
            timeout_in_seconds: *timeout_seconds,
        },
        Tag::DebugId(debug_id) => {
            data = read_data(debug_id, swf_version, compiler)?;
            let debug_id = <[u8; 16]>::try_from(&data[..])
                .map_err(|_| anyhow!("expected 16 bytes, got {}", data.len()))?;
            swf::Tag::DebugId(debug_id)
        }
        Tag::SetTabIndex { depth, tab_index } => swf::Tag::SetTabIndex {
            depth: *depth,
            tab_index: *tab_index,
        },
        Tag::DoAbc(abc) => {
            data = read_data(abc, swf_version, compiler)?;
            swf::Tag::DoAbc(&data)
        }
        Tag::CsmTextSettings {
            text,
            advanced_rendering,
            grid_fit,
            thickness,
            sharpness,
        } => swf::Tag::CsmTextSettings(CsmTextSettings {
            id: *text,
            use_advanced_rendering: *advanced_rendering,
            grid_fit: match grid_fit {
                GridFit::None => TextGridFit::None,
                GridFit::Pixel => TextGridFit::Pixel,
                GridFit::SubPixel => TextGridFit::SubPixel,
            },
            thickness: *thickness,
            sharpness: *sharpness,
        }),
        Tag::SymbolClass(links) => {
            // TODO Maybe add support to the swf crate? It silently truncates
            // the count, and can't write a count that doesn't match the links.
            if u16::try_from(links.len()).is_err() {
                bail!("too many links: {}, at most 65535", links.len());
            }
            let links = links
                .iter()
                .enumerate()
                .map(|(i, link)| {
                    Ok(SymbolClassLink {
                        id: link.character,
                        class_name: null_terminated_str(&link.name)
                            .with_context(|| format!("link #{i}: name"))?,
                    })
                })
                .collect::<Result<_>>()?;
            swf::Tag::SymbolClass(links)
        }
        Tag::DefineScalingGrid {
            character,
            splitter,
        } => swf::Tag::DefineScalingGrid {
            id: *character,
            splitter_rect: rectangle(splitter).context("splitter")?,
        },
        Tag::DoAbc2 {
            lazy_initialize,
            name,
            data: abc,
        } => {
            let mut flags = DoAbc2Flag::empty();
            flags.set(DoAbc2Flag::LAZY_INITIALIZE, *lazy_initialize);
            let name = null_terminated_str(name).context("name")?;
            data = read_data(abc, swf_version, compiler)?;
            swf::Tag::DoAbc2(DoAbc2 {
                flags,
                name,
                data: &data,
            })
        }
        Tag::DefineSceneAndFrameLabelData {
            scenes,
            frame_labels,
        } => swf::Tag::DefineSceneAndFrameLabelData(DefineSceneAndFrameLabelData {
            scenes: frame_label_data(scenes).context("scenes")?,
            frame_labels: frame_label_data(frame_labels).context("frame_labels")?,
        }),
        Tag::DefineBinaryData {
            id,
            data: binary_data,
        } => {
            data = read_data(binary_data, swf_version, compiler)?;
            swf::Tag::DefineBinaryData(DefineBinaryData {
                id: *id,
                data: &data,
            })
        }
        Tag::DefineFontName {
            font,
            name,
            copyright,
        } => swf::Tag::DefineFontName {
            id: *font,
            name: null_terminated_str(name).context("name")?,
            copyright_info: null_terminated_str(copyright).context("copyright")?,
        },
        Tag::Raw {
            code,
            data: body,
            force_long: false,
        } => {
            check_tag_code(*code)?;
            data = read_data(body, swf_version, compiler)?;
            swf::Tag::Unknown {
                tag_code: *code,
                data: &data,
            }
        }
        Tag::Raw {
            code,
            data: body,
            force_long: true,
        } => {
            check_tag_code(*code)?;
            data = read_data(body, swf_version, compiler)?;

            // TODO Maybe add support to the swf crate?
            let length = u32::try_from(data.len()).context("tag body too long")?;
            output.extend_from_slice(&(code << 6 | 0b111111).to_le_bytes());
            output.extend_from_slice(&length.to_le_bytes());
            output.extend_from_slice(&data);
            return Ok(());
        }
    };

    swf::write::write_tag(&swf_tag, swf_version, output)?;
    Ok(())
}

/// Returns the bytes of binary data, reading them from the files of the
/// compiler or compiling them if needed.
fn read_data<'a>(data: &'a Data, swf_version: u8, compiler: &Compiler) -> Result<Cow<'a, [u8]>> {
    match data {
        Data::InlineBytes(bytes) => Ok(Cow::Borrowed(&bytes.0)),
        Data::File(path) => compiler
            .files
            .read(path)
            .map(Cow::Owned)
            .with_context(|| format!("failed to read file \"{path}\"")),
        Data::Rascal(script) => cfg_select! {
            feature = "rascal" => {
                crate::rascal::compile(script, swf_version, compiler.files)
                    .map(Cow::Owned)
                    .context("failed to compile the ActionScript")
            }
            _ => {{
                let _ = (script, swf_version);
                bail!("Rascal is not available")
            }}
        },
        Data::Asc(script) => cfg_select! {
            feature = "asc" => {{
                let Some(config) = &compiler.asc else {
                    bail!("asc is not configured")
                };
                crate::asc::compile(script, config, compiler.files)
                    .map(Cow::Owned)
                    .context("failed to compile the ActionScript")
            }}
            _ => {{
                let _ = script;
                bail!("asc is not available")
            }}
        },
    }
}

/// Fails if the tag code does not fit in the 10 bits of a tag header.
fn check_tag_code(code: u16) -> Result<()> {
    if code >= 1 << 10 {
        bail!("tag code {code} does not fit in 10 bits");
    }
    Ok(())
}

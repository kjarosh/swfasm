//! Converts the `DefineFont` tag for the swf crate.

use super::shape::convert_shape_record;
use crate::source::Shape;
use anyhow::{Context, Result, bail};
use swf::FontV1;

/// Converts a `DefineFont` tag into an `swf::FontV1`.
pub(super) fn convert_define_font(id: u16, glyphs: &[Shape], swf_version: u8) -> Result<FontV1> {
    let glyphs = glyphs
        .iter()
        .enumerate()
        .map(|(i, glyph)| {
            glyph
                .records
                .iter()
                .enumerate()
                .map(|(j, record)| {
                    // Glyphs are written with 1 bit of fill style indices and
                    // no bits of line style indices.
                    // TODO Maybe add support to the swf crate? It always
                    // writes these numbers of bits, though `SHAPE` stores them.
                    convert_shape_record(record, 1, 0)
                        .with_context(|| format!("records: record #{j}"))
                })
                .collect::<Result<Vec<_>>>()
                .with_context(|| format!("glyphs: glyph #{i}"))
        })
        .collect::<Result<Vec<_>>>()?;
    let font = FontV1 { id, glyphs };
    check_offsets(&font, swf_version)?;
    Ok(font)
}

/// Fails if the offset of a glyph does not fit in the 16 bits of the offset
/// table.
// TODO Maybe add support to the swf crate? It silently truncates the offsets.
fn check_offsets(font: &FontV1, swf_version: u8) -> Result<()> {
    // Offsets count bytes from the start of the offset table, which has 2
    // bytes per glyph and is followed by the glyphs.
    let mut offset = 2 * font.glyphs.len();
    for (i, glyph) in font.glyphs.iter().enumerate() {
        if offset > 0xFFFF {
            bail!("glyphs: glyph #{i} starts at offset {offset}, which does not fit in 16 bits");
        }
        // The size of the glyph, from a font with only that glyph, whose body
        // is the ID, one offset and the glyph.
        let single = FontV1 {
            id: font.id,
            glyphs: vec![glyph.clone()],
        };
        let mut tag = Vec::new();
        swf::write::write_tag(&swf::Tag::DefineFont(single), swf_version, &mut tag)?;
        let (_, length) = swf::read::Reader::new(&tag, swf_version).read_tag_code_and_length()?;
        offset += length - 4;
    }
    Ok(())
}

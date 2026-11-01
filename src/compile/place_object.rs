//! Converts the `PlaceObject` tags for the swf crate.
//!
//! The values are checked here, so that errors point to the invalid field.

use super::{fixed8, fixed16, twips};
use crate::source::{ColorTransform, Matrix, Tag};
use anyhow::{Context, Result, bail};
use std::fmt::Display;
use swf::{Fixed8, Fixed16, PlaceObject, PlaceObjectAction};

/// Converts a `PlaceObject` tag of any version into an `swf::PlaceObject`.
pub(super) fn convert(tag: &Tag) -> Result<PlaceObject<'_>> {
    Ok(match tag {
        Tag::PlaceObject {
            character,
            depth,
            matrix,
            color_transform,
        } => {
            if let Some(color_transform) = color_transform
                && (color_transform.alpha_mult != 1.0 || color_transform.alpha_add != 0)
            {
                bail!("color_transform: PlaceObject cannot encode non-default alpha terms");
            }
            PlaceObject {
                version: 1,
                action: PlaceObjectAction::Place(*character),
                depth: *depth,
                matrix: Some(convert_matrix(matrix).context("matrix")?),
                color_transform: color_transform
                    .as_ref()
                    .map(convert_color_transform)
                    .transpose()
                    .context("color_transform")?,
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
        _ => panic!("not a PlaceObject tag: {tag:?}"),
    })
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

fn convert_matrix(matrix: &Matrix) -> Result<swf::Matrix> {
    // Values are stored in at most 31 bits, as their bit count is stored in 5
    // bits.
    let fixed = |value: f64| -> Result<Fixed16> {
        let fixed = fixed16(value)?;
        check_sbits(fixed.get(), 31, value)?;
        Ok(fixed)
    };
    let translate = |pixels: f64| -> Result<swf::Twips> {
        let twips = twips(pixels)?;
        check_sbits(twips.get(), 31, format!("{pixels}px"))?;
        Ok(twips)
    };
    Ok(swf::Matrix {
        a: fixed(matrix.scale_x).context("scale_x")?,
        b: fixed(matrix.rotate_skew0).context("rotate_skew0")?,
        c: fixed(matrix.rotate_skew1).context("rotate_skew1")?,
        d: fixed(matrix.scale_y).context("scale_y")?,
        tx: translate(matrix.translate_x).context("translate_x")?,
        ty: translate(matrix.translate_y).context("translate_y")?,
    })
}

fn convert_color_transform(color_transform: &ColorTransform) -> Result<swf::ColorTransform> {
    // Values are stored in at most 15 bits, as their bit count is stored in 4
    // bits.
    let mult = |value: f64| -> Result<Fixed8> {
        let fixed = fixed8(value)?;
        check_sbits(fixed.get().into(), 15, value)?;
        Ok(fixed)
    };
    let add = |value: i16| -> Result<i16> {
        check_sbits(value.into(), 15, value)?;
        Ok(value)
    };
    Ok(swf::ColorTransform {
        r_multiply: mult(color_transform.red_mult).context("red_mult")?,
        g_multiply: mult(color_transform.green_mult).context("green_mult")?,
        b_multiply: mult(color_transform.blue_mult).context("blue_mult")?,
        a_multiply: mult(color_transform.alpha_mult).context("alpha_mult")?,
        r_add: add(color_transform.red_add).context("red_add")?,
        g_add: add(color_transform.green_add).context("green_add")?,
        b_add: add(color_transform.blue_add).context("blue_add")?,
        a_add: add(color_transform.alpha_add).context("alpha_add")?,
    })
}

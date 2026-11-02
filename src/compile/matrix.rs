//! Converts matrices for the swf crate.
//!
//! The values are checked here, so that errors point to the invalid field.

use super::{check_sbits, fixed16, twips};
use crate::source::Matrix;
use anyhow::{Context, Result};
use swf::Fixed16;

pub(super) fn convert_matrix(matrix: &Matrix) -> Result<swf::Matrix> {
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

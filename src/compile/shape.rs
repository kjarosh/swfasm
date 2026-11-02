//! Converts the `DefineShape` tag for the swf crate.
//!
//! The values are checked here, so that errors point to the invalid field.

use super::matrix::convert_matrix;
use super::{fixed8, rgb, twips, twips_in_bits};
use crate::source::{
    FillStyle, Gradient, GradientInterpolation, GradientSpread, LineStyle, Rectangle, Shape,
    ShapeRecord,
};
use anyhow::{Context, Result, bail};
use swf::{PointDelta, ShapeFlag, ShapeStyles, StyleChangeData, Twips};

/// Converts a `DefineShape` tag into an `swf::Shape`.
pub(super) fn convert(shape: &Shape, swf_version: u8) -> Result<swf::Shape> {
    // TODO Maybe add support to the swf crate? It writes 255 and more fill
    // styles with an extended count, which `DefineShape` doesn't have.
    if shape.fill_styles.len() > 254 {
        bail!("fill_styles: DefineShape can have at most 254 fill styles");
    }
    if shape.line_styles.len() > 0xFFFF {
        bail!("line_styles: there can be at most 65535 line styles");
    }
    let fill_styles = shape
        .fill_styles
        .iter()
        .enumerate()
        .map(|(i, fill_style)| {
            convert_fill_style(fill_style, swf_version)
                .with_context(|| format!("fill_styles: fill style #{i}"))
        })
        .collect::<Result<Vec<_>>>()?;
    let line_styles = shape
        .line_styles
        .iter()
        .enumerate()
        .map(|(i, line_style)| {
            convert_line_style(line_style).with_context(|| format!("line_styles: line style #{i}"))
        })
        .collect::<Result<Vec<_>>>()?;

    // Style indices are stored in as many bits as needed for the number of
    // styles.
    let num_fill_bits = num_bits(fill_styles.len());
    let num_line_bits = num_bits(line_styles.len());
    let records = shape
        .records
        .iter()
        .enumerate()
        .map(|(i, record)| {
            convert_shape_record(record, num_fill_bits, num_line_bits)
                .with_context(|| format!("records: record #{i}"))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(swf::Shape {
        version: 1,
        id: shape.id,
        shape_bounds: convert_rectangle(&shape.bounds).context("bounds")?,
        // Not written in `DefineShape`.
        edge_bounds: swf::Rectangle::default(),
        flags: ShapeFlag::empty(),
        styles: ShapeStyles {
            fill_styles,
            line_styles,
        },
        shape: records,
    })
}

/// Returns the number of bits needed to store `n` as an unsigned number.
fn num_bits(n: usize) -> u32 {
    usize::BITS - n.leading_zeros()
}

fn convert_rectangle(rectangle: &Rectangle) -> Result<swf::Rectangle<Twips>> {
    // Values are stored in at most 31 bits, as their bit count is stored in 5
    // bits.
    Ok(swf::Rectangle {
        x_min: twips_in_bits(rectangle.x_min, 31).context("x_min")?,
        x_max: twips_in_bits(rectangle.x_max, 31).context("x_max")?,
        y_min: twips_in_bits(rectangle.y_min, 31).context("y_min")?,
        y_max: twips_in_bits(rectangle.y_max, 31).context("y_max")?,
    })
}

fn convert_fill_style(fill_style: &FillStyle, swf_version: u8) -> Result<swf::FillStyle> {
    Ok(match fill_style {
        FillStyle::Solid(color) => swf::FillStyle::Color(rgb(*color)?),
        FillStyle::LinearGradient(gradient) => {
            swf::FillStyle::LinearGradient(convert_gradient(gradient)?)
        }
        FillStyle::RadialGradient(gradient) => {
            swf::FillStyle::RadialGradient(convert_gradient(gradient)?)
        }
        FillStyle::FocalGradient {
            gradient,
            focal_point,
        } => swf::FillStyle::FocalGradient {
            gradient: convert_gradient(gradient).context("gradient")?,
            focal_point: fixed8(*focal_point).context("focal_point")?,
        },
        FillStyle::Bitmap {
            character,
            matrix,
            repeating,
            smoothed,
        } => {
            // TODO Maybe add support to the swf crate? It writes 0x40 and 0x41
            // instead of 0x42 and 0x43 before SWF 8.
            if !smoothed && swf_version < 8 {
                bail!("smoothed: non-smoothed bitmap fills cannot be written before SWF 8");
            }
            swf::FillStyle::Bitmap {
                id: *character,
                matrix: convert_matrix(matrix).context("matrix")?,
                is_smoothed: *smoothed,
                is_repeating: *repeating,
            }
        }
    })
}

fn convert_gradient(gradient: &Gradient) -> Result<swf::Gradient> {
    // The count of records is stored in 4 bits.
    if gradient.records.len() > 15 {
        bail!("records: there can be at most 15 gradient records");
    }
    let records = gradient
        .records
        .iter()
        .enumerate()
        .map(|(i, record)| {
            Ok(swf::GradientRecord {
                ratio: record.ratio,
                color: rgb(record.color)
                    .context("color")
                    .with_context(|| format!("records: record #{i}"))?,
            })
        })
        .collect::<Result<_>>()?;
    Ok(swf::Gradient {
        matrix: convert_matrix(&gradient.matrix).context("matrix")?,
        spread: match gradient.spread {
            GradientSpread::Pad => swf::GradientSpread::Pad,
            GradientSpread::Reflect => swf::GradientSpread::Reflect,
            GradientSpread::Repeat => swf::GradientSpread::Repeat,
        },
        interpolation: match gradient.interpolation {
            GradientInterpolation::Rgb => swf::GradientInterpolation::Rgb,
            GradientInterpolation::LinearRgb => swf::GradientInterpolation::LinearRgb,
        },
        records,
    })
}

fn convert_line_style(line_style: &LineStyle) -> Result<swf::LineStyle> {
    // The width is stored as a 16-bit unsigned number.
    let width = twips(line_style.width).context("width")?;
    if !(0..=0xFFFF).contains(&width.get()) {
        bail!("width: {}px is out of range", line_style.width);
    }
    Ok(swf::LineStyle::new()
        .with_width(width)
        .with_color(rgb(line_style.color).context("color")?))
}

fn convert_shape_record(
    record: &ShapeRecord,
    num_fill_bits: u32,
    num_line_bits: u32,
) -> Result<swf::ShapeRecord> {
    // Deltas of edges are stored in at most 17 bits, as their bit count minus
    // 2 is stored in 4 bits.
    let delta = |dx: f64, dy: f64, dx_name: &str, dy_name: &str| -> Result<PointDelta<Twips>> {
        Ok(PointDelta::new(
            twips_in_bits(dx, 17).context(dx_name.to_owned())?,
            twips_in_bits(dy, 17).context(dy_name.to_owned())?,
        ))
    };
    Ok(match record {
        ShapeRecord::StyleChange {
            move_to,
            fill_style_0,
            fill_style_1,
            line_style,
        } => {
            let move_to = move_to
                .as_ref()
                .map(|point| -> Result<_> {
                    // Values are stored in at most 31 bits, as their bit count
                    // is stored in 5 bits.
                    Ok(swf::Point::new(
                        twips_in_bits(point.x, 31).context("x")?,
                        twips_in_bits(point.y, 31).context("y")?,
                    ))
                })
                .transpose()
                .context("move_to")?;
            let fill_index = |index: Option<u16>| {
                index
                    .map(|index| style_index(index, num_fill_bits, "fill"))
                    .transpose()
            };
            swf::ShapeRecord::StyleChange(Box::new(StyleChangeData {
                move_to,
                fill_style_0: fill_index(*fill_style_0).context("fill_style_0")?,
                fill_style_1: fill_index(*fill_style_1).context("fill_style_1")?,
                line_style: line_style
                    .map(|index| style_index(index, num_line_bits, "line"))
                    .transpose()
                    .context("line_style")?,
                new_styles: None,
            }))
        }
        ShapeRecord::StraightEdge { dx, dy } => swf::ShapeRecord::StraightEdge {
            delta: delta(*dx, *dy, "dx", "dy")?,
        },
        ShapeRecord::CurvedEdge {
            control_dx,
            control_dy,
            anchor_dx,
            anchor_dy,
        } => swf::ShapeRecord::CurvedEdge {
            control_delta: delta(*control_dx, *control_dy, "control_dx", "control_dy")?,
            anchor_delta: delta(*anchor_dx, *anchor_dy, "anchor_dx", "anchor_dy")?,
        },
    })
}

/// Fails if a style index does not fit in the bits of style indices.
fn style_index(index: u16, num_bits: u32, kind: &str) -> Result<u32> {
    let index = u32::from(index);
    if index >> num_bits != 0 {
        bail!("{index} does not fit in the {num_bits} bits of {kind} style indices");
    }
    Ok(index)
}

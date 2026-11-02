//! Converts the `DefineButton` tag for the swf crate.
//!
//! The values are checked here, so that errors point to the invalid field.

use super::matrix::convert_matrix;
use crate::source::ButtonRecord;
use anyhow::{Context, Result};
use swf::{BlendMode, Button, ButtonAction, ButtonActionCondition, ButtonState, ColorTransform};

/// Converts a `DefineButton` tag into an `swf::Button`, given the data of its
/// actions.
pub(super) fn convert_define_button<'a>(
    id: u16,
    records: &[ButtonRecord],
    actions: &'a [u8],
) -> Result<Button<'a>> {
    Ok(Button {
        id,
        // Not written in `DefineButton`.
        is_track_as_menu: false,
        records: records
            .iter()
            .enumerate()
            .map(|(i, record)| {
                convert_record(record).with_context(|| format!("records: record #{i}"))
            })
            .collect::<Result<_>>()?,
        // In `DefineButton`, the swf crate writes only the data of the first
        // action.
        actions: vec![ButtonAction {
            conditions: ButtonActionCondition::empty(),
            action_data: actions,
        }],
    })
}

fn convert_record(record: &ButtonRecord) -> Result<swf::ButtonRecord> {
    let flags = record
        .states
        .iter()
        .fold(0, |flags, &state| flags | state as u8);
    Ok(swf::ButtonRecord {
        states: ButtonState::from_bits_retain(flags),
        id: record.character,
        depth: record.depth,
        matrix: convert_matrix(&record.matrix).context("matrix")?,
        // Not written in `DefineButton`.
        color_transform: ColorTransform::IDENTITY,
        filters: Vec::new(),
        blend_mode: BlendMode::Normal,
    })
}

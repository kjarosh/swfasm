//! Converts the `PlaceObject` tags for the swf crate.
//!
//! The values are checked here, so that errors point to the invalid field.

use super::matrix::convert_matrix;
use super::{Compiler, check_sbits, fixed8, null_terminated_str, read_data};
use crate::source::{ClipAction, ClipEvent, ColorTransform, Tag};
use anyhow::{Context, Result, bail};
use std::borrow::Cow;
use swf::{ClipEventFlag, Fixed8, PlaceObject, PlaceObjectAction};

/// Data read for a `PlaceObject` tag, which the converted tag borrows.
#[derive(Default)]
pub(super) struct PlaceObjectData<'a> {
    clip_actions: Vec<Cow<'a, [u8]>>,
}

/// Converts a `PlaceObject` tag of any version into an `swf::PlaceObject`.
///
/// The data that the `swf::PlaceObject` refers to, is read into `data`.
pub(super) fn convert<'a>(
    tag: &'a Tag,
    swf_version: u8,
    compiler: &Compiler,
    data: &'a mut PlaceObjectData<'a>,
) -> Result<PlaceObject<'a>> {
    if let Tag::PlaceObject2 {
        clip_actions: Some(clip_actions),
        ..
    } = tag
    {
        data.clip_actions = clip_actions
            .iter()
            .enumerate()
            .map(|(i, clip_action)| {
                read_data(&clip_action.actions, swf_version, compiler)
                    .with_context(|| format!("clip_actions: clip action #{i}"))
            })
            .collect::<Result<_>>()?;
    }
    let data: &'a PlaceObjectData = data;

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
        Tag::PlaceObject2 {
            depth,
            r#move,
            character,
            matrix,
            color_transform,
            ratio,
            name,
            clip_depth,
            clip_actions,
            all_clip_events,
        } => PlaceObject {
            version: 2,
            action: match (r#move, character) {
                (false, Some(character)) => PlaceObjectAction::Place(*character),
                (true, None) => PlaceObjectAction::Modify,
                (true, Some(character)) => PlaceObjectAction::Replace(*character),
                (false, None) => PlaceObjectAction::None,
            },
            depth: *depth,
            matrix: matrix
                .as_ref()
                .map(convert_matrix)
                .transpose()
                .context("matrix")?,
            color_transform: color_transform
                .as_ref()
                .map(convert_color_transform)
                .transpose()
                .context("color_transform")?,
            ratio: *ratio,
            name: name
                .as_deref()
                .map(null_terminated_str)
                .transpose()
                .context("name")?,
            clip_depth: *clip_depth,
            class_name: None,
            filters: None,
            background_color: None,
            blend_mode: None,
            clip_actions: convert_clip_actions(
                clip_actions.as_deref(),
                all_clip_events.as_deref(),
                &data.clip_actions,
                swf_version,
            )?,
            has_image: false,
            is_bitmap_cached: None,
            is_visible: None,
            amf_data: None,
        },
        _ => panic!("not a PlaceObject tag: {tag:?}"),
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

/// Converts the clip actions of a tag, given the data of each of them, with
/// the events that override `AllEventFlags`.
fn convert_clip_actions<'a>(
    clip_actions: Option<&[ClipAction]>,
    all_clip_events: Option<&[ClipEvent]>,
    data: &'a [Cow<'a, [u8]>],
    swf_version: u8,
) -> Result<Option<swf::ClipActions<'a>>> {
    let clip_actions = match (clip_actions, all_clip_events) {
        (Some(clip_actions), _) => clip_actions,
        // `AllEventFlags` is a part of clip actions, so they are written
        // without any.
        (None, Some(_)) => &[],
        (None, None) => return Ok(None),
    };
    let records = clip_actions
        .iter()
        .zip(data)
        .enumerate()
        .map(|(i, (clip_action, data))| {
            convert_clip_action(clip_action, data, swf_version)
                .with_context(|| format!("clip_actions: clip action #{i}"))
        })
        .collect::<Result<Vec<_>>>()?;
    let all_event_flags = match all_clip_events {
        Some(events) => clip_event_flags(events, swf_version).context("all_clip_events")?,
        None => records
            .iter()
            .fold(ClipEventFlag::empty(), |flags, record| {
                flags | record.events
            }),
    };
    Ok(Some(swf::ClipActions {
        all_event_flags,
        records,
    }))
}

fn convert_clip_action<'a>(
    clip_action: &ClipAction,
    data: &'a [u8],
    swf_version: u8,
) -> Result<swf::ClipAction<'a>> {
    Ok(swf::ClipAction {
        events: clip_event_flags(&clip_action.events, swf_version)?,
        key_code: clip_action.key_code,
        action_data: data,
    })
}

/// Converts events to their flags, failing for events that are not supported
/// in the SWF version.
fn clip_event_flags(events: &[ClipEvent], swf_version: u8) -> Result<ClipEventFlag> {
    let mut flags = ClipEventFlag::empty();
    for &event in events {
        let flag = event as u32;
        if swf_version < 6 && flag > 0xFFFF {
            bail!("{event:?} events are not supported before SWF 6");
        }
        flags |= ClipEventFlag::from_bits_retain(flag);
    }
    Ok(flags)
}

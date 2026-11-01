//! Converts the `PlaceObject` tags for the swf crate.
//!
//! The values are checked here, so that errors point to the invalid field.

use super::{fixed8, fixed16, read_data, twips};
use crate::files::FileProvider;
use crate::source::{ClipAction, ClipEvent, ColorTransform, Matrix, Tag};
use anyhow::{Context, Result, bail};
use std::borrow::Cow;
use std::fmt::Display;
use swf::{ClipEventFlag, Fixed8, Fixed16, PlaceObject, PlaceObjectAction, SwfStr};

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
    files: &dyn FileProvider,
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
                read_data(&clip_action.actions, swf_version, files)
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
            name: name.as_deref().map(swf_str).transpose().context("name")?,
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

/// Converts a string to a null-terminated `SwfStr`, failing if it contains a
/// null character.
fn swf_str(string: &str) -> Result<&SwfStr> {
    if string.contains('\0') {
        bail!("{string:?} contains a null character");
    }
    Ok(SwfStr::from_utf8_str(string))
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

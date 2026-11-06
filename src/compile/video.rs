//! Converts the video tags for the swf crate.

use crate::source::{DefineVideoStream, VideoCodec, VideoDeblocking};

/// Converts a `DefineVideoStream` tag into an `swf::DefineVideoStream`.
pub(super) fn convert_define_video_stream(stream: &DefineVideoStream) -> swf::DefineVideoStream {
    swf::DefineVideoStream {
        id: stream.id,
        num_frames: stream.num_frames,
        width: stream.width,
        height: stream.height,
        is_smoothed: stream.smoothing,
        deblocking: convert_deblocking(stream.deblocking),
        codec: convert_codec(stream.codec),
    }
}

fn convert_deblocking(deblocking: VideoDeblocking) -> swf::VideoDeblocking {
    match deblocking {
        VideoDeblocking::UseVideoPacketValue => swf::VideoDeblocking::UseVideoPacketValue,
        VideoDeblocking::Off => swf::VideoDeblocking::None,
        VideoDeblocking::Level1 => swf::VideoDeblocking::Level1,
        VideoDeblocking::Level2 => swf::VideoDeblocking::Level2,
        VideoDeblocking::Level3 => swf::VideoDeblocking::Level3,
        VideoDeblocking::Level4 => swf::VideoDeblocking::Level4,
    }
}

fn convert_codec(codec: VideoCodec) -> swf::VideoCodec {
    match codec {
        VideoCodec::None => swf::VideoCodec::None,
        VideoCodec::H263 => swf::VideoCodec::H263,
        VideoCodec::ScreenVideo => swf::VideoCodec::ScreenVideo,
        VideoCodec::Vp6 => swf::VideoCodec::Vp6,
        VideoCodec::Vp6WithAlpha => swf::VideoCodec::Vp6WithAlpha,
        VideoCodec::ScreenVideoV2 => swf::VideoCodec::ScreenVideoV2,
        VideoCodec::H264 => swf::VideoCodec::H264,
    }
}

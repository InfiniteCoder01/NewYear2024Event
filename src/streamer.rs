extern crate gstreamer as gst;
extern crate gstreamer_app as gst_app;
extern crate gstreamer_video as gst_video;

use crate::renderer::*;
use gst::{prelude::*, Caps, ElementFactory};
use pango::prelude::FontMapExt;

struct DrawingContext {
    layout: LayoutWrapper,
    info: Option<gst_video::VideoInfo>,
}

#[derive(Debug)]
struct LayoutWrapper(pango::Layout);

impl std::ops::Deref for LayoutWrapper {
    type Target = pango::Layout;

    fn deref(&self) -> &pango::Layout {
        assert_eq!(self.0.ref_count(), 1);
        &self.0
    }
}

// SAFETY: We ensure that there are never multiple references to the layout.
unsafe impl Send for LayoutWrapper {}

pub fn stream(
    size: (usize, usize),
    fps: usize,
    audio_bitrate: usize,
    rtmp_uri: &str,
    mut draw_frame: impl FnMut(&mut Frame) + Send + Sync + 'static,
) {
    // let pipeline_str = format!(
    //     concat!(
    //         "appsrc caps=\"video/x-raw,format=RGB,width={},height={},framerate={}/1\" name=appsrc0 ! ",
    //         "videoconvert ! video/x-raw, format=I420, width={}, height={}, framerate={}/1 ! ",
    //         "x264enc ! h264parse ! ",
    //         "flvmux streamable=true name=mux ! ",
    //         "rtmpsink location={} ",
    //         "audiotestsrc ! voaacenc bitrate=128000 ! mux."
    //     ),
    //     width, height, fps,
    //     width, height, fps,
    //     rtmp_uri
    // );

    gst::init().unwrap();
    let pipeline = gst::Pipeline::default();

    let (enc, parse, cvt) = ("x264enc", "h264parse", "v4l2convert");

    // * Source
    let (width, height) = size;
    let video_info =
        gst_video::VideoInfo::builder(gst_video::VideoFormat::Rgb, width as u32, height as u32)
            .fps(gst::Fraction::new(fps as _, 1))
            .build()
            .unwrap();
    let background = ElementFactory::make("videotestsrc")
        .property("caps", video_info)
        .property_from_str("pattern", "ball")
        .build()
        .unwrap();
    let video_overlay = ElementFactory::make("cairooverlay").build().unwrap();

    // * Convert
    let videoconvert = ElementFactory::make(cvt).build().unwrap();
    let caps_filter = ElementFactory::make("capsfilter")
        .property(
            "caps",
            Caps::builder("video/x-raw").field("format", "I420").build(),
        )
        .build()
        .unwrap();
    let video_encoder = ElementFactory::make(enc).build().unwrap();
    let video_decoder = ElementFactory::make(parse).build().unwrap();

    // * Mux
    let mux = ElementFactory::make("flvmux")
        .property("streamable", true)
        .build()
        .unwrap();

    // * Sink
    let rtmp_sink = ElementFactory::make("rtmpsink")
        .property("location", rtmp_uri)
        .build()
        .unwrap();

    // * Audio
    let audio_source = ElementFactory::make("audiotestsrc").build().unwrap();
    let audio_encoder = ElementFactory::make("voaacenc")
        .property("bitrate", audio_bitrate as i32)
        .build()
        .unwrap();

    // * Add
    pipeline
        .add_many([
            &background,
            &video_overlay,
            &videoconvert,
            &caps_filter,
            &video_encoder,
            &video_decoder,
            &mux,
            &rtmp_sink,
            &audio_source,
            &audio_encoder,
        ])
        .unwrap();

    // * Link video
    gst::Element::link_many([
        &background,
        &video_overlay,
        &videoconvert,
        &caps_filter,
        &video_encoder,
        &video_decoder,
        &mux,
        &rtmp_sink,
    ])
    .unwrap();

    // * Link audio
    gst::Element::link_many([&audio_source, &audio_encoder, &mux]).unwrap();

    // * Draw callback
    let fontmap = pangocairo::FontMap::new();
    let context = fontmap.create_context();
    let layout = LayoutWrapper(pango::Layout::new(&context));
    let font_desc = pango::FontDescription::from_string("Sans Bold 26");
    layout.set_font_description(Some(&font_desc));
    layout.set_text("GStreamer");
    let drawer = std::sync::Arc::new(std::sync::Mutex::new(DrawingContext { layout, info: None }));
    let drawer_clone = drawer.clone();
    video_overlay.connect("draw", false, move |args| {
        use std::f64::consts::PI;

        let drawer = &drawer_clone;
        let drawer = drawer.lock().unwrap();

        // Get the signal's arguments
        let _overlay = args[0].get::<gst::Element>().unwrap();
        // This is the cairo context. This is the root of all of cairo's
        // drawing functionality.
        let cr = args[1].get::<cairo::Context>().unwrap();
        let timestamp = args[2].get::<gst::ClockTime>().unwrap();
        let _duration = args[3].get::<gst::ClockTime>().unwrap();

        let info = drawer.info.as_ref().unwrap();
        let layout = &drawer.layout;

        let angle = 2.0 * PI * (timestamp % (10 * gst::ClockTime::SECOND)).nseconds() as f64
            / (10.0 * gst::ClockTime::SECOND.nseconds() as f64);

        // The image we draw (the text) will be static, but we will change the
        // transformation on the drawing context, which rotates and shifts everything
        // that we draw afterwards. Like this, we have no complicated calculations
        // in the actual drawing below.
        // Calling multiple transformation methods after each other will apply the
        // new transformation on top. If you repeat the cr.rotate(angle) line below
        // this a second time, everything in the canvas will rotate twice as fast.
        cr.translate(
            f64::from(info.width()) / 2.0,
            f64::from(info.height()) / 2.0,
        );
        cr.rotate(angle);

        // This loop will render 10 times the string "GStreamer" in a circle
        for i in 0..10 {
            // Cairo, like most rendering frameworks, is using a stack for transformations
            // with this, we push our current transformation onto this stack - allowing us
            // to make temporary changes / render something / and then returning to the
            // previous transformations.
            cr.save().expect("Failed to save state");

            let angle = (360. * f64::from(i)) / 10.0;
            let red = (1.0 + f64::cos((angle - 60.0) * PI / 180.0)) / 2.0;
            cr.set_source_rgb(red, 0.0, 1.0 - red);
            cr.rotate(angle * PI / 180.0);

            // Update the text layout. This function is only updating pango's internal state.
            // So e.g. that after a 90 degree rotation it knows that what was previously going
            // to end up as a 200x100 rectangle would now be 100x200.
            pangocairo::functions::update_layout(&cr, layout);
            let (width, _height) = layout.size();
            // Using width and height of the text, we can properly position it within
            // our canvas.
            cr.move_to(
                -(f64::from(width) / f64::from(pango::SCALE)) / 2.0,
                -(f64::from(info.height())) / 2.0,
            );
            // After telling the layout object where to draw itself, we actually tell
            // it to draw itself into our cairo context.
            pangocairo::functions::show_layout(&cr, layout);

            // Here we go one step up in our stack of transformations, removing any
            // changes we did to them since the last call to cr.save();
            cr.restore().expect("Failed to restore state");
        }

        None
    });

    video_overlay.connect("caps-changed", false, move |args| {
        let _overlay = args[0].get::<gst::Element>().unwrap();
        let caps = args[1].get::<gst::Caps>().unwrap();

        let mut drawer = drawer.lock().unwrap();
        drawer.info = Some(gst_video::VideoInfo::from_caps(&caps).unwrap());

        None
    });

    pipeline.set_state(gst::State::Playing).unwrap();

    for msg in pipeline.bus().unwrap().iter_timed(gst::ClockTime::NONE) {
        use gst::MessageView;

        match msg.view() {
            MessageView::Eos(..) => break,
            MessageView::Error(err) => {
                panic!("{}", err);
            }
            _ => (),
        }
    }

    pipeline.set_state(gst::State::Null).unwrap();
}

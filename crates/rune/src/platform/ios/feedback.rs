#![allow(unsafe_code)]

use std::ffi::c_void;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;

use crate::frame::encoded::{FrameReports, OnFrameEncoded};
use crate::frame::present_time::metal::{LayerHook, Log};
use crate::frame::presented::{FramePresentReports, OnFramePresented};
use crate::platform::embed_frame::FrameSummary;
use crate::runtime::Waker;
use crate::time::Instant;

pub(super) struct Feedback {
    reports: FrameReports,
    presented: FramePresentReports,
    log: Option<Arc<Log>>,
    _hook: Option<LayerHook>,
}

impl Feedback {
    pub(super) fn new(encoded: Option<OnFrameEncoded>, presented: Option<OnFramePresented>, window: &str, layer: *mut c_void, presents: Waker) -> Self {
        let mut presented = FramePresentReports::new(presented, Some(window));
        let reports = FrameReports::with_presented_seq(encoded, presented.wanted(), Some(window));
        let (mut log, mut hook) = (None, None);
        if presented.wanted() {
            // SAFETY: `layer` is the view's live `CAMetalLayer`, retained here for as long as the
            // hook keeps it.
            let layer = unsafe { Retained::retain(layer.cast::<AnyObject>()) };
            let timed = Arc::new(Log::new(false, true).with_wake(move || presents()));
            hook = layer.and_then(|layer| LayerHook::hook(layer, timed.clone()));
            if hook.is_some() {
                presented.set_source(timed.clone());
                log = Some(timed);
            }
        }
        Self { reports, presented, log, _hook: hook }
    }

    pub(super) fn frame(&mut self, summary: &FrameSummary, target: Instant, refresh_hz: Option<f64>) {
        if self.reports.wanted() { self.reports.set_refresh(refresh_hz); }
        let Some(drawn) = summary.drawn else {
            self.reports.skipped();
            return;
        };
        let at = self.reports.stamp().map(|_| drawn.submitted);
        let seq = self.reports.drawn(at, Some(target), drawn.started, drawn.update_ms);
        if let (Some(seq), Some(drawable)) = (seq, self.log.as_ref().and_then(|log| log.acquired())) {
            self.presented.push_and_drain(drawable, seq);
        }
    }

    pub(super) fn drain(&mut self) { self.presented.drain(); }

    pub(super) fn reset(&mut self) { self.presented.reset(); }
}

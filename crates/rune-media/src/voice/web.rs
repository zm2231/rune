use super::VoiceError;
use super::control::{Signal, SignalSender, VoiceDevice};
use super::duplex::{self, Heard, Render};
use super::inline;
use super::io::{Capture, Meters, Playback};
use super::machine::Fault;
use crate::recorder::MicPermission;
use js_sys::Float32Array;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::{Closure, wasm_bindgen};

#[wasm_bindgen(module = "/src/voice/web.js")]
extern "C" {
    #[wasm_bindgen(js_name = voiceSupported)]
    fn supported() -> bool;
    #[wasm_bindgen(js_name = voicePermission)]
    fn permission_state() -> String;
    #[wasm_bindgen(js_name = voiceRequestPermission)]
    fn request(done: &JsValue);
    #[wasm_bindgen(js_name = voiceWakeAfter)]
    pub(super) fn wake_after(ms: f64, callback: &JsValue);
    #[wasm_bindgen(js_name = voiceOpen)]
    fn open(rate: u32, frames: &Closure<dyn FnMut(Float32Array, u32) -> Float32Array>, ended: &Closure<dyn FnMut(String, String)>) -> JsValue;
    #[wasm_bindgen(js_name = voiceClose)]
    fn close(session: &JsValue);
}

fn answer(state: &str) -> MicPermission {
    match state {
        "granted" => MicPermission::Granted,
        "denied" => MicPermission::Denied,
        "restricted" => MicPermission::Restricted,
        _ => MicPermission::Undetermined,
    }
}

pub(super) fn check() -> Result<(), VoiceError> {
    if supported() {
        Ok(())
    } else {
        Err(VoiceError::OpenFailed("this browser has no microphone access here; it needs HTTPS (or localhost) and AudioWorklet".into()))
    }
}

pub(super) fn permission() -> MicPermission {
    answer(&permission_state())
}

pub(super) fn request_permission(callback: Box<dyn FnOnce(MicPermission) + Send>) {
    let done = Closure::once_into_js(move |state: String| callback(answer(&state)));
    request(&done);
}

type Ends = Rc<RefCell<Option<(Render, Heard)>>>;

struct Session {
    handle: JsValue,
    _frames: Closure<dyn FnMut(Float32Array, u32) -> Float32Array>,
    _ended: Closure<dyn FnMut(String, String)>,
}

struct Browser {
    rate: u32,
    signals: SignalSender,
    meters: Arc<Meters>,
    ends: Ends,
    session: Option<Session>,
}

pub(super) fn device(rate: u32, capture: Capture, playback: Playback, signals: SignalSender) -> Box<dyn VoiceDevice> {
    let meters = capture.meters.clone();
    let ends = Rc::new(RefCell::new(Some(duplex::passing(capture, playback))));
    Box::new(Browser { rate, signals, meters, ends, session: None })
}

fn fault(kind: &str, message: String) -> Signal {
    match kind {
        "changed" => Signal::DevicesChanged,
        "denied" | "unsupported" => Signal::Lost(Fault::lasting(message)),
        _ => Signal::Lost(Fault::passing(message)),
    }
}

impl Browser {
    fn stop(&mut self) {
        if let Some(session) = self.session.take() {
            close(&session.handle);
        }
    }
}

impl VoiceDevice for Browser {
    fn prepare(&mut self) -> Result<(), Fault> {
        Ok(())
    }

    fn run(&mut self) -> Result<(), Fault> {
        if self.session.is_some() {
            return Ok(());
        }
        if permission() == MicPermission::Denied {
            return Err(Fault::lasting("microphone access was refused; allow it in the browser's site settings"));
        }
        let (ends, meters) = (self.ends.clone(), self.meters.clone());
        let mut speaker = Vec::new();
        let frames = Closure::<dyn FnMut(Float32Array, u32) -> Float32Array>::new(move |mic: Float32Array, late: u32| {
            let heard = mic.to_vec();
            speaker.resize(heard.len(), 0.0);
            meters.count_late(u64::from(late));
            if let Some((render, listen)) = ends.borrow_mut().as_mut() {
                listen.capture(&heard);
                render.render(&mut speaker);
            } else {
                speaker.fill(0.0);
            }
            Float32Array::from(&speaker[..])
        });
        let signals = self.signals.clone();
        let ended = Closure::<dyn FnMut(String, String)>::new(move |kind: String, message: String| {
            signals.send(fault(&kind, message)).ok();
            inline::soon();
        });
        let handle = open(self.rate, &frames, &ended);
        self.session = Some(Session { handle, _frames: frames, _ended: ended });
        Ok(())
    }

    fn halt(&mut self) {
        self.stop();
    }

    fn discard(&mut self) {
        self.stop();
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        self.stop();
    }
}

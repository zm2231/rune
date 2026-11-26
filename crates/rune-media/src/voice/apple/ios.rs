use super::super::VoiceError;
use super::super::control::{Signal, SignalSender};
use super::super::machine::Fault;
use crate::recorder::MicPermission;
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, ProtocolObject};
use objc2::{class, msg_send};
use objc2_avf_audio::{
    AVAudioSession, AVAudioSessionCategoryOptions, AVAudioSessionCategoryPlayAndRecord,
    AVAudioSessionInterruptionNotification, AVAudioSessionInterruptionOptionKey, AVAudioSessionInterruptionOptions,
    AVAudioSessionInterruptionType, AVAudioSessionInterruptionTypeKey, AVAudioSessionMediaServicesWereResetNotification,
    AVAudioSessionModeVoiceChat, AVAudioSessionRecordPermission, AVAudioSessionRouteChangeNotification,
    AVAudioSessionRouteChangeReason, AVAudioSessionRouteChangeReasonKey, AVAudioSessionSetActiveOptions,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol, NSString};
use std::ptr::NonNull;
use std::sync::Mutex;

const PREFERRED_RATE: f64 = 48_000.0;
const PREFERRED_BUFFER: f64 = 0.01;

pub(crate) fn check() -> Result<(), VoiceError> {
    let key = NSString::from_str("NSMicrophoneUsageDescription");
    // SAFETY: `NSBundle`'s `mainBundle` and `objectForInfoDictionaryKey:` match their Foundation signatures.
    let present = unsafe {
        let bundle: Option<Retained<AnyObject>> = msg_send![class!(NSBundle), mainBundle];
        bundle.is_some_and(|bundle| {
            let value: Option<Retained<AnyObject>> = msg_send![&*bundle, objectForInfoDictionaryKey: &*key];
            value.is_some()
        })
    };
    if present {
        Ok(())
    } else {
        Err(VoiceError::OpenFailed("the app's Info.plist has no NSMicrophoneUsageDescription, so iOS won't let it use the microphone".into()))
    }
}

fn session() -> Retained<AVAudioSession> {
    // SAFETY: `sharedInstance` takes no arguments and returns the app's session.
    unsafe { AVAudioSession::sharedInstance() }
}

#[allow(deprecated)]
pub(crate) fn permission() -> MicPermission {
    // SAFETY: `recordPermission` reads the session's current answer.
    match unsafe { session().recordPermission() } {
        AVAudioSessionRecordPermission::Granted => MicPermission::Granted,
        AVAudioSessionRecordPermission::Denied => MicPermission::Denied,
        _ => MicPermission::Undetermined,
    }
}

#[allow(deprecated)]
pub(crate) fn request_permission(callback: Box<dyn FnOnce(MicPermission) + Send>) {
    let callback = Mutex::new(Some(callback));
    let block = RcBlock::new(move |granted: Bool| {
        if let Some(callback) = callback.lock().ok().and_then(|mut c| c.take()) {
            callback(if granted.as_bool() { MicPermission::Granted } else { MicPermission::Denied });
        }
    });
    // SAFETY: `block` matches `void (^)(BOOL)` and the session copies it.
    unsafe { session().requestRecordPermission(&block) };
}

fn info_number(note: &NSNotification, key: Option<&NSString>) -> Option<usize> {
    let key = key?;
    // SAFETY: `userInfo` is an `NSDictionary` or nil, its values for these keys are `NSNumber`s, and the selectors match their Foundation signatures.
    unsafe {
        let info: Option<Retained<AnyObject>> = msg_send![note, userInfo];
        let value: Option<Retained<AnyObject>> = msg_send![&*info?, objectForKey: key];
        let number: usize = msg_send![&*value?, unsignedIntegerValue];
        Some(number)
    }
}

fn interruption(note: &NSNotification) -> Option<Signal> {
    // SAFETY: the keys are immutable AVFAudio string constants.
    let (type_key, option_key) = unsafe { (AVAudioSessionInterruptionTypeKey, AVAudioSessionInterruptionOptionKey) };
    let kind = info_number(note, type_key)?;
    if kind == AVAudioSessionInterruptionType::Began.0 {
        return Some(Signal::InterruptionBegan);
    }
    let options = info_number(note, option_key).unwrap_or(0);
    Some(Signal::InterruptionEnded { should_resume: options & AVAudioSessionInterruptionOptions::ShouldResume.0 != 0 })
}

fn route_change(note: &NSNotification) -> Option<Signal> {
    // SAFETY: the key is an immutable AVFAudio string constant.
    let reason = info_number(note, unsafe { AVAudioSessionRouteChangeReasonKey })?;
    let moved = [AVAudioSessionRouteChangeReason::NewDeviceAvailable.0, AVAudioSessionRouteChangeReason::OldDeviceUnavailable.0];
    moved.contains(&reason).then_some(Signal::DevicesChanged)
}

pub(super) struct Platform {
    observers: Vec<Retained<ProtocolObject<dyn NSObjectProtocol>>>,
    active: bool,
}

impl Platform {
    pub(super) fn new(signals: SignalSender) -> Self {
        let center = NSNotificationCenter::defaultCenter();
        let background = NSString::from_str("UIApplicationDidEnterBackgroundNotification");
        let foreground = NSString::from_str("UIApplicationWillEnterForegroundNotification");
        // SAFETY: the names are immutable AVFAudio string constants.
        let (interrupted, routed, reset) = unsafe {
            (AVAudioSessionInterruptionNotification, AVAudioSessionRouteChangeNotification, AVAudioSessionMediaServicesWereResetNotification)
        };
        let watch: [(Option<&NSString>, fn(&NSNotification) -> Option<Signal>); 5] = [
            (interrupted, interruption),
            (routed, route_change),
            (reset, |_| Some(Signal::MediaReset)),
            (Some(&background), |_| Some(Signal::Backgrounded)),
            (Some(&foreground), |_| Some(Signal::Foregrounded)),
        ];
        let observers = watch
            .into_iter()
            .filter_map(|(name, read)| {
                let name = name?;
                let signals = signals.clone();
                let block = RcBlock::new(move |note: NonNull<NSNotification>| {
                    // SAFETY: the center passes a valid notification for the duration of the block.
                    if let Some(signal) = read(unsafe { note.as_ref() }) {
                        signals.send(signal).ok();
                    }
                });
                // SAFETY: a nil queue runs the block on the posting thread; it only sends on a channel, which is thread-safe.
                Some(unsafe { center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block) })
            })
            .collect();
        Self { observers, active: false }
    }

    pub(super) fn prepare(&mut self) -> Result<(), Fault> {
        let session = session();
        // SAFETY: the category and mode are immutable AVFAudio string constants.
        let (Some(category), Some(mode)) = (unsafe { AVAudioSessionCategoryPlayAndRecord }, unsafe { AVAudioSessionModeVoiceChat }) else {
            return Err(Fault::lasting("the voice audio session is unavailable"));
        };
        let options = AVAudioSessionCategoryOptions::AllowBluetoothHFP | AVAudioSessionCategoryOptions::DefaultToSpeaker;
        // SAFETY: `setCategory:mode:options:error:` takes valid session constants and options.
        unsafe { session.setCategory_mode_options_error(category, mode, options) }
            .map_err(|e| Fault::passing(format!("setting the audio session: {}", e.localizedDescription())))?;
        // SAFETY: preferences are hints the session may ignore; both values are in range.
        unsafe {
            session.setPreferredSampleRate_error(PREFERRED_RATE).ok();
            session.setPreferredIOBufferDuration_error(PREFERRED_BUFFER).ok();
        }
        Ok(())
    }

    pub(super) fn activate(&mut self) -> Result<(), Fault> {
        self.prepare()?;
        // SAFETY: `setActive:error:` on the shared session; activating an active session is a no-op, and after a media reset it brings the session back.
        unsafe { session().setActive_error(true) }
            .map_err(|e| Fault::passing(format!("activating the audio session: {}", e.localizedDescription())))?;
        self.active = true;
        Ok(())
    }

    pub(super) fn deactivate(&mut self) {
        if self.active {
            // SAFETY: `setActive:withOptions:error:` on the shared session, letting other apps resume their audio.
            unsafe { session().setActive_withOptions_error(false, AVAudioSessionSetActiveOptions::NotifyOthersOnDeactivation) }.ok();
            self.active = false;
        }
    }
}

impl Drop for Platform {
    fn drop(&mut self) {
        self.deactivate();
        let center = NSNotificationCenter::defaultCenter();
        for observer in &self.observers {
            // SAFETY: each observer is a token `addObserverForName:object:queue:usingBlock:` returned on this center.
            unsafe { center.removeObserver(observer.as_ref()) };
        }
    }
}

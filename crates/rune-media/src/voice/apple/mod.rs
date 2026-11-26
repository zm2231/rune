#![allow(unsafe_code)]

#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "ios")]
use ios as platform;
#[cfg(target_os = "macos")]
use macos as platform;

pub(super) use platform::{check, permission, request_permission};

use super::control::{SignalSender, VoiceDevice};
use super::io::{Capture, Playback};
use super::machine::Fault;
use crate::recorder::MicPermission;
use objc2_audio_toolbox::{
    AURenderCallbackStruct, AUVoiceIOOtherAudioDuckingConfiguration, AUVoiceIOOtherAudioDuckingLevel,
    AudioComponentDescription, AudioComponentFindNext, AudioComponentInstanceDispose, AudioComponentInstanceNew,
    AudioOutputUnitStart, AudioOutputUnitStop, AudioUnit, AudioUnitInitialize, AudioUnitRenderActionFlags,
    AudioUnitRender, AudioUnitSetProperty, AudioUnitUninitialize, kAUVoiceIOProperty_OtherAudioDuckingConfiguration,
    kAudioOutputUnitProperty_EnableIO, kAudioOutputUnitProperty_SetInputCallback, kAudioUnitManufacturer_Apple,
    kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitProperty_SetRenderCallback, kAudioUnitProperty_StreamFormat,
    kAudioUnitScope_Global, kAudioUnitScope_Input, kAudioUnitScope_Output, kAudioUnitSubType_VoiceProcessingIO,
    kAudioUnitType_Output,
};
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp, kAudioFormatFlagIsFloat,
    kAudioFormatFlagIsPacked, kAudioFormatLinearPCM,
};
use std::ffi::c_void;
use std::ptr::{NonNull, null_mut};

const MAX_SLICE: u32 = 4096;
const INPUT_BUS: u32 = 1;
const OUTPUT_BUS: u32 = 0;

struct Input {
    unit: AudioUnit,
    capture: Capture,
    scratch: Box<[f32]>,
}

struct Vpio {
    rate: f64,
    unit: AudioUnit,
    started: bool,
    input: Box<Input>,
    playback: Box<Playback>,
    platform: platform::Platform,
}

pub(super) fn device(rate: u32, capture: Capture, playback: Playback, signals: SignalSender) -> Box<dyn VoiceDevice> {
    Box::new(Vpio {
        rate: f64::from(rate),
        unit: null_mut(),
        started: false,
        input: Box::new(Input { unit: null_mut(), capture, scratch: vec![0.0; MAX_SLICE as usize].into_boxed_slice() }),
        playback: Box::new(playback),
        platform: platform::Platform::new(signals),
    })
}

fn status(code: i32, what: &str) -> Result<(), Fault> {
    if code == 0 { Ok(()) } else { Err(Fault::passing(format!("{what} failed (OSStatus {code})"))) }
}

fn set<T>(unit: AudioUnit, id: u32, scope: u32, element: u32, value: &T, what: &str) -> Result<(), Fault> {
    let size = u32::try_from(std::mem::size_of::<T>()).unwrap_or(u32::MAX);
    // SAFETY: `unit` is a live instance, and `value` points to `size` readable bytes of the property's type.
    status(unsafe { AudioUnitSetProperty(unit, id, scope, element, (value as *const T).cast(), size) }, what)
}

fn mono_float(rate: f64) -> AudioStreamBasicDescription {
    AudioStreamBasicDescription {
        mSampleRate: rate,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
        mBytesPerPacket: 4,
        mFramesPerPacket: 1,
        mBytesPerFrame: 4,
        mChannelsPerFrame: 1,
        mBitsPerChannel: 32,
        mReserved: 0,
    }
}

impl Vpio {
    fn build(&mut self) -> Result<(), Fault> {
        let desc = AudioComponentDescription {
            componentType: kAudioUnitType_Output,
            componentSubType: kAudioUnitSubType_VoiceProcessingIO,
            componentManufacturer: kAudioUnitManufacturer_Apple,
            componentFlags: 0,
            componentFlagsMask: 0,
        };
        // SAFETY: `desc` is a valid description, and a null component starts the search at the first match.
        let component = unsafe { AudioComponentFindNext(null_mut(), NonNull::from(&desc)) };
        if component.is_null() {
            return Err(Fault::lasting("the system's voice processing unit is unavailable"));
        }
        let mut unit: AudioUnit = null_mut();
        // SAFETY: `component` was just found, and `unit` is a valid out-pointer.
        status(unsafe { AudioComponentInstanceNew(component, NonNull::from(&mut unit)) }, "creating the voice unit")?;
        if let Err(fault) = self.configure(unit) {
            // SAFETY: `unit` was created above, isn't started and is disposed once.
            unsafe { AudioComponentInstanceDispose(unit) };
            return Err(fault);
        }
        self.unit = unit;
        self.input.unit = unit;
        Ok(())
    }

    fn configure(&mut self, unit: AudioUnit) -> Result<(), Fault> {
        let on: u32 = 1;
        set(unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Input, INPUT_BUS, &on, "enabling the microphone")?;
        set(unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Output, OUTPUT_BUS, &on, "enabling the speaker")?;
        set(unit, kAudioUnitProperty_MaximumFramesPerSlice, kAudioUnitScope_Global, 0, &MAX_SLICE, "sizing callbacks")?;
        let format = mono_float(self.rate);
        set(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Output, INPUT_BUS, &format, "setting the microphone format")?;
        set(unit, kAudioUnitProperty_StreamFormat, kAudioUnitScope_Input, OUTPUT_BUS, &format, "setting the speaker format")?;
        self.input.unit = unit;
        let input = AURenderCallbackStruct {
            inputProc: Some(on_input),
            inputProcRefCon: (&raw mut *self.input).cast::<c_void>(),
        };
        set(unit, kAudioOutputUnitProperty_SetInputCallback, kAudioUnitScope_Global, INPUT_BUS, &input, "installing the microphone callback")?;
        let render = AURenderCallbackStruct {
            inputProc: Some(on_render),
            inputProcRefCon: (&raw mut *self.playback).cast::<c_void>(),
        };
        set(unit, kAudioUnitProperty_SetRenderCallback, kAudioUnitScope_Input, OUTPUT_BUS, &render, "installing the speaker callback")?;
        let ducking = AUVoiceIOOtherAudioDuckingConfiguration {
            mEnableAdvancedDucking: true as _,
            mDuckingLevel: AUVoiceIOOtherAudioDuckingLevel::Min,
        };
        set(unit, kAUVoiceIOProperty_OtherAudioDuckingConfiguration, kAudioUnitScope_Global, 0, &ducking, "ducking").ok();
        // SAFETY: `unit` is configured and not yet initialized.
        status(unsafe { AudioUnitInitialize(unit) }, "starting the voice unit")
    }
}

impl VoiceDevice for Vpio {
    fn prepare(&mut self) -> Result<(), Fault> {
        self.platform.prepare()?;
        if self.unit.is_null() {
            self.build()?;
        }
        Ok(())
    }

    fn run(&mut self) -> Result<(), Fault> {
        if matches!(permission(), MicPermission::Denied | MicPermission::Restricted) {
            return Err(Fault::lasting("microphone access is off for this app; turn it on in the system's privacy settings"));
        }
        self.platform.activate()?;
        self.prepare()?;
        if !self.started {
            // SAFETY: `self.unit` is a live, initialized voice unit.
            status(unsafe { AudioOutputUnitStart(self.unit) }, "starting audio")?;
            self.started = true;
        }
        Ok(())
    }

    fn halt(&mut self) {
        self.stop_unit();
        self.platform.deactivate();
    }

    fn discard(&mut self) {
        self.stop_unit();
        if !self.unit.is_null() {
            // SAFETY: `self.unit` is live and stopped; after disposal no callback runs, and it is cleared so it is never used again.
            unsafe {
                AudioUnitUninitialize(self.unit);
                AudioComponentInstanceDispose(self.unit);
            }
            self.unit = null_mut();
            self.input.unit = null_mut();
        }
    }
}

impl Vpio {
    fn stop_unit(&mut self) {
        if self.started {
            // SAFETY: `self.unit` is a live, started unit; stopping waits for the callback in flight.
            unsafe { AudioOutputUnitStop(self.unit) };
            self.started = false;
        }
    }
}

impl Drop for Vpio {
    fn drop(&mut self) {
        self.discard();
    }
}

unsafe extern "C-unwind" fn on_input(
    refcon: NonNull<c_void>,
    flags: NonNull<AudioUnitRenderActionFlags>,
    time: NonNull<AudioTimeStamp>,
    bus: u32,
    frames: u32,
    _data: *mut AudioBufferList,
) -> i32 {
    // SAFETY: the refcon is the `Input` box the unit was configured with, which outlives the unit, and only this callback touches it while the unit runs.
    let input = unsafe { refcon.cast::<Input>().as_mut() };
    let len = frames as usize;
    if len > input.scratch.len() {
        input.capture.meters.count_oversized();
        return 0;
    }
    let mut list = AudioBufferList {
        mNumberBuffers: 1,
        mBuffers: [AudioBuffer { mNumberChannels: 1, mDataByteSize: frames * 4, mData: input.scratch.as_mut_ptr().cast() }],
    };
    // SAFETY: called from the unit's own input callback with its flags and timestamp; `list` points to `frames` writable floats.
    let code = unsafe { AudioUnitRender(input.unit, flags.as_ptr(), time, bus, frames, NonNull::from(&mut list)) };
    if code == 0 {
        input.capture.capture(&input.scratch[..len]);
    }
    code
}

unsafe extern "C-unwind" fn on_render(
    refcon: NonNull<c_void>,
    _flags: NonNull<AudioUnitRenderActionFlags>,
    _time: NonNull<AudioTimeStamp>,
    _bus: u32,
    frames: u32,
    data: *mut AudioBufferList,
) -> i32 {
    // SAFETY: the refcon is the `Playback` box the unit was configured with, which outlives the unit, and only this callback touches it while the unit runs.
    let playback = unsafe { refcon.cast::<Playback>().as_mut() };
    // SAFETY: the unit passes a valid buffer list for this render, or null.
    let Some(list) = (unsafe { data.as_mut() }) else { return 0 };
    if list.mNumberBuffers == 0 || list.mBuffers[0].mData.is_null() {
        return 0;
    }
    let buffer = &mut list.mBuffers[0];
    let len = (buffer.mDataByteSize as usize / 4).min(frames as usize);
    // SAFETY: the unit's buffer holds `mDataByteSize` bytes of mono floats in the format set on the output bus.
    let out = unsafe { std::slice::from_raw_parts_mut(buffer.mData.cast::<f32>(), len) };
    playback.render(out);
    0
}

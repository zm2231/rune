use objc2::rc::Retained;
use objc2_foundation::NSSet;
use objc2_ui_kit::{UIEvent, UITouch, UIView};
use rune::TouchPhase;

use super::host::Host;

/// Feeds a UIKit touch callback to the runtime. A finger's id is its `UITouch`'s address,
/// which UIKit keeps for the whole touch; times are `UITouch.timestamp`, seconds since boot.
/// A move replays the touch's coalesced samples, the ones UIKit gathered between display
/// refreshes, so a fling's velocity sees the finger's whole path.
pub(crate) fn feed(host: &mut dyn Host, view: &UIView, touches: &NSSet<UITouch>, event: Option<&UIEvent>, phase: TouchPhase) {
    for touch in touches {
        let id = Retained::as_ptr(&touch) as u64;
        let coalesced = match (phase, event) {
            (TouchPhase::Moved, Some(event)) => event.coalescedTouchesForTouch(&touch),
            _ => None,
        };
        match coalesced {
            Some(samples) if !samples.is_empty() => {
                for sample in &samples {
                    send(host, view, id, &sample, phase);
                }
            }
            _ => send(host, view, id, &touch, phase),
        }
    }
}

fn send(host: &mut dyn Host, view: &UIView, id: u64, touch: &UITouch, phase: TouchPhase) {
    let at = touch.locationInView(Some(view));
    host.touch(id, phase, at.x as f32, at.y as f32, touch.timestamp());
}

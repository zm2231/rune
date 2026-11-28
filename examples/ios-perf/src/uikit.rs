#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObject};
use objc2::{ClassType, Encode, Encoding, class, define_class, msg_send, sel};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;

use crate::rows;

// SAFETY: Foundation declares it as an `NSRunLoopMode`, an `NSString` that lives for the process.
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
    static NSRunLoopCommonModes: &'static AnyObject;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FrameRateRange {
    minimum: f32,
    maximum: f32,
    preferred: f32,
}

// SAFETY: `CAFrameRateRange` is three floats in this order.
unsafe impl Encode for FrameRateRange {
    const ENCODING: Encoding = Encoding::Struct("CAFrameRateRange", &[f32::ENCODING, f32::ENCODING, f32::ENCODING]);
}

const FRAME_RATES: FrameRateRange = FrameRateRange { minimum: 80.0, maximum: 120.0, preferred: 120.0 };
const AUTORESIZE_FILL: usize = 2 | 16;
const TAG_SWATCH: isize = 1;
const TAG_TITLE: isize = 2;
const TAG_DETAIL: isize = 3;

pub type OnTick = Box<dyn FnMut(f64, f64, &AnyObject)>;

thread_local! {
    static TICK: RefCell<Option<OnTick>> = const { RefCell::new(None) };
    static ROWS: Cell<usize> = const { Cell::new(0) };
    static WIDTH: Cell<f64> = const { Cell::new(0.0) };
    static TABLE: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
    static KEEP: RefCell<Vec<Retained<NSObject>>> = const { RefCell::new(Vec::new()) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "IosPerfTicker"]
    struct Ticker;

    impl Ticker {
        #[unsafe(method(tick:))]
        fn tick(&self, link: &AnyObject) {
            // SAFETY: a display link's `timestamp` and `targetTimestamp` are `CFTimeInterval`s.
            let (shown, target): (f64, f64) = unsafe { (msg_send![link, timestamp], msg_send![link, targetTimestamp]) };
            TICK.with(|tick| if let Some(tick) = tick.borrow_mut().as_mut() { tick(shown, target, link) });
        }
    }
);

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "IosPerfRows"]
    struct Rows;

    impl Rows {
        #[unsafe(method(tableView:numberOfRowsInSection:))]
        fn count(&self, _table: &AnyObject, _section: isize) -> isize { ROWS.get() as isize }

        #[unsafe(method(tableView:cellForRowAtIndexPath:))]
        fn cell(&self, table: &AnyObject, index: &AnyObject) -> *mut AnyObject {
            let id = NSString::from_str("row");
            // SAFETY: `row` is an `NSInteger`; the dequeued cell is autoreleased or nil.
            let (row, reused): (isize, Option<Retained<AnyObject>>) = unsafe { (msg_send![index, row], msg_send![table, dequeueReusableCellWithIdentifier: &*id]) };
            let cell = reused.unwrap_or_else(|| make_cell(&id));
            fill(&cell, row.max(0) as usize);
            Retained::autorelease_return(cell)
        }
    }
);

fn new_view(class: &objc2::runtime::AnyClass, frame: CGRect) -> Retained<AnyObject> {
    // SAFETY: every `UIView` subclass takes `initWithFrame:`.
    unsafe {
        let alloc: Allocated<AnyObject> = msg_send![class, alloc];
        msg_send![alloc, initWithFrame: frame]
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> CGRect { CGRect::new(CGPoint::new(x, y), CGSize::new(w, h)) }

fn color([r, g, b]: [u8; 3]) -> Retained<AnyObject> {
    let c = |v: u8| v as f64 / 255.0;
    // SAFETY: `colorWithRed:green:blue:alpha:` takes four `CGFloat`s.
    unsafe { msg_send![class!(UIColor), colorWithRed: c(r), green: c(g), blue: c(b), alpha: 1.0f64] }
}

fn label(frame: CGRect, size: f64, rgb: [u8; 3], tag: isize) -> Retained<AnyObject> {
    let label = new_view(class!(UILabel), frame);
    // SAFETY: `UILabel` properties of these types.
    unsafe {
        let font: Retained<AnyObject> = msg_send![class!(UIFont), systemFontOfSize: size];
        let _: () = msg_send![&*label, setFont: &*font];
        let _: () = msg_send![&*label, setTextColor: &*color(rgb)];
        let _: () = msg_send![&*label, setTag: tag];
    }
    label
}

fn make_cell(id: &NSString) -> Retained<AnyObject> {
    let width = WIDTH.get();
    // SAFETY: `UITableViewCell`'s designated initializer and properties of these types.
    unsafe {
        let alloc: Allocated<AnyObject> = msg_send![class!(UITableViewCell), alloc];
        let cell: Retained<AnyObject> = msg_send![alloc, initWithStyle: 0isize, reuseIdentifier: id];
        let _: () = msg_send![&*cell, setSelectionStyle: 0isize];
        let content: Retained<AnyObject> = msg_send![&*cell, contentView];
        let swatch = new_view(class!(UIView), rect(16.0, 16.0, 28.0, 28.0));
        let layer: Retained<AnyObject> = msg_send![&*swatch, layer];
        let _: () = msg_send![&*layer, setCornerRadius: 14.0f64];
        let _: () = msg_send![&*swatch, setTag: TAG_SWATCH];
        let _: () = msg_send![&*content, addSubview: &*swatch];
        let _: () = msg_send![&*content, addSubview: &*label(rect(56.0, 10.0, width - 72.0, 22.0), rows::TITLE_SIZE as f64, rows::TITLE_RGB, TAG_TITLE)];
        let _: () = msg_send![&*content, addSubview: &*label(rect(56.0, 32.0, width - 72.0, 18.0), rows::DETAIL_SIZE as f64, rows::DETAIL_RGB, TAG_DETAIL)];
        cell
    }
}

fn fill(cell: &AnyObject, row: usize) {
    // SAFETY: `make_cell` made these tagged subviews: a `UIView` and two `UILabel`s.
    unsafe {
        let content: Retained<AnyObject> = msg_send![cell, contentView];
        let swatch: Option<Retained<AnyObject>> = msg_send![&*content, viewWithTag: TAG_SWATCH];
        let title: Option<Retained<AnyObject>> = msg_send![&*content, viewWithTag: TAG_TITLE];
        let detail: Option<Retained<AnyObject>> = msg_send![&*content, viewWithTag: TAG_DETAIL];
        if let Some(swatch) = swatch { let _: () = msg_send![&*swatch, setBackgroundColor: &*color(rows::swatch(row))]; }
        if let Some(title) = title { let _: () = msg_send![&*title, setText: &*NSString::from_str(&rows::title(row))]; }
        if let Some(detail) = detail { let _: () = msg_send![&*detail, setText: &*NSString::from_str(&rows::detail(row))]; }
    }
}

fn root_view() -> Option<Retained<AnyObject>> {
    // SAFETY: UIKit's application, scene and window accessors, on the main thread.
    unsafe {
        let app: Retained<AnyObject> = msg_send![class!(UIApplication), sharedApplication];
        let scenes: Retained<AnyObject> = msg_send![&*app, connectedScenes];
        let scene: Option<Retained<AnyObject>> = msg_send![&*scenes, anyObject];
        let window: Option<Retained<AnyObject>> = msg_send![&*scene?, keyWindow];
        let root: Option<Retained<AnyObject>> = msg_send![&*window?, rootViewController];
        msg_send![&*root?, view]
    }
}

pub fn show_table(count: usize) -> bool {
    let Some(root) = root_view() else { return false };
    show_table_in(&root, count);
    true
}

pub fn show_table_in(root: &AnyObject, count: usize) {
    ROWS.set(count);
    // SAFETY: `UITableView` and `UIView` properties of these types, on the main thread. The
    // table holds its data source weakly, so `KEEP` keeps it.
    unsafe {
        let bounds: CGRect = msg_send![root, bounds];
        WIDTH.set(bounds.size.width);
        let alloc: Allocated<AnyObject> = msg_send![class!(UITableView), alloc];
        let table: Retained<AnyObject> = msg_send![alloc, initWithFrame: bounds, style: 0isize];
        let _: () = msg_send![&*table, setAutoresizingMask: AUTORESIZE_FILL];
        let _: () = msg_send![&*table, setRowHeight: crate::drive::ROW_HEIGHT as f64];
        let _: () = msg_send![&*table, setEstimatedRowHeight: crate::drive::ROW_HEIGHT as f64];
        let _: () = msg_send![&*table, setSeparatorStyle: 0isize];
        let _: () = msg_send![&*table, setBackgroundColor: &*color(rows::BACKGROUND_RGB)];
        let source: Retained<Rows> = msg_send![Rows::class(), new];
        let _: () = msg_send![&*table, setDataSource: &*source];
        let _: () = msg_send![root, addSubview: &*table];
        KEEP.with(|keep| keep.borrow_mut().push(Retained::into_super(source)));
        TABLE.with(|t| *t.borrow_mut() = Some(table));
    }
}

pub fn table_offset() -> Option<f64> {
    TABLE.with(|t| {
        let table = t.borrow();
        // SAFETY: `contentOffset` is a `CGPoint`.
        table.as_ref().map(|table| unsafe { msg_send![&**table, contentOffset] }).map(|p: CGPoint| p.y)
    })
}

pub fn set_table_offset(y: f64) {
    TABLE.with(|t| {
        if let Some(table) = &*t.borrow() {
            // SAFETY: `contentOffset` is a `CGPoint`.
            let _: () = unsafe { msg_send![&**table, setContentOffset: CGPoint::new(0.0, y)] };
        }
    });
}

pub fn start_link(tick: OnTick) {
    TICK.with(|t| *t.borrow_mut() = Some(tick));
    // SAFETY: `tick:` is a method of `Ticker` that takes the link; a display link's frame rate
    // range is a `CAFrameRateRange`. The link retains its target until it is invalidated.
    unsafe {
        let ticker: Retained<Ticker> = msg_send![Ticker::class(), new];
        let link: Retained<AnyObject> = msg_send![class!(CADisplayLink), displayLinkWithTarget: &*ticker, selector: sel!(tick:)];
        let _: () = msg_send![&*link, setPreferredFrameRateRange: FRAME_RATES];
        let run_loop: Retained<AnyObject> = msg_send![class!(NSRunLoop), mainRunLoop];
        let _: () = msg_send![&*link, addToRunLoop: &*run_loop, forMode: NSRunLoopCommonModes];
    }
}

pub fn stop_link(link: &AnyObject) {
    // SAFETY: invalidating a display link on the main thread, whose run loop it was added to.
    let _: () = unsafe { msg_send![link, invalidate] };
}

#[link(name = "UIKit", kind = "framework")]
unsafe extern "C" {
    fn UIAccessibilityIsReduceMotionEnabled() -> bool;
}

#[link(name = "Metal", kind = "framework")]
unsafe extern "C" {
    fn MTLCreateSystemDefaultDevice() -> *mut AnyObject;
}

pub fn gpu_allocated_mib() -> f64 {
    // SAFETY: `MTLCreateSystemDefaultDevice` answers a +1 `MTLDevice` or null, the device Rune
    // draws with; `currentAllocatedSize` is an `NSUInteger` safe to read from any thread.
    unsafe {
        let Some(device) = Retained::from_raw(MTLCreateSystemDefaultDevice()) else { return f64::NAN };
        let bytes: usize = msg_send![&*device, currentAllocatedSize];
        bytes as f64 / (1024.0 * 1024.0)
    }
}

pub fn reduce_motion() -> bool {
    // SAFETY: a UIKit function that takes nothing and answers a `BOOL`, read on the main thread.
    unsafe { UIAccessibilityIsReduceMotionEnabled() }
}

pub fn display() -> (isize, bool) {
    // SAFETY: `maximumFramesPerSecond` is an `NSInteger`; an Info.plist boolean is an `NSNumber`.
    unsafe {
        let screen: Retained<AnyObject> = msg_send![class!(UIScreen), mainScreen];
        let fps: isize = msg_send![&*screen, maximumFramesPerSecond];
        let bundle: Retained<AnyObject> = msg_send![class!(NSBundle), mainBundle];
        let key = NSString::from_str("CADisableMinimumFrameDurationOnPhone");
        let value: Option<Retained<AnyObject>> = msg_send![&*bundle, objectForInfoDictionaryKey: &*key];
        let on = value.is_some_and(|v| { let on: bool = msg_send![&*v, boolValue]; on });
        (fps, on)
    }
}

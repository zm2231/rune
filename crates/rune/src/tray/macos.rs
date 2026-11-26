//! `NSStatusBar` / `NSStatusItem` on macOS. Primary clicks send `TrayEvent::Activate`;
//! secondary clicks and Control-clicks open the flat `NSMenu`, whose picks send
//! `TrayEvent::Menu`. With `menu_on_primary_click` the menu is the status item's own, filled
//! from the latest items each time it opens. VoiceOver's and automation's `AXShowMenu` opens the
//! menu either way. `Tray::new` must run on the main thread; `set_menu`, `set_tooltip`,
//! `set_icon`, `set_icon_name`, `set_title`, `set_accessibility_label`, `rect` and `Drop` may
//! be called from any thread (AppKit calls dispatch to the main queue when off the main thread).

#![allow(unsafe_code)]

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use objc2::{
    AnyThread, ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject, Sel},
    sel,
};
use objc2_app_kit::{
    NSApplication, NSBitmapFormat, NSBitmapImageRep, NSButton, NSCellImagePosition, NSColorSpace,
    NSControl, NSDeviceRGBColorSpace, NSEvent, NSEventMask, NSEventModifierFlags, NSEventType,
    NSFont, NSFontWeightRegular, NSImage, NSImageScaling, NSMenu, NSMenuDelegate, NSMenuItem,
    NSResponder, NSScreen, NSStatusBar, NSStatusBarButton, NSStatusItem, NSVariableStatusItemLength,
    NSView,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSSize, NSString};

use super::{
    ClickAction, Handler, TrayClick, TrayError, TrayEvent, TrayIcon, TrayItem, TraySpec,
    macos_icon_point_size, macos_screen_rect_to_physical, premultiply_rgba, route_click,
};

const DEFAULT_BAR_THICKNESS: f64 = 22.0;

struct Model {
    accessibility_label: String,
    title: String,
    tooltip: String,
    icon: TrayIcon,
    thickness: f64,
    menu: Vec<TrayItem>,
    rect: Option<[i32; 4]>,
}

/// Thin platform seam around `NSStatusBar` / `NSStatusItem` / `NSStatusBarButton`, so headless
/// tests can drive image construction, menu building, click routing, updates and teardown without
/// creating a visible item in the live macOS menu bar.
trait StatusItemHost: 'static {
    fn set_image(&self, image: &NSImage);
    fn set_title(&self, title: &str);
    fn set_tooltip(&self, tooltip: Option<&str>);
    fn screen_rect(&self, mtm: MainThreadMarker) -> Option<[i32; 4]>;
    fn set_menu(&self, menu: Option<&NSMenu>);
    fn perform_click(&self);
    fn remove(&self);
}

struct SystemStatusItem {
    bar: Retained<NSStatusBar>,
    item: Retained<NSStatusItem>,
    button: Retained<NSStatusBarButton>,
    removed: Cell<bool>,
}

impl SystemStatusItem {
    fn new(
        mtm: MainThreadMarker,
        bar: Retained<NSStatusBar>,
        target: &ButtonTarget,
    ) -> Result<Self, TrayError> {
        let item = bar.statusItemWithLength(NSVariableStatusItemLength);
        let Some(button) = item.button(mtm) else {
            bar.removeStatusItem(&item);
            return Err(TrayError::Backend("NSStatusItem has no button".into()));
        };
        button.setImageScaling(NSImageScaling::ScaleProportionallyDown);
        // Monospaced digits in the menu-bar font so live counters keep steady width.
        let bar_font_size = NSFont::menuBarFontOfSize(0.0).pointSize();
        // SAFETY: `NSFontWeightRegular` is an immutable AppKit font-weight constant.
        let font = NSFont::monospacedDigitSystemFontOfSize_weight(bar_font_size, unsafe { NSFontWeightRegular });
        button.setFont(Some(&font));
        // SAFETY: main-thread AppKit button; `target` is retained in `ITEMS` until `remove`
        // clears both target and action.
        unsafe {
            button.setTarget(Some(target));
            button.setAction(Some(sel!(runeTrayClick:)));
        }
        let _ = button.sendActionOn(NSEventMask::LeftMouseUp | NSEventMask::RightMouseUp);
        let plain: &AnyObject = &button;
        if plain.class() == NSStatusBarButton::class() {
            // SAFETY: `StatusButton` subclasses `NSStatusBarButton`, adds no instance variables
            // and overrides only accessibility methods with AppKit's signatures. The button's
            // class is exactly `NSStatusBarButton`, so no runtime subclass (KVO's) is replaced.
            unsafe { AnyObject::set_class(plain, StatusButton::class()) };
        }
        Ok(Self { bar, item, button, removed: Cell::new(false) })
    }
}

impl StatusItemHost for SystemStatusItem {
    fn set_image(&self, image: &NSImage) {
        self.button.setImage(Some(image));
    }

    fn set_title(&self, title: &str) {
        if title.is_empty() {
            self.button.setTitle(&NSString::from_str(""));
            self.button.setImagePosition(NSCellImagePosition::ImageOnly);
        } else {
            self.button.setImagePosition(NSCellImagePosition::ImageLeading);
            self.button.setTitle(&NSString::from_str(title));
        }
    }

    fn set_tooltip(&self, tooltip: Option<&str>) {
        let tip = tooltip.filter(|s| !s.is_empty()).map(NSString::from_str);
        self.button.setToolTip(tip.as_deref());
    }

    fn screen_rect(&self, mtm: MainThreadMarker) -> Option<[i32; 4]> {
        if self.removed.get() {
            return None;
        }
        let window = self.button.window()?;
        let in_window = self.button.convertRect_toView(self.button.bounds(), None);
        let rect = window.convertRectToScreen(in_window);
        let primary_height = NSScreen::screens(mtm).iter().next()?.frame().size.height;
        let scale = window.backingScaleFactor();
        macos_screen_rect_to_physical(
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
            primary_height,
            scale,
        )
    }

    fn set_menu(&self, menu: Option<&NSMenu>) {
        if !self.removed.get() {
            self.item.setMenu(menu);
        }
    }

    fn perform_click(&self) {
        if self.removed.get() {
            return;
        }
        // SAFETY: main-thread status bar button; with `item.menu` set, `performClick:` opens
        // and tracks the status item's menu synchronously with native highlight and placement.
        unsafe { self.button.performClick(None) };
    }

    fn remove(&self) {
        if self.removed.replace(true) {
            return;
        }
        // SAFETY: main-thread button; clear the non-retaining target before dropping it.
        unsafe {
            self.button.setTarget(None);
            self.button.setAction(None);
        }
        self.item.setMenu(None);
        self.bar.removeStatusItem(&self.item);
    }
}

struct LiveItem {
    host: Rc<dyn StatusItemHost>,
    target: Retained<ButtonTarget>,
    model: Arc<Mutex<Model>>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static ITEMS: RefCell<HashMap<u64, LiveItem>> = RefCell::new(HashMap::new());
}

struct MenuEntry {
    id: String,
    enabled: bool,
}

struct MenuIvars {
    entries: Vec<MenuEntry>,
    on_event: Handler,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneTrayMenuTarget"]
    #[ivars = MenuIvars]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(runeTrayMenuPick:))]
        fn pick(&self, sender: &NSMenuItem) {
            let tag = sender.tag();
            let picked = usize::try_from(tag)
                .ok()
                .and_then(|i| self.ivars().entries.get(i))
                .filter(|entry| entry.enabled)
                .map(|entry| entry.id.clone());
            if let Some(id) = picked {
                dispatch_event(&self.ivars().on_event, TrayEvent::Menu(id));
            }
        }
    }
);

struct ButtonIvars {
    id: u64,
    model: Arc<Mutex<Model>>,
    on_event: Handler,
    menu_on_primary_click: bool,
    own_menu: RefCell<Option<Retained<NSMenu>>>,
    showing_menu: Cell<bool>,
    last_menu: RefCell<Option<(Retained<NSMenu>, Retained<MenuTarget>)>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneTrayButtonTarget"]
    #[ivars = ButtonIvars]
    struct ButtonTarget;

    impl ButtonTarget {
        #[unsafe(method(runeTrayClick:))]
        fn click(&self, _sender: Option<&AnyObject>) {
            let Some(mtm) = MainThreadMarker::new() else { return };
            let event = NSApplication::sharedApplication(mtm).currentEvent();
            self.handle_click(mtm, event_click(event.as_deref()));
        }
    }

    // SAFETY: `ButtonTarget` inherits from `NSObject`, which implements `NSObjectProtocol`.
    unsafe impl NSObjectProtocol for ButtonTarget {}

    // SAFETY: main-thread only; `menuNeedsUpdate:` matches AppKit's `NSMenuDelegate` selector ABI.
    unsafe impl NSMenuDelegate for ButtonTarget {
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            let Some(mtm) = MainThreadMarker::new() else { return };
            let items = self.ivars().model.lock().unwrap_or_else(|e| e.into_inner()).menu.clone();
            self.refill(mtm, menu, &items);
        }
    }
);

define_class!(
    #[unsafe(super(NSStatusBarButton, NSButton, NSControl, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneTrayStatusBarButton"]
    struct StatusButton;

    impl StatusButton {
        #[unsafe(method(accessibilityPerformShowMenu))]
        fn accessibility_perform_show_menu(&self) -> bool {
            let target = self.target();
            target.is_some_and(|target| show_menu_soon(Retained::as_ptr(&target)))
        }

        #[unsafe(method(isAccessibilitySelectorAllowed:))]
        fn is_accessibility_selector_allowed(&self, selector: Sel) -> bool {
            // SAFETY: `NSView` implements `isAccessibilitySelectorAllowed:` with this signature.
            selector == sel!(accessibilityPerformShowMenu)
                || unsafe { msg_send![super(self), isAccessibilitySelectorAllowed: selector] }
        }
    }
);

struct ShowingMenuGuard<'a>(&'a Cell<bool>);
impl Drop for ShowingMenuGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

impl ButtonTarget {
    fn handle_click(&self, mtm: MainThreadMarker, click: TrayClick) {
        let ivars = self.ivars();
        if ivars.showing_menu.get() {
            return;
        }
        let items = ivars.model.lock().unwrap_or_else(|e| e.into_inner()).menu.clone();
        match route_click(click, ivars.menu_on_primary_click, !items.is_empty()) {
            ClickAction::Ignore => {}
            ClickAction::Activate => {
                let _ = refresh_rect(ivars.id, mtm);
                dispatch_event(&ivars.on_event, TrayEvent::Activate { token: None });
            }
            ClickAction::ShowMenu => {
                let host = ITEMS.with(|map| map.borrow().get(&ivars.id).map(|live| live.host.clone()));
                let Some(host) = host else { return };
                ivars.showing_menu.set(true);
                let _guard = ShowingMenuGuard(&ivars.showing_menu);
                let own = ivars.own_menu.borrow().clone();
                if let Some(menu) = own {
                    host.set_menu(Some(&menu));
                    host.perform_click();
                } else {
                    let menu = new_menu(mtm);
                    self.refill(mtm, &menu, &items);
                    host.set_menu(Some(&menu));
                    host.perform_click();
                    host.set_menu(None);
                }
            }
        }
    }

    fn refill(&self, mtm: MainThreadMarker, menu: &NSMenu, items: &[TrayItem]) {
        let ivars = self.ivars();
        if let Some((old, _)) = ivars.last_menu.borrow_mut().take() {
            clear_menu_targets(&old);
        }
        menu.removeAllItems();
        let target = fill_menu(mtm, menu, items, ivars.on_event.clone());
        *ivars.last_menu.borrow_mut() = Some((Retained::from(menu), target));
    }
}

fn show_menu_soon(target: *const AnyObject) -> bool {
    let found = ITEMS.with(|map| {
        map.borrow().iter().find(|(_, live)| std::ptr::eq(Retained::as_ptr(&live.target).cast::<AnyObject>(), target)).map(|(id, live)| {
            (*id, !live.model.lock().unwrap_or_else(|e| e.into_inner()).menu.is_empty())
        })
    });
    let Some((id, true)) = found else { return false };
    crate::platform::main_queue::on_main(move || {
        let Some(mtm) = MainThreadMarker::new() else { return };
        let target = ITEMS.with(|map| map.borrow().get(&id).map(|live| live.target.clone()));
        if let Some(target) = target {
            target.handle_click(mtm, TrayClick::Secondary);
        }
    });
    true
}

fn dispatch_event(on_event: &Handler, event: TrayEvent) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| on_event(event))).is_err() {
        eprintln!("rune: tray callback panicked");
    }
}

fn event_click(event: Option<&NSEvent>) -> TrayClick {
    let Some(event) = event else { return TrayClick::Primary };
    let is_right = matches!(event.r#type(), NSEventType::RightMouseUp | NSEventType::RightMouseDown);
    let control = event.modifierFlags().contains(NSEventModifierFlags::Control);
    if is_right || control { TrayClick::Secondary } else { TrayClick::Primary }
}

fn valid_thickness(thickness: f64) -> f64 {
    if thickness.is_finite() && thickness > 0.0 { thickness } else { DEFAULT_BAR_THICKNESS }
}

fn accessibility_label(spec: &TraySpec) -> &str {
    if !spec.title.is_empty() { &spec.title } else { &spec.tooltip }
}

fn non_empty(text: &str) -> Option<&str> {
    (!text.is_empty()).then_some(text)
}

fn validate_icon_layout(icon: &TrayIcon, thickness: f64) -> Result<(f64, f64, isize, isize, isize), TrayError> {
    let Some((pt_w, pt_h)) = macos_icon_point_size(icon.width, icon.height, thickness) else {
        return Err(TrayError::Backend("tray icon dimensions must be non-zero".into()));
    };
    let w = isize::try_from(icon.width)
        .map_err(|_| TrayError::Backend("tray icon width overflow".into()))?;
    let h = isize::try_from(icon.height)
        .map_err(|_| TrayError::Backend("tray icon height overflow".into()))?;
    let row_bytes = w
        .checked_mul(4)
        .ok_or_else(|| TrayError::Backend("tray icon row size overflow".into()))?;
    let expected = usize::try_from(h)
        .ok()
        .and_then(|rows| usize::try_from(row_bytes).ok()?.checked_mul(rows));
    if expected != Some(icon.rgba.len()) {
        return Err(TrayError::Backend("tray icon size doesn't match its pixels".into()));
    }
    Ok((pt_w, pt_h, w, h, row_bytes))
}

/// Builds a premultiplied sRGB `NSBitmapImageRep` wrapped in an `NSImage` sized in points to
/// `thickness` high while preserving the pixel buffer's aspect ratio.
fn make_image(icon: &TrayIcon, thickness: f64, description: &str) -> Result<Retained<NSImage>, TrayError> {
    let (pt_w, pt_h, w, h, row_bytes) = validate_icon_layout(icon, thickness)?;
    objc2::rc::autoreleasepool(|_| {
        // SAFETY: null planes asks NSBitmapImageRep to allocate its own width × height × 4 buffer;
        // NSDeviceRGBColorSpace is an immutable AppKit constant.
        let rep = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                w,
                h,
                8,
                4,
                true,
                false,
                NSDeviceRGBColorSpace,
                NSBitmapFormat::empty(),
                row_bytes,
                32,
            )
        }
        .ok_or_else(|| TrayError::Backend("could not allocate tray bitmap".into()))?;
        let data = rep.bitmapData();
        let stride = rep.bytesPerRow();
        if data.is_null() || stride < row_bytes {
            return Err(TrayError::Backend("tray bitmap has no pixel buffer".into()));
        }
        let src_row_bytes = icon.width as usize * 4;
        for (y, row) in icon.rgba.chunks_exact(src_row_bytes).enumerate() {
            // SAFETY: `rep` owns `h` rows of `stride >= src_row_bytes` bytes starting at `data`.
            let dst = unsafe {
                std::slice::from_raw_parts_mut(data.offset(y as isize * stride), src_row_bytes)
            };
            premultiply_rgba(row, dst);
        }
        let rep = rep
            .bitmapImageRepByRetaggingWithColorSpace(&NSColorSpace::sRGBColorSpace())
            .unwrap_or(rep);
        let point_size = NSSize::new(pt_w, pt_h);
        rep.setSize(point_size);
        let image = NSImage::initWithSize(NSImage::alloc(), point_size);
        image.addRepresentation(&rep);
        image.setTemplate(icon.template);
        if !description.is_empty() {
            image.setAccessibilityDescription(Some(&NSString::from_str(description)));
        }
        Ok(image)
    })
}

fn new_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    crate::native_menu::set_layout_direction(&menu);
    menu
}

fn fill_menu(
    mtm: MainThreadMarker,
    menu: &NSMenu,
    items: &[TrayItem],
    on_event: Handler,
) -> Retained<MenuTarget> {
    let entries: Vec<MenuEntry> = items
        .iter()
        .filter_map(|item| match item {
            TrayItem::Action { id, enabled, .. } | TrayItem::Check { id, enabled, .. } => {
                Some(MenuEntry { id: id.clone(), enabled: *enabled })
            }
            TrayItem::Separator => None,
        })
        .collect();
    // SAFETY: main-thread NSObject initialization immediately following set_ivars.
    let target: Retained<MenuTarget> =
        unsafe { msg_send![super(mtm.alloc().set_ivars(MenuIvars { entries, on_event })), init] };
    let target_obj: &AnyObject = &target;
    let mut tag = 0isize;
    for item in items {
        match item {
            TrayItem::Action { label, enabled, .. } => {
                let ns_item = make_menu_item(mtm, label, *enabled, None, target_obj, tag);
                tag += 1;
                menu.addItem(&ns_item);
            }
            TrayItem::Check { label, enabled, checked, .. } => {
                let ns_item = make_menu_item(mtm, label, *enabled, Some(*checked), target_obj, tag);
                tag += 1;
                menu.addItem(&ns_item);
            }
            TrayItem::Separator => {
                menu.addItem(&NSMenuItem::separatorItem(mtm));
            }
        }
    }
    target
}

fn make_menu_item(
    mtm: MainThreadMarker,
    label: &str,
    enabled: bool,
    checked: Option<bool>,
    target_obj: &AnyObject,
    tag: isize,
) -> Retained<NSMenuItem> {
    // SAFETY: main-thread NSMenuItem; `target` is kept alive in `ButtonTarget::last_menu`
    // until replaced or removed, and `clear_menu_targets` clears it before release.
    let ns_item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc(),
            &NSString::from_str(label),
            Some(sel!(runeTrayMenuPick:)),
            &NSString::from_str(""),
        )
    };
    // SAFETY: `target_obj` responds to `runeTrayMenuPick:`.
    unsafe { ns_item.setTarget(Some(target_obj)) };
    ns_item.setTag(tag);
    ns_item.setEnabled(enabled);
    if let Some(is_checked) = checked {
        ns_item.setState(if is_checked { 1 } else { 0 });
    }
    ns_item
}

fn clear_menu_targets(menu: &NSMenu) {
    for i in 0..menu.numberOfItems() {
        let Some(item) = menu.itemAtIndex(i) else { continue };
        // SAFETY: main-thread menu item; clearing target/action prevents sends after release.
        unsafe {
            item.setTarget(None);
            item.setAction(None);
        }
    }
}

fn alloc_button_target(
    mtm: MainThreadMarker,
    id: u64,
    model: Arc<Mutex<Model>>,
    on_event: Handler,
    menu_on_primary_click: bool,
) -> Retained<ButtonTarget> {
    let ivars = ButtonIvars {
        id,
        model,
        on_event,
        menu_on_primary_click,
        own_menu: RefCell::new(None),
        showing_menu: Cell::new(false),
        last_menu: RefCell::new(None),
    };
    // SAFETY: main-thread NSObject initialization immediately following set_ivars.
    unsafe { msg_send![super(mtm.alloc().set_ivars(ivars)), init] }
}

fn install_native(
    mtm: MainThreadMarker,
    id: u64,
    model: Arc<Mutex<Model>>,
    target: Retained<ButtonTarget>,
    host: Rc<dyn StatusItemHost>,
    image: &NSImage,
) -> Native {
    host.set_image(image);
    let initial_tip = model.lock().unwrap_or_else(|e| e.into_inner()).tooltip.clone();
    host.set_tooltip(non_empty(&initial_tip));
    model.lock().unwrap_or_else(|e| e.into_inner()).rect = host.screen_rect(mtm);
    if target.ivars().menu_on_primary_click {
        let menu = new_menu(mtm);
        menu.setDelegate(Some(ProtocolObject::from_ref(&*target)));
        *target.ivars().own_menu.borrow_mut() = Some(menu);
    }
    ITEMS.with(|map| {
        map.borrow_mut().insert(id, LiveItem { host, target, model: model.clone() });
    });
    apply_menu(id);
    Native { id, model }
}

fn live_entry(id: u64) -> Option<(Rc<dyn StatusItemHost>, Arc<Mutex<Model>>)> {
    ITEMS.with(|map| {
        map.borrow().get(&id).map(|live| (live.host.clone(), live.model.clone()))
    })
}

fn apply_menu(id: u64) {
    let Some((host, model)) = live_entry(id) else { return };
    let own = ITEMS.with(|map| map.borrow().get(&id).and_then(|live| live.target.ivars().own_menu.borrow().clone()));
    let Some(menu) = own else { return };
    let has_menu = !model.lock().unwrap_or_else(|e| e.into_inner()).menu.is_empty();
    host.set_menu(has_menu.then_some(&*menu));
}

fn apply_tooltip(id: u64) {
    let Some((host, model)) = live_entry(id) else { return };
    let tip = model.lock().unwrap_or_else(|e| e.into_inner()).tooltip.clone();
    host.set_tooltip(non_empty(&tip));
}

fn apply_title(id: u64, mtm: MainThreadMarker) {
    let Some((host, model)) = live_entry(id) else { return };
    let title = model.lock().unwrap_or_else(|e| e.into_inner()).title.clone();
    host.set_title(&title);
    model.lock().unwrap_or_else(|e| e.into_inner()).rect = host.screen_rect(mtm);
}

fn apply_icon(id: u64, mtm: MainThreadMarker) -> Result<(), TrayError> {
    let Some((host, model)) = live_entry(id) else { return Ok(()) };
    let (icon, thickness, label) = {
        let m = model.lock().unwrap_or_else(|e| e.into_inner());
        (m.icon.clone(), m.thickness, m.accessibility_label.clone())
    };
    let image = make_image(&icon, thickness, &label)?;
    host.set_image(&image);
    model.lock().unwrap_or_else(|e| e.into_inner()).rect = host.screen_rect(mtm);
    Ok(())
}

fn refresh_rect(id: u64, mtm: MainThreadMarker) -> Option<[i32; 4]> {
    let (host, model) = live_entry(id)?;
    let rect = host.screen_rect(mtm);
    model.lock().unwrap_or_else(|e| e.into_inner()).rect = rect;
    rect
}

fn remove_item(id: u64) {
    let Some(live) = ITEMS.with(|map| map.borrow_mut().remove(&id)) else { return };
    if let Some((menu, _)) = live.target.ivars().last_menu.borrow_mut().take() {
        clear_menu_targets(&menu);
    }
    if let Some(menu) = live.target.ivars().own_menu.borrow_mut().take() {
        menu.setDelegate(None);
    }
    live.host.remove();
}

fn remove_all_on_main() {
    let ids: Vec<u64> = ITEMS.with(|map| map.borrow().keys().copied().collect());
    for id in ids {
        remove_item(id);
    }
}

pub(super) fn remove_all() {
    if MainThreadMarker::new().is_some() {
        remove_all_on_main();
    } else {
        crate::platform::main_queue::on_main(remove_all_on_main);
    }
}

pub(super) struct Native {
    id: u64,
    model: Arc<Mutex<Model>>,
}

impl Native {
    pub(super) fn new(spec: TraySpec, on_event: Handler) -> Result<Self, TrayError> {
        let mtm = MainThreadMarker::new().ok_or_else(|| {
            TrayError::Backend("macOS tray must be created on the main thread".into())
        })?;
        let bar = NSStatusBar::systemStatusBar();
        let thickness = valid_thickness(bar.thickness());
        let label = accessibility_label(&spec).to_owned();
        let image = make_image(&spec.icon, thickness, &label)?;
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let model = Arc::new(Mutex::new(Model {
            accessibility_label: label,
            title: String::new(),
            tooltip: spec.tooltip,
            icon: spec.icon,
            thickness,
            menu: spec.menu,
            rect: None,
        }));
        let target = alloc_button_target(mtm, id, model.clone(), on_event, spec.menu_on_primary_click);
        let host = Rc::new(SystemStatusItem::new(mtm, bar, &target)?);
        Ok(install_native(mtm, id, model, target, host, &image))
    }

    pub(super) fn set_menu(&self, menu: Vec<TrayItem>) {
        self.model.lock().unwrap_or_else(|e| e.into_inner()).menu = menu;
        let id = self.id;
        if MainThreadMarker::new().is_some() {
            apply_menu(id);
        } else {
            crate::platform::main_queue::on_main(move || apply_menu(id));
        }
    }

    pub(super) fn set_tooltip(&self, tooltip: String) {
        self.model.lock().unwrap_or_else(|e| e.into_inner()).tooltip = tooltip;
        let id = self.id;
        if MainThreadMarker::new().is_some() {
            apply_tooltip(id);
        } else {
            crate::platform::main_queue::on_main(move || apply_tooltip(id));
        }
    }

    pub(super) fn set_icon(&self, icon: TrayIcon) -> Result<(), TrayError> {
        let thickness = self.model.lock().unwrap_or_else(|e| e.into_inner()).thickness;
        validate_icon_layout(&icon, thickness)?;
        self.model.lock().unwrap_or_else(|e| e.into_inner()).icon = icon;
        let id = self.id;
        if let Some(mtm) = MainThreadMarker::new() {
            apply_icon(id, mtm)
        } else {
            crate::platform::main_queue::on_main(move || {
                if let Some(mtm) = MainThreadMarker::new() {
                    apply_icon(id, mtm).ok();
                }
            });
            Ok(())
        }
    }

    pub(super) fn set_icon_name(&self, _name: String) {}

    pub(super) fn set_accessibility_label(&self, label: String) {
        self.model.lock().unwrap_or_else(|e| e.into_inner()).accessibility_label = label;
        let id = self.id;
        if let Some(mtm) = MainThreadMarker::new() {
            apply_icon(id, mtm).ok();
        } else {
            crate::platform::main_queue::on_main(move || {
                if let Some(mtm) = MainThreadMarker::new() {
                    apply_icon(id, mtm).ok();
                }
            });
        }
    }

    pub(super) fn set_title(&self, title: String) {
        self.model.lock().unwrap_or_else(|e| e.into_inner()).title = title;
        let id = self.id;
        if let Some(mtm) = MainThreadMarker::new() {
            apply_title(id, mtm);
        } else {
            crate::platform::main_queue::on_main(move || {
                if let Some(mtm) = MainThreadMarker::new() {
                    apply_title(id, mtm);
                }
            });
        }
    }

    pub(super) fn rect(&self) -> Option<[i32; 4]> {
        let id = self.id;
        if let Some(mtm) = MainThreadMarker::new() {
            refresh_rect(id, mtm)
        } else {
            let cached = self.model.lock().unwrap_or_else(|e| e.into_inner()).rect;
            crate::platform::main_queue::on_main(move || {
                if let Some(mtm) = MainThreadMarker::new() {
                    let _ = refresh_rect(id, mtm);
                }
            });
            cached
        }
    }
}

impl Drop for Native {
    fn drop(&mut self) {
        let id = self.id;
        if MainThreadMarker::new().is_some() {
            remove_item(id);
        } else {
            crate::platform::main_queue::on_main(move || remove_item(id));
        }
    }
}

#[cfg(test)]
#[path = "macos_tests.rs"]
mod tests;

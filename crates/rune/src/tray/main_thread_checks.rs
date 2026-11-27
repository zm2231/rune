use super::*;
use std::sync::atomic::AtomicUsize;

struct FakeStatusItem {
    image: RefCell<Option<Retained<NSImage>>>,
    titles: RefCell<Vec<String>>,
    tooltips: RefCell<Vec<Option<String>>>,
    highlights: RefCell<Vec<bool>>,
    rect: Cell<Option<[i32; 4]>>,
    attached: RefCell<Option<Retained<NSMenu>>>,
    popped_menus: RefCell<Vec<Retained<NSMenu>>>,
    removed: Cell<bool>,
    on_pop_up: RefCell<Option<Box<dyn Fn(&NSMenu)>>>,
}

impl FakeStatusItem {
    fn new() -> Self {
        Self {
            image: RefCell::new(None),
            titles: RefCell::new(Vec::new()),
            tooltips: RefCell::new(Vec::new()),
            highlights: RefCell::new(Vec::new()),
            rect: Cell::new(Some([2400, 0, 56, 48])),
            attached: RefCell::new(None),
            popped_menus: RefCell::new(Vec::new()),
            removed: Cell::new(false),
            on_pop_up: RefCell::new(None),
        }
    }
}

impl StatusItemHost for FakeStatusItem {
    fn set_image(&self, image: &NSImage) {
        *self.image.borrow_mut() = Some(Retained::from(image));
    }

    fn set_title(&self, title: &str) {
        self.titles.borrow_mut().push(title.to_owned());
    }

    fn set_tooltip(&self, tooltip: Option<&str>) {
        self.tooltips.borrow_mut().push(tooltip.map(str::to_owned));
    }

    fn set_highlighted(&self, highlighted: bool) {
        self.highlights.borrow_mut().push(highlighted);
    }

    fn screen_rect(&self, _mtm: MainThreadMarker) -> Option<[i32; 4]> {
        if self.removed.get() { None } else { self.rect.get() }
    }

    fn set_menu(&self, menu: Option<&NSMenu>) {
        *self.attached.borrow_mut() = menu.map(Retained::from);
    }

    fn perform_click(&self) {
        let Some(menu) = self.attached.borrow().clone() else { return };
        if let Some(delegate) = menu.delegate() {
            // SAFETY: the delegate is the tray's `ButtonTarget`, which implements `menuNeedsUpdate:`.
            let _: () = unsafe { msg_send![&*delegate, menuNeedsUpdate: &*menu] };
        }
        self.popped_menus.borrow_mut().push(menu.clone());
        if let Some(cb) = self.on_pop_up.borrow().as_ref() {
            cb(&menu);
        }
    }

    fn remove(&self) {
        self.removed.set(true);
    }
}

fn new_with_fake(
    mtm: MainThreadMarker,
    spec: TraySpec,
    thickness: f64,
    on_event: impl Fn(TrayEvent) + Send + Sync + 'static,
) -> Result<(Native, Rc<FakeStatusItem>, Retained<ButtonTarget>), TrayError> {
    let thickness = valid_thickness(thickness);
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
        highlighted: false,
    }));
    let target = alloc_button_target(mtm, id, model.clone(), Arc::new(on_event), spec.menu_on_primary_click);
    let fake = Rc::new(FakeStatusItem::new());
    let host: Rc<dyn StatusItemHost> = fake.clone();
    let native = install_native(mtm, id, model, target.clone(), host, &image);
    Ok((native, fake, target))
}

pub fn fake_host_lifecycle_menu_tooltip_clicks_and_drop(mtm: MainThreadMarker) {
    let events: Arc<Mutex<Vec<TrayEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let spec = TraySpec::new("dev.rune.test", "Test Tray", TrayIcon::rgba(32, 32, vec![255; 32 * 32 * 4]).template(true))
        .tooltip("Initial tip")
        .menu(vec![
            TrayItem::action("open", "Open"),
            TrayItem::Separator,
            TrayItem::action("disabled", "Disabled").enabled(false),
            TrayItem::action("quit", "Quit"),
        ]);

    let (native, fake, target) = new_with_fake(mtm, spec, 22.0, move |ev| {
        sink.lock().unwrap().push(ev);
    })
    .unwrap();

    // Image & initial tooltip applied.
    let img = fake.image.borrow().clone().expect("image set");
    assert_eq!(img.size(), NSSize::new(22.0, 22.0));
    assert!(img.isTemplate());
    assert_eq!(img.accessibilityDescription().map(|s| s.to_string()), Some("Test Tray".into()));
    assert_eq!(*fake.tooltips.borrow(), vec![Some("Initial tip".into())]);

    // Tooltip updates including clearing to empty -> None.
    native.set_tooltip("Updated tip".into());
    native.set_tooltip(String::new());
    assert_eq!(
        *fake.tooltips.borrow(),
        vec![Some("Initial tip".into()), Some("Updated tip".into()), None]
    );

    // Primary click sends Activate { token: None }.
    target.handle_click(mtm, TrayClick::Primary);
    assert_eq!(*events.lock().unwrap(), vec![TrayEvent::Activate { token: None }]);

    // Secondary click builds and pops the flat NSMenu.
    target.handle_click(mtm, TrayClick::Secondary);
    assert_eq!(fake.popped_menus.borrow().len(), 1);
    assert!(fake.attached.borrow().is_none());
    let menu = fake.popped_menus.borrow()[0].clone();
    assert!(!menu.autoenablesItems());
    assert_eq!(menu.numberOfItems(), 4);
    let open = menu.itemAtIndex(0).unwrap();
    let sep = menu.itemAtIndex(1).unwrap();
    let disabled = menu.itemAtIndex(2).unwrap();
    let quit = menu.itemAtIndex(3).unwrap();
    assert_eq!(open.title().to_string(), "Open");
    assert!(open.isEnabled());
    assert!(sep.isSeparatorItem());
    assert_eq!(disabled.title().to_string(), "Disabled");
    assert!(!disabled.isEnabled());
    assert_eq!(quit.title().to_string(), "Quit");
    assert!(quit.isEnabled());

    // Invoking action on enabled items fires Menu(id); disabled item does not.
    // SAFETY: invoking the ObjC target/action installed on the main-thread test menu items.
    unsafe {
        let open_target = open.target().unwrap();
        let _: () = msg_send![&*open_target, runeTrayMenuPick: &*open];
        let disabled_target = disabled.target().unwrap();
        let _: () = msg_send![&*disabled_target, runeTrayMenuPick: &*disabled];
        let quit_target = quit.target().unwrap();
        let _: () = msg_send![&*quit_target, runeTrayMenuPick: &*quit];
    }
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            TrayEvent::Activate { token: None },
            TrayEvent::Menu("open".into()),
            TrayEvent::Menu("quit".into()),
        ]
    );

    // Updating the menu changes subsequent pop-ups and clears old menu targets.
    native.set_menu(vec![TrayItem::action("resume", "Resume")]);
    target.handle_click(mtm, TrayClick::Secondary);
    assert_eq!(fake.popped_menus.borrow().len(), 2);
    assert!(open.target().is_none(), "previous menu targets cleared on replacement");
    let new_menu = fake.popped_menus.borrow()[1].clone();
    assert_eq!(new_menu.numberOfItems(), 1);
    assert_eq!(new_menu.itemAtIndex(0).unwrap().title().to_string(), "Resume");

    // Empty menu does not pop up.
    native.set_menu(Vec::new());
    target.handle_click(mtm, TrayClick::Secondary);
    assert_eq!(fake.popped_menus.borrow().len(), 2);

    // Drop removes the status item and clears the latest menu's targets.
    assert!(!fake.removed.get());
    drop(native);
    assert!(fake.removed.get());
    assert!(new_menu.itemAtIndex(0).unwrap().target().is_none());
}

pub fn callback_panic_is_caught_and_reentrant_menu_click_is_ignored(mtm: MainThreadMarker) {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let spec = TraySpec::new("dev.rune.panic", "Panic", TrayIcon::rgba(16, 16, vec![255; 16 * 16 * 4]))
        .menu(vec![TrayItem::action("a", "A")]);
    let (native, fake, target) = new_with_fake(mtm, spec, 22.0, move |_| {
        count.fetch_add(1, Ordering::SeqCst);
        panic!("boom");
    })
    .unwrap();

    // Panicking Activate callback does not unwind.
    target.handle_click(mtm, TrayClick::Primary);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let target_for_cb = target.clone();
    *fake.on_pop_up.borrow_mut() = Some(Box::new(move |_| {
        target_for_cb.handle_click(mtm, TrayClick::Secondary);
    }));
    target.handle_click(mtm, TrayClick::Secondary);
    assert_eq!(fake.popped_menus.borrow().len(), 1);
    drop(native);
}

pub fn fake_host_set_icon_set_title_rect_and_check_menu_items(mtm: MainThreadMarker) {
    let events: Arc<Mutex<Vec<TrayEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let spec = TraySpec::new(
        "dev.rune.live",
        "Live Status",
        TrayIcon::rgba(32, 32, vec![255; 32 * 32 * 4]).template(true),
    )
    .menu(vec![
        TrayItem::action("open", "Open"),
        TrayItem::check("hud", "Show HUD", false),
        TrayItem::check("alert", "Alert", true),
        TrayItem::check("locked", "Locked", true).enabled(false),
    ]);

    let (native, fake, target) = new_with_fake(mtm, spec, 22.0, move |ev| {
        sink.lock().unwrap().push(ev);
    })
    .unwrap();

    // Initial rect comes from host; live updates reflect immediately on main thread.
    assert_eq!(native.rect(), Some([2400, 0, 56, 48]));
    fake.rect.set(Some([2320, 0, 92, 48]));
    assert_eq!(native.rect(), Some([2320, 0, 92, 48]));

    // set_title updates host and supports clearing back to empty.
    assert!(fake.titles.borrow().is_empty(), "Tray::new keeps title text unset");
    native.set_title("42%".into());
    native.set_title(String::new());
    assert_eq!(*fake.titles.borrow(), vec!["42%".to_owned(), String::new()]);

    // set_icon_name is a clean no-op on macOS.
    native.set_icon_name("ignored-symbolic".into());

    // set_icon swaps the NSImage (updating size/template while preserving accessibility label).
    let wide_alert = TrayIcon::rgba(48, 24, vec![255, 64, 64, 255].repeat(48 * 24));
    native.set_icon(wide_alert).unwrap();
    let img = fake.image.borrow().clone().expect("updated image");
    assert_eq!(img.size(), NSSize::new(44.0, 22.0));
    assert!(!img.isTemplate());
    assert_eq!(img.accessibilityDescription().map(|s| s.to_string()), Some("Live Status".into()));

    native.set_accessibility_label("Live Status unavailable".into());
    let img = fake.image.borrow().clone().expect("relabelled image");
    assert_eq!(img.accessibilityDescription().map(|s| s.to_string()), Some("Live Status unavailable".into()));
    assert_eq!(img.size(), NSSize::new(44.0, 22.0));
    native.set_icon(TrayIcon::rgba(32, 32, vec![255; 32 * 32 * 4])).unwrap();
    let img = fake.image.borrow().clone().expect("updated image");
    assert_eq!(img.accessibilityDescription().map(|s| s.to_string()), Some("Live Status unavailable".into()));
    native.set_icon(TrayIcon::rgba(48, 24, vec![255, 64, 64, 255].repeat(48 * 24))).unwrap();

    // Invalid icons are rejected without replacing the current NSImage.
    assert!(matches!(native.set_icon(TrayIcon::rgba(0, 16, Vec::new())), Err(TrayError::Backend(_))));
    assert!(matches!(native.set_icon(TrayIcon::rgba(4, 4, vec![0; 8])), Err(TrayError::Backend(_))));
    assert_eq!(fake.image.borrow().as_ref().unwrap().size(), NSSize::new(44.0, 22.0));

    // Check menu items set NSMenuItem.state (1 = on, 0 = off) and dispatch enabled clicks.
    target.handle_click(mtm, TrayClick::Secondary);
    let menu = fake.popped_menus.borrow()[0].clone();
    assert_eq!(menu.numberOfItems(), 4);
    let open = menu.itemAtIndex(0).unwrap();
    let hud = menu.itemAtIndex(1).unwrap();
    let alert = menu.itemAtIndex(2).unwrap();
    let locked = menu.itemAtIndex(3).unwrap();
    assert_eq!(open.state(), 0);
    assert_eq!(hud.state(), 0);
    assert!(hud.isEnabled());
    assert_eq!(alert.state(), 1);
    assert!(alert.isEnabled());
    assert_eq!(locked.state(), 1);
    assert!(!locked.isEnabled());

    // SAFETY: invoking target/action installed on the main-thread test menu items.
    unsafe {
        let hud_target = hud.target().unwrap();
        let _: () = msg_send![&*hud_target, runeTrayMenuPick: &*hud];
        let locked_target = locked.target().unwrap();
        let _: () = msg_send![&*locked_target, runeTrayMenuPick: &*locked];
    }
    assert_eq!(*events.lock().unwrap(), vec![TrayEvent::Menu("hud".into())]);

    // Flipping checked state via set_menu updates subsequent pop-ups.
    native.set_menu(vec![TrayItem::check("hud", "Show HUD", true)]);
    target.handle_click(mtm, TrayClick::Secondary);
    let next_menu = fake.popped_menus.borrow()[1].clone();
    assert_eq!(next_menu.itemAtIndex(0).unwrap().state(), 1);

    // Reading rect off the main thread returns the cached reading without blocking.
    let native = Arc::new(native);
    let for_thread = native.clone();
    let off_main_rect = std::thread::spawn(move || for_thread.rect()).join().unwrap();
    assert_eq!(off_main_rect, Some([2320, 0, 92, 48]));

    // Removing all tray items on exit hides the status item immediately before Drop runs.
    remove_all_on_main();
    assert!(fake.removed.get(), "remove_all must remove live status items immediately");
    assert_eq!(native.rect(), None);

    drop(native);
}

pub fn menu_on_primary_click_attaches_a_menu_filled_when_it_opens(mtm: MainThreadMarker) {
    let events: Arc<Mutex<Vec<TrayEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let spec = TraySpec::new("dev.rune.extra", "Extra", TrayIcon::rgba(16, 16, vec![255; 16 * 16 * 4]))
        .menu(vec![TrayItem::action("open", "Open")])
        .menu_on_primary_click(true);
    let (native, fake, target) = new_with_fake(mtm, spec, 22.0, move |ev| sink.lock().unwrap().push(ev)).unwrap();

    let attached = fake.attached.borrow().clone().expect("the status item owns the menu");
    assert!(attached.delegate().is_some());

    native.set_menu(vec![TrayItem::action("resume", "Resume"), TrayItem::action("quit", "Quit")]);
    fake.perform_click();
    assert_eq!(attached.numberOfItems(), 2);
    assert_eq!(attached.itemAtIndex(0).unwrap().title().to_string(), "Resume");
    let quit = attached.itemAtIndex(1).unwrap();
    // SAFETY: invoking the target/action installed on the main-thread test menu item.
    unsafe {
        let quit_target = quit.target().unwrap();
        let _: () = msg_send![&*quit_target, runeTrayMenuPick: &*quit];
    }
    assert_eq!(*events.lock().unwrap(), vec![TrayEvent::Menu("quit".into())]);

    target.handle_click(mtm, TrayClick::Primary);
    assert_eq!(fake.popped_menus.borrow().len(), 2);
    assert!(fake.attached.borrow().is_some(), "the menu stays attached after it opens");

    native.set_menu(Vec::new());
    assert!(fake.attached.borrow().is_none());
    target.handle_click(mtm, TrayClick::Primary);
    target.handle_click(mtm, TrayClick::Secondary);
    assert_eq!(
        *events.lock().unwrap(),
        vec![TrayEvent::Menu("quit".into()), TrayEvent::Activate { token: None }]
    );

    drop(native);
    assert!(attached.delegate().is_none());
    assert!(fake.removed.get());
}

pub fn set_highlighted_survives_icon_title_and_menu_updates(mtm: MainThreadMarker) {
    let spec = TraySpec::new("dev.rune.pill", "Pill", TrayIcon::rgba(16, 16, vec![255; 16 * 16 * 4]))
        .menu(vec![TrayItem::action("a", "A")]);
    let (native, fake, target) = new_with_fake(mtm, spec, 22.0, |_| {}).unwrap();

    native.set_title("1".into());
    native.set_icon(TrayIcon::rgba(16, 16, vec![0; 16 * 16 * 4])).unwrap();
    assert!(fake.highlights.borrow().is_empty());

    native.set_highlighted(true);
    native.set_title("2".into());
    native.set_icon(TrayIcon::rgba(16, 16, vec![255; 16 * 16 * 4])).unwrap();
    assert_eq!(*fake.highlights.borrow(), vec![true; 3]);
    target.handle_click(mtm, TrayClick::Secondary);
    assert_eq!(fake.popped_menus.borrow().len(), 1);

    native.set_highlighted(false);
    native.set_title("3".into());
    assert_eq!(*fake.highlights.borrow(), vec![true, true, true, false]);
    drop(native);
}

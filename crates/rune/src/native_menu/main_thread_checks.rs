use super::*;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

pub fn macos_native_submenus_build_fire_and_clear(mtm: MainThreadMarker) {
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.setMainMenu(None);

    let hits: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink_hits = hits.clone();
    let sink: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |id: &str| {
        sink_hits.lock().unwrap().push(id.to_owned());
    });

    let items: Vec<MenuItem<()>> = vec![
        MenuItem::submenu("Attach", [
            MenuItem::submenu("prod", [
                MenuItem::with_action("main", |_, _| {}).id("prod:main").checked(true),
                MenuItem::separator(),
                MenuItem::with_action("logs", |_, _| {}).id("prod:logs").enabled(false),
            ]),
            MenuItem::submenu("offline", [
                MenuItem::with_action("unreachable", |_, _| {}).id("offline:unreachable"),
            ])
            .enabled(false),
            MenuItem::submenu("Empty", []),
        ])
        .id("attach-sub")
        .menu("Session"),
        MenuItem::with_action("Detach", |_, _| {})
            .id("detach")
            .menu("Session")
            .after("attach-sub"),
        MenuItem::submenu("Disabled Top", [
            MenuItem::with_action("Nope", |_, _| {}).id("disabled-top:nope"),
        ])
        .enabled(false)
        .menu("Session"),
    ];

    let mut actions = Vec::new();
    let descriptions: Vec<_> = items.into_iter().map(|i| i.split(&mut actions)).collect();
    validate(&descriptions).unwrap();

    let mut native = macos::Native::new().unwrap();
    native.set(&descriptions, sink).unwrap();

    {
        let root = app.mainMenu().expect("main menu installed");
        let session_top = (0..root.numberOfItems())
            .filter_map(|i| root.itemAtIndex(i))
            .find(|i| default_macos::name(i) == "Session")
            .expect("Session top-level menu");
        let session = session_top.submenu().expect("Session submenu");
        // Update parent menu as AppKit does before tracking, to verify disabled submenus stay disabled.
        session.update();
        assert_eq!(session.numberOfItems(), 3);

        let attach_item = session.itemAtIndex(0).unwrap();
        let detach_item = session.itemAtIndex(1).unwrap();
        let disabled_top_item = session.itemAtIndex(2).unwrap();
        assert_eq!(attach_item.title().to_string(), "Attach");
        assert!(attach_item.isEnabled());
        assert_eq!(detach_item.title().to_string(), "Detach");
        assert_eq!(disabled_top_item.title().to_string(), "Disabled Top");
        assert!(!disabled_top_item.isEnabled(), "disabled top-level submenu must stay disabled after menu.update()");

        let attach_menu = attach_item.submenu().expect("Attach NSMenu");
        attach_menu.update();
        assert_eq!(attach_menu.numberOfItems(), 3);
        let prod_item = attach_menu.itemAtIndex(0).unwrap();
        let offline_item = attach_menu.itemAtIndex(1).unwrap();
        let empty_item = attach_menu.itemAtIndex(2).unwrap();
        assert_eq!(prod_item.title().to_string(), "prod");
        assert!(prod_item.isEnabled());
        assert_eq!(offline_item.title().to_string(), "offline");
        assert!(!offline_item.isEnabled(), "disabled nested submenu must stay disabled");
        assert_eq!(empty_item.title().to_string(), "Empty");
        assert_eq!(empty_item.submenu().unwrap().numberOfItems(), 0);

        // Deeply nested leaf items: checked state, separator, disabled leaf, and firing via AppKit.
        let prod_menu = prod_item.submenu().expect("prod NSMenu");
        assert_eq!(prod_menu.numberOfItems(), 3);
        let main_item = prod_menu.itemAtIndex(0).unwrap();
        let sep_item = prod_menu.itemAtIndex(1).unwrap();
        let logs_item = prod_menu.itemAtIndex(2).unwrap();
        assert_eq!(main_item.title().to_string(), "main");
        assert_eq!(main_item.state(), 1);
        assert!(main_item.isEnabled());
        assert!(sep_item.isSeparatorItem());
        assert_eq!(logs_item.title().to_string(), "logs");
        assert!(!logs_item.isEnabled());

        prod_menu.performActionForItemAtIndex(0);
        prod_menu.performActionForItemAtIndex(2);
        // Child inside disabled submenu must not fire even if invoked directly.
        let offline_menu = offline_item.submenu().expect("offline NSMenu");
        let unreachable_item = offline_menu.itemAtIndex(0).unwrap();
        assert!(!unreachable_item.isEnabled());
        offline_menu.performActionForItemAtIndex(0);
        assert_eq!(*hits.lock().unwrap(), ["prod:main"]);

        // Dropping Native clears nested targets recursively and removes the empty Session menu.
        drop(native);
        assert!(main_item.target().is_none(), "nested leaf target must be cleared on teardown");
        assert!(main_item.action().is_none(), "nested leaf action must be cleared on teardown");
        assert!(unreachable_item.target().is_none(), "disabled nested leaf target must be cleared");
        let still_has_session = (0..root.numberOfItems())
            .filter_map(|i| root.itemAtIndex(i))
            .any(|i| default_macos::name(&i) == "Session");
        assert!(!still_has_session, "empty custom top-level menu should be removed on teardown");
        app.setMainMenu(None);
    }
}

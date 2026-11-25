use std::cell::RefCell;

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_foundation::{NSObject, NSString};
use objc2_ui_kit::{
    UIApplication, UIApplicationDelegate, UIResponder, UIScene, UISceneConfiguration,
    UISceneConnectionOptions, UISceneDelegate, UISceneSession, UIViewController, UIWindow, UIWindowScene,
    UIWindowSceneDelegate,
};
use rune::AppSpec;

use super::host::{Factory, factory};
use super::view::RuneView;

thread_local! {
    static APP: RefCell<Option<Factory>> = const { RefCell::new(None) };
}

define_class!(
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneAppDelegate"]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl UIApplicationDelegate for AppDelegate {
        #[unsafe(method_id(application:configurationForConnectingSceneSession:options:))]
        fn configuration(
            &self,
            _app: &UIApplication,
            session: &UISceneSession,
            _options: &UISceneConnectionOptions,
        ) -> Retained<UISceneConfiguration> {
            let config = UISceneConfiguration::configurationWithName_sessionRole(None, &session.role(), self.mtm());
            // SAFETY: `SceneDelegate` is a `UIResponder` that conforms to `UIWindowSceneDelegate`.
            unsafe { config.setDelegateClass(Some(SceneDelegate::class())) };
            config
        }
    }
);

#[derive(Default)]
struct Scene {
    window: RefCell<Option<Retained<UIWindow>>>,
    view: RefCell<Option<Retained<RuneView>>>,
}

define_class!(
    // UIKit makes the scene delegate itself, with `init`.
    #[unsafe(super(UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RuneSceneDelegate"]
    #[ivars = Scene]
    struct SceneDelegate;

    impl SceneDelegate {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Retained<Self> {
            let this = this.set_ivars(Scene::default());
            // SAFETY: `UIResponder`'s `init`, on an allocated object whose ivars are set.
            unsafe { msg_send![super(this), init] }
        }
    }

    unsafe impl NSObjectProtocol for SceneDelegate {}

    unsafe impl UISceneDelegate for SceneDelegate {
        #[unsafe(method(scene:willConnectToSession:options:))]
        fn will_connect(&self, scene: &UIScene, _session: &UISceneSession, _options: &UISceneConnectionOptions) {
            let Some(scene) = AsRef::<AnyObject>::as_ref(scene).downcast_ref::<UIWindowScene>() else { return };
            let Some(app) = APP.with(|app| app.borrow().clone()) else { return };
            let mtm = self.mtm();
            let window = UIWindow::initWithWindowScene(mtm.alloc(), scene);
            let view = RuneView::with_factory(mtm, app);
            let controller = UIViewController::new(mtm);
            controller.setView(Some(&view));
            window.setRootViewController(Some(&controller));
            window.makeKeyAndVisible();
            *self.ivars().window.borrow_mut() = Some(window);
            *self.ivars().view.borrow_mut() = Some(view);
        }

        #[unsafe(method(sceneDidDisconnect:))]
        fn did_disconnect(&self, _scene: &UIScene) {
            self.ivars().view.borrow_mut().take();
            self.ivars().window.borrow_mut().take();
        }

        #[unsafe(method(sceneDidEnterBackground:))]
        fn did_enter_background(&self, _scene: &UIScene) {
            if let Some(view) = &*self.ivars().view.borrow() {
                view.enter_background();
            }
        }

        #[unsafe(method(sceneWillEnterForeground:))]
        fn will_enter_foreground(&self, _scene: &UIScene) {
            if let Some(view) = &*self.ivars().view.borrow() {
                view.enter_foreground();
            }
        }
    }

    unsafe impl UIWindowSceneDelegate for SceneDelegate {
        #[unsafe(method_id(window))]
        fn window(&self) -> Option<Retained<UIWindow>> {
            self.ivars().window.borrow().clone()
        }

        #[unsafe(method(setWindow:))]
        fn set_window(&self, window: Option<&UIWindow>) {
            *self.ivars().window.borrow_mut() = window.map(|w| w.retain());
        }
    }
);

/// Runs `spec` as the app: starts UIKit with a scene whose window a [`RuneView`] fills, and
/// never returns. Call it from `main`, on the main thread.
pub fn run<S: 'static>(spec: AppSpec<S>) -> ! {
    let Some(mtm) = MainThreadMarker::new() else { panic!("rune_ios::run must be called on the main thread") };
    APP.with(|app| *app.borrow_mut() = Some(factory(spec)));
    let _ = SceneDelegate::class();
    UIApplication::main(None, Some(&NSString::from_class(AppDelegate::class())), mtm)
}

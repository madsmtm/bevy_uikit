#![expect(non_snake_case, reason = "UIKit does not use Rust naming conventions")]
use std::cell::{Cell, RefCell, RefMut};

use bevy_app::{App, AppExit, PluginsState};
use bevy_ecs::entity::Entity;
use bevy_ecs::message::{Message, MessageReader};
use bevy_ecs::query::{QuerySingleError, With};
use bevy_tasks::tick_global_task_pools_on_main_thread;
use bevy_window::{PrimaryWindow, Window, WindowCreated, WindowEvent};
use dispatch2::MainThreadBound;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2::{available, define_class, msg_send, ClassType, MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{kCFRunLoopDefaultMode, CFRunLoop};
use objc2_foundation::{
    ns_string, NSDictionary, NSObject, NSObjectProtocol, NSSet, NSString, NSURL,
};
#[allow(deprecated)]
use objc2_ui_kit::UIApplicationOpenURLOptionsKey;
use objc2_ui_kit::{
    UIApplication, UIApplicationDelegate, UIApplicationLaunchOptionsKey, UISceneConfiguration,
    UISceneConnectionOptions, UISceneSession, UIWindow,
};
use tracing::{error, trace, warn};

use crate::scene_delegate::SceneDelegate;
use crate::windows::{setup_window, WorldHelper};
use crate::UIKitWindows;

/// The [`App::runner`] for the [`UIKitPlugin`](crate::UIKitPlugin) plugin.
pub fn uikit_runner(mut app: App) -> AppExit {
    let mtm = MainThreadMarker::new().expect("UIKit applications must be run on the main thread");

    trace!("polling plugins until they're ready");
    // TODO: Is this sufficient for making plugins ready?
    while app.plugins_state() == PluginsState::Adding {
        tick_global_task_pools_on_main_thread(); // TODO: Poll this elsewhere too?
    }
    if app.plugins_state() == PluginsState::Ready {
        app.finish();
        app.cleanup();
    }
    assert_eq!(app.plugins_state(), PluginsState::Cleaned);

    trace!("starting UIApplicationMain");

    // Store the application in a static. `UIApplicationMain` does not give us
    // any other way of passing it onwards.
    let previous_app = APP_STATE.get(mtm).replace(Some(app));
    if previous_app.is_some() {
        panic!("tried to run `uikit_runner` twice");
    }

    UIApplication::main(
        None, // No custom UIApplication.
        Some(&NSString::from_class(ApplicationDelegate::class())),
        mtm,
    )
}

/// The [`AppExit`] message makes no sense on iOS, as the application neither
/// can nor should choose when to exit:
/// https://developer.apple.com/library/archive/qa/qa1561/_index.html
///
/// If we encounter such an message, we abort and complain loudly.
pub fn disallow_app_exit(mut exit_messages: MessageReader<AppExit>) {
    for message in exit_messages.read() {
        if cfg!(debug_assertions) {
            panic!("`AppExit::{message:?}` is not supported on iOS");
        }
    }
}

/// The application can be in the following states:
/// - Not registered / deinitialized (None).
/// - Present (Some(handler)).
/// - In use (RefCell borrowed).
type AppState = RefCell<Option<App>>;

static APP_STATE: MainThreadBound<AppState> = {
    // SAFETY: Creating marker in a `const` context,
    // where there is no concept of the main thread.
    let mtm = unsafe { MainThreadMarker::new_unchecked() };
    MainThreadBound::new(RefCell::new(None), mtm)
};

/// Get the [`App`].
///
/// # Panics
///
/// Panics if:
/// - The application is already in use (possibly a re-entrant call?).
/// - The application wasn't initialized.
#[track_caller]
pub(crate) fn access_app(mtm: MainThreadMarker) -> RefMut<'static, App> {
    RefMut::map(APP_STATE.get(mtm).borrow_mut(), |app| {
        app.as_mut().expect("application was not initialized")
    })
}

fn queue_closure(_mtm: MainThreadMarker, closure: impl FnOnce() + 'static) {
    let run_loop = CFRunLoop::main().unwrap();

    // Convert `FnOnce()` to `Block<dyn Fn()>`.
    let closure = Cell::new(Some(closure));
    let block = block2::RcBlock::new(move || {
        if let Some(closure) = closure.take() {
            closure()
        } else {
            error!("tried to execute queued closure on main thread twice");
        }
    });

    let mode = unsafe { kCFRunLoopDefaultMode.unwrap() };
    // SAFETY: The runloop is valid, the mode is a `CFStringRef`, and the block doesn't need to be
    // sendable, because we are on the main thread (which is also the run loop we have queued this
    // on).
    unsafe { CFRunLoop::perform_block(&run_loop, Some(mode), Some(&block)) }
}

/// Send a message to the application, and [update](App::update) it once afterwards to ensure the
/// message was processed.
///
/// Tries to do this synchronously if the application is not in use, but will fall back to
/// scheduling the message to be sent later if it was.
pub(crate) fn send_message(mtm: MainThreadMarker, message: impl Message) {
    if let Ok(mut app) = APP_STATE.get(mtm).try_borrow_mut() {
        let app = app.as_mut().expect("application was not initialized");
        app.world_mut().write_message(message);
        app.update();
    } else {
        trace!("re-entrant access of App, scheduling message for later");
        queue_closure(mtm, move || {
            let mut app = access_app(mtm);
            app.world_mut().write_message(message);
            app.update();
        });
    }
}

pub(crate) fn send_window_message(
    mtm: MainThreadMarker,
    message: impl Into<WindowEvent> + Message + Clone,
) {
    if let Ok(mut app) = APP_STATE.get(mtm).try_borrow_mut() {
        let app = app.as_mut().expect("application was not initialized");
        app.world_mut().send_window_message(message);
        app.update();
    } else {
        trace!("re-entrant access of App, scheduling message for later");
        queue_closure(mtm, move || {
            let mut app = access_app(mtm);
            app.world_mut().send_window_message(message);
            app.update();
        });
    }
}

#[derive(Debug)]
pub(crate) struct Ivars {}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "BevyApplicationDelegate"]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    #[derive(Debug)]
    pub(crate) struct ApplicationDelegate;

    unsafe impl NSObjectProtocol for ApplicationDelegate {}

    /// Called by `UIKitApplicationMain`.
    impl ApplicationDelegate {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Retained<Self> {
            let this = this.set_ivars(Ivars {});
            unsafe { msg_send![super(this), init] }
        }
    }

    // NOTE: We implement `application:configurationForConnectingSceneSession:options:`, which means
    // that on iOS 13.0 or later, certain methods here are not called, and instead only the scene
    // delegate methods are.
    //
    // See https://stackoverflow.com/a/9860393 for transitions here.
    unsafe impl UIApplicationDelegate for ApplicationDelegate {
        //
        // Lifecycle events
        //

        #[unsafe(method(application:willFinishLaunchingWithOptions:))]
        fn application_willFinishLaunchingWithOptions(
            &self,
            _application: &UIApplication,
            launch_options: Option<&NSDictionary<UIApplicationLaunchOptionsKey, AnyObject>>,
        ) -> bool {
            trace!(
                ?launch_options,
                "application:willFinishLaunchingWithOptions:"
            );

            // Run the App once (should end up calling the `Startup` events).
            // TODO: Avoid running the `Update` events here too (as that's
            // probably too soon)?
            let mut app = access_app(self.mtm());
            app.update();

            true
        }

        #[unsafe(method(application:didFinishLaunchingWithOptions:))]
        fn application_didFinishLaunchingWithOptions(
            &self,
            _application: &UIApplication,
            launch_options: Option<&NSDictionary<UIApplicationLaunchOptionsKey, AnyObject>>,
        ) -> bool {
            trace!(
                ?launch_options,
                "application:didFinishLaunchingWithOptions:"
            );

            let mut app = access_app(self.mtm());
            // TODO: Run app.update here?

            // Scenes are only available on iOS 13.0 and above, so if not available, act roughly
            // as-if `scene:willConnectToSession:options:` was called, and initialize the primary
            // window.
            if cfg!(feature = "no-scene")
                || !available!(ios = 13.0, tvos = 13.0, visionos = 1.0, ..)
            {
                let world = app.world_mut();
                let query = world
                    .query_filtered::<(Entity, &Window), With<PrimaryWindow>>()
                    .single(&world);
                let (entity, uikit_window) = match query {
                    Ok((entity, window)) => {
                        trace!("initializing primary window");
                        // If the user provided a primary window, initialize that.
                        let uikit_window = setup_window(None, entity, window, self.mtm());
                        (entity, uikit_window)
                    }
                    Err(QuerySingleError::NoEntities(_)) => {
                        trace!("creating primary window");
                        // If there was no primary window, let's create it ourselves.
                        let entity = world.spawn((Window::default(), PrimaryWindow));
                        let window = entity.get::<Window>().unwrap();
                        let uikit_window = setup_window(None, entity.id(), window, self.mtm());
                        (entity.id(), uikit_window)
                    }
                    Err(e) => panic!("failed fetching primary window: {e}"),
                };

                world
                    .non_send_resource_mut::<UIKitWindows>()
                    .insert(entity, uikit_window);
                world.send_window_message(WindowCreated { window: entity });
                // Intentional update, to preserve the amount of updates regardless of using scenes.
                app.update();
            }

            true
        }

        // Only called when not using scenes.
        #[unsafe(method(applicationWillEnterForeground:))]
        fn applicationWillEnterForeground(&self, _application: &UIApplication) {
            trace!("applicationWillEnterForeground:");
        }

        // Only called when not using scenes.
        #[unsafe(method(applicationDidBecomeActive:))]
        fn applicationDidBecomeActive(&self, _application: &UIApplication) {
            trace!("applicationDidBecomeActive:");
        }

        // Only called when not using scenes.
        #[unsafe(method(applicationWillResignActive:))]
        fn applicationWillResignActive(&self, _application: &UIApplication) {
            trace!("applicationWillResignActive:");
        }

        // Only called when not using scenes.
        #[unsafe(method(applicationDidEnterBackground:))]
        fn applicationDidEnterBackground(&self, _application: &UIApplication) {
            trace!("applicationDidEnterBackground:");
        }

        #[unsafe(method(applicationWillTerminate:))]
        fn applicationWillTerminate(&self, _application: &UIApplication) {
            trace!("applicationWillTerminate:");

            let app = APP_STATE
                .get(self.mtm())
                .borrow_mut()
                .take()
                .expect("application was not initialized");
            // `Drop` the `App` to cleanly shut down Bevy's state.
            // TODO: Emit a message too?
            let _: App = app;
        }

        //
        // Various events
        //

        #[unsafe(method(applicationDidReceiveMemoryWarning:))]
        fn applicationDidReceiveMemoryWarning(&self, _application: &UIApplication) {
            trace!("applicationDidReceiveMemoryWarning:");
        }

        // TODO: Called when using scenes or not?
        #[unsafe(method(application:openURL:options:))]
        #[allow(deprecated)]
        fn application_openURL_options(
            &self,
            _application: &UIApplication,
            url: &NSURL,
            options: &NSDictionary<UIApplicationOpenURLOptionsKey, AnyObject>,
        ) -> bool {
            trace!(?url, ?options, "application:openURL:options:");
            // TODO: Handle URL opening
            false
        }

        // Scenes

        #[cfg(not(feature = "no-scene"))]
        #[unsafe(method_id(application:configurationForConnectingSceneSession:options:))]
        fn application_configurationForConnectingSceneSession_options(
            &self,
            _application: &UIApplication,
            connecting_scene_session: &UISceneSession,
            options: &UISceneConnectionOptions,
        ) -> Retained<UISceneConfiguration> {
            trace!(
                scene = ?connecting_scene_session.persistentIdentifier(),
                user_info = ?connecting_scene_session.userInfo(),
                configuration = ?connecting_scene_session.configuration(),
                ?options,
                "application:configurationForConnectingSceneSession:options:"
            );

            // TODO: State restoration based on the scene session.
            // TODO: User activities.

            // TODO: Support multiple scene kinds somehow?
            let config = UISceneConfiguration::configurationWithName_sessionRole(
                Some(ns_string!("Bevy Configuration")),
                &connecting_scene_session.role(),
                self.mtm(),
            );

            unsafe { config.setDelegateClass(Some(SceneDelegate::class())) };

            config
        }

        #[cfg(not(feature = "no-scene"))]
        #[unsafe(method(application:didDiscardSceneSessions:))]
        fn application_didDiscardSceneSessions(
            &self,
            _application: &UIApplication,
            scene_sessions: &NSSet<UISceneSession>,
        ) {
            trace!(?scene_sessions, "application:didDiscardSceneSessions:");
            // TODO: State restoration based on UISceneSession.
        }

        // Storyboarding

        #[unsafe(method_id(window))]
        fn window(&self) -> Option<Retained<UIWindow>> {
            None
        }

        #[unsafe(method(setWindow:))]
        fn setWindow(&self, _window: Option<&UIWindow>) {
            warn!("setting a story board is not supported in Bevy, remove `UIMainStoryboardFile` key from `Info.plist`");
        }

        // TODO: State restoration.
        // TODO: User activities.
        // TODO: Expose other UIApplicationDelegate events to the user?
    }
);

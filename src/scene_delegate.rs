#![expect(non_snake_case, reason = "UIKit does not use Rust naming conventions")]
use std::cell::Cell;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QuerySingleError, With};
use bevy_window::{PrimaryWindow, Window, WindowCreated};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, DefinedClass as _, MainThreadOnly, Message as _};
use objc2_foundation::{ns_string, NSNumber, NSObjectProtocol, NSSet};
use objc2_ui_kit::{
    UICoordinateSpace, UIInterfaceOrientation, UIOpenURLContext, UIResponder, UIScene,
    UISceneConnectionOptions, UISceneDelegate, UISceneSession, UITraitCollection, UIWindow,
    UIWindowScene, UIWindowSceneDelegate,
};
use tracing::trace;

use crate::app::access_app;
use crate::windows::setup_window;
use crate::{UIKitWindows, USER_INFO_WINDOW_ENTITY_ID, WINDOW_ACTIVITY_TYPE};

pub(crate) struct Ivars {
    window: Cell<Option<Retained<UIWindow>>>,
}

define_class!(
    #[unsafe(super(UIResponder))]
    #[name = "BevySceneDelegate"]
    #[ivars = Ivars]
    pub(crate) struct SceneDelegate;

    /// Called internally by `UIApplication` based on `UISceneDelegateClassName` in `Info.plist`.
    impl SceneDelegate {
        #[unsafe(method_id(init))]
        fn init(this: Allocated<Self>) -> Retained<Self> {
            let this = this.set_ivars(Ivars {
                window: Cell::new(None),
            });
            unsafe { msg_send![super(this), init] }
        }
    }

    unsafe impl NSObjectProtocol for SceneDelegate {}

    unsafe impl UISceneDelegate for SceneDelegate {
        #[unsafe(method(scene:willConnectToSession:options:))]
        fn scene_willConnectToSession_options(
            &self,
            scene: &UIScene,
            session: &UISceneSession,
            connection_options: &UISceneConnectionOptions,
        ) {
            trace!(
                scene = ?unsafe { scene.session().persistentIdentifier() },
                ?session,
                ?connection_options,
                "scene:willConnectToSession:options:"
            );

            let scene = scene.downcast_ref::<UIWindowScene>().unwrap();

            let mut app = access_app(self.mtm());
            let world = app.world_mut();

            // Try to get `Entity` that was passed by `create_windows`.
            let entity = unsafe {
                connection_options
                    .userActivities()
                    .iter()
                    .find(|activity| &*activity.activityType() == ns_string!(WINDOW_ACTIVITY_TYPE))
                    .and_then(|activity| activity.userInfo())
                    .and_then(|user_info| {
                        user_info.objectForKey(ns_string!(USER_INFO_WINDOW_ENTITY_ID))
                    })
                    .and_then(|obj| obj.downcast::<NSNumber>().ok())
                    .map(|number| Entity::from_bits(number.as_u64()))
            };

            let (entity, uikit_window) = if let Some(entity) = entity {
                trace!("creating requested window");
                let window = world
                    .get::<Window>(entity)
                    .expect("failed fetching Window component on newly created window");
                let uikit_window = setup_window(Some(scene), entity, window, self.mtm());
                (entity, uikit_window)
            } else {
                // The entity can be missing in two scenarios:
                // - This is the initial launch.
                // - The user decided to launch a new window using system buttons.
                let query = world
                    .query_filtered::<Entity, With<PrimaryWindow>>()
                    .get_single(&world);
                match query {
                    Ok(entity) => {
                        // If we have a primary window, check if we have already initialized it.
                        let uikit_windows = world.non_send_resource_mut::<UIKitWindows>();
                        if !uikit_windows.is_initialized(entity) {
                            trace!("initializing primary window");
                            // If we have not, assume this is the initial launch, and configure the entity.
                            let window = world.get::<Window>(entity).unwrap();
                            let uikit_window =
                                setup_window(Some(scene), entity, window, self.mtm());
                            (entity, uikit_window)
                        } else {
                            trace!("creating user-requested window");
                            // Otherwise, assume that this is a user-launched window.
                            let entity = world.spawn(Window::default());
                            let window = entity.get::<Window>().unwrap();
                            let uikit_window =
                                setup_window(Some(scene), entity.id(), window, self.mtm());
                            (entity.id(), uikit_window)
                        }
                    }
                    Err(QuerySingleError::NoEntities(_)) => {
                        trace!("creating primary window");
                        // If there was no primary window, let's create it ourselves.
                        let entity = world.spawn((Window::default(), PrimaryWindow));
                        let window = entity.get::<Window>().unwrap();
                        let uikit_window =
                            setup_window(Some(scene), entity.id(), window, self.mtm());
                        (entity.id(), uikit_window)
                    }
                    Err(e) => panic!("failed fetching primary window: {e}"),
                }
            };

            world
                .non_send_resource_mut::<UIKitWindows>()
                .insert(entity, uikit_window);
            world.send_event(WindowCreated { window: entity });
            app.update();
        }

        #[unsafe(method(sceneWillEnterForeground:))]
        fn sceneWillEnterForeground(&self, scene: &UIScene) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, "sceneWillEnterForeground:");
        }

        #[unsafe(method(sceneDidBecomeActive:))]
        fn sceneDidBecomeActive(&self, scene: &UIScene) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, "sceneDidBecomeActive:");
        }

        #[unsafe(method(sceneWillResignActive:))]
        fn sceneWillResignActive(&self, scene: &UIScene) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, "sceneWillResignActive:");
        }

        #[unsafe(method(sceneDidEnterBackground:))]
        fn sceneDidEnterBackground(&self, scene: &UIScene) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, "sceneDidEnterBackground:");
        }

        #[unsafe(method(sceneDidDisconnect:))]
        fn sceneDidDisconnect(&self, scene: &UIScene) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, "sceneDidDisconnect:");
        }

        #[unsafe(method(scene:openURLContexts:))]
        fn scene_openURLContexts(&self, scene: &UIScene, url_contexts: &NSSet<UIOpenURLContext>) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, ?url_contexts, "scene:openURLContexts:");
            // TODO: Handle URL opening
        }
    }

    unsafe impl UIWindowSceneDelegate for SceneDelegate {
        #[unsafe(method_id(window))]
        fn window(&self) -> Option<Retained<UIWindow>> {
            let window = self.ivars().window.take();
            self.ivars().window.set(window.clone());
            window
        }

        #[unsafe(method(setWindow:))]
        fn setWindow(&self, window: Option<&UIWindow>) {
            trace!("UIWindowSceneDelegate.window configured");
            self.ivars().window.set(window.map(|w| w.retain()));
        }

        #[unsafe(method(windowScene:didUpdateCoordinateSpace:interfaceOrientation:traitCollection:))]
        fn windowScene_didUpdateCoordinateSpace_interfaceOrientation_traitCollection(
            &self,
            scene: &UIWindowScene,
            _previous_coordinate_space: &ProtocolObject<dyn UICoordinateSpace>,
            _previous_interface_orientation: UIInterfaceOrientation,
            _previous_trait_collection: &UITraitCollection,
        ) {
            trace!(
                scene = ?unsafe { scene.session().persistentIdentifier() },
                "windowScene:didUpdateCoordinateSpace:interfaceOrientation:traitCollection:",
            );
        }
    }
);

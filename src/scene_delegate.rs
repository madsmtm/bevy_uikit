#![expect(non_snake_case, reason = "UIKit does not use Rust naming conventions")]
use std::cell::Cell;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QuerySingleError, With};
use bevy_window::{
    PrimaryWindow, Window, WindowActivate, WindowBackground, WindowCreated, WindowDeactivate,
    WindowDestroyed, WindowForeground,
};
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
use crate::windows::{setup_window, WorldHelper};
use crate::{UIKitWindows, USER_INFO_WINDOW_ENTITY_ID, WINDOW_ACTIVITY_TYPE};

pub(crate) struct Ivars {
    entity: Cell<Option<Entity>>,
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
                entity: Cell::new(None),
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
                user_info = ?unsafe { session.userInfo() },
                configuration = ?unsafe { session.configuration() },
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
                            trace!("creating system-requested window");
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

            self.ivars().entity.set(Some(entity));
            let uiwindow = uikit_window.uiwindow.retain().into_super();
            self.ivars().window.set(Some(uiwindow));

            world
                .non_send_resource_mut::<UIKitWindows>()
                .insert(entity, uikit_window);
            world.send_window_event(WindowCreated { window: entity });
            app.update();
        }

        #[unsafe(method(sceneWillEnterForeground:))]
        fn sceneWillEnterForeground(&self, scene: &UIScene) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, "sceneWillEnterForeground:");

            let mut app = access_app(self.mtm());
            if let Some(window) = self.ivars().entity.get() {
                app.world_mut()
                    .send_window_event(WindowForeground { window });
            }
            app.update();
        }

        #[unsafe(method(sceneDidBecomeActive:))]
        fn sceneDidBecomeActive(&self, scene: &UIScene) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, "sceneDidBecomeActive:");

            let mut app = access_app(self.mtm());
            if let Some(window) = self.ivars().entity.get() {
                app.world_mut().send_window_event(WindowActivate { window });
            }
            if let Some(uiwindow) = self.window() {
                uiwindow.makeKeyAndVisible();
            }
            app.update();
        }

        #[unsafe(method(sceneWillResignActive:))]
        fn sceneWillResignActive(&self, scene: &UIScene) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, "sceneWillResignActive:");

            let mut app = access_app(self.mtm());
            if let Some(window) = self.ivars().entity.get() {
                app.world_mut()
                    .send_window_event(WindowDeactivate { window });
            }
            app.update();
        }

        #[unsafe(method(sceneDidEnterBackground:))]
        fn sceneDidEnterBackground(&self, scene: &UIScene) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, "sceneDidEnterBackground:");

            let mut app = access_app(self.mtm());
            if let Some(window) = self.ivars().entity.get() {
                app.world_mut()
                    .send_window_event(WindowBackground { window });
            }
            app.update();
        }

        #[unsafe(method(sceneDidDisconnect:))]
        fn sceneDidDisconnect(&self, scene: &UIScene) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, "sceneDidDisconnect:");

            let mut app = access_app(self.mtm());
            // User/system may have requested scene destruction; if so, we remove it from the world.
            if let Some(entity) = self.ivars().entity.get() {
                // despawn_windows will take care of unregistering from UIKitWindows.
                // Ignore if it doesn't exist, that's likely because someone else despawned it.
                let _ = app.world_mut().try_despawn(entity);
                app.world_mut()
                    .send_window_event(WindowDestroyed { window: entity });
                self.ivars().entity.set(None);
            }
            app.update();
        }

        #[unsafe(method(scene:openURLContexts:))]
        fn scene_openURLContexts(&self, scene: &UIScene, url_contexts: &NSSet<UIOpenURLContext>) {
            trace!(scene = ?unsafe { scene.session().persistentIdentifier() }, ?url_contexts, "scene:openURLContexts:");
            // TODO: Handle URL opening
        }
    }

    unsafe impl UIWindowSceneDelegate for SceneDelegate {
        #[unsafe(method_id(window))]
        fn __window(&self) -> Option<Retained<UIWindow>> {
            self.window()
        }

        #[unsafe(method(setWindow:))]
        fn setWindow(&self, window: Option<&UIWindow>) {
            self.ivars().window.set(window.map(|w| w.retain()));
        }

        #[unsafe(method(windowScene:didUpdateCoordinateSpace:interfaceOrientation:traitCollection:))]
        fn windowScene_didUpdateCoordinateSpace_interfaceOrientation_traitCollection(
            &self,
            _scene: &UIWindowScene,
            _previous_coordinate_space: &ProtocolObject<dyn UICoordinateSpace>,
            _previous_interface_orientation: UIInterfaceOrientation,
            _previous_trait_collection: &UITraitCollection,
        ) {
            // Happens quite often apparently?
            // trace!(
            //     scene = ?unsafe { _scene.session().persistentIdentifier() },
            //     ?_previous_coordinate_space,
            //     ?_previous_interface_orientation,
            //     ?_previous_trait_collection,
            //     "windowScene:didUpdateCoordinateSpace:interfaceOrientation:traitCollection:",
            // );
        }
    }
);

impl SceneDelegate {
    fn window(&self) -> Option<Retained<UIWindow>> {
        let window = self.ivars().window.take();
        self.ivars().window.set(window.clone());
        window
    }
}

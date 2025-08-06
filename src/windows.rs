use std::mem;
use std::ptr::NonNull;

use bevy_ecs::{
    entity::{hash_map::EntityHashMap, Entity},
    event::BufferedEvent,
    lifecycle::RemovedComponents,
    query::{Added, Changed, Without},
    system::{NonSend, NonSendMut, Query},
    world::World,
};
use bevy_window::{PrimaryWindow, Window, WindowEvent, WindowTheme};
use block2::RcBlock;
use objc2::{available, rc::Retained, MainThreadMarker, MainThreadOnly};
use objc2::{define_class, msg_send, AllocAnyThread, Message};
use objc2_core_foundation::{CGFloat, CGSize};
use objc2_foundation::{ns_string, NSDictionary, NSError, NSNumber, NSString, NSUserActivity};
use objc2_ui_kit::{
    UIApplication, UISceneActivationRequestOptions, UISceneDestructionRequestOptions,
    UIUserInterfaceStyle, UIWindow, UIWindowScene,
};
use tracing::{error, trace};

use crate::{view::ViewController, MainThread, USER_INFO_WINDOW_ENTITY_ID, WINDOW_ACTIVITY_TYPE};

pub(crate) trait WorldHelper {
    fn send_window_event(&mut self, event: impl Into<WindowEvent> + BufferedEvent + Clone);
}

impl WorldHelper for World {
    fn send_window_event(&mut self, event: impl Into<WindowEvent> + BufferedEvent + Clone) {
        self.send_event(event.clone());
        self.send_event(event.into());
    }
}

/// The state specific to UIKit for each window.
#[derive(Debug)]
pub struct UIKitWindow {
    // Is unset if not using scenes
    scene: Option<Retained<UIWindowScene>>,
    pub(crate) uiwindow: Retained<BevyWindow>,
}

/// A resource mapping Window entities to `UIKitWindow`.
///
/// This is necessary because we cannot just add `UIKitWindow` as a component
/// of the window, because it is non-send.
#[derive(Debug, Default)]
pub struct UIKitWindows {
    entity_to_uikit: EntityHashMap<UIKitWindow>,
}

impl UIKitWindows {
    pub(crate) fn get(&self, entity: Entity) -> Option<&UIKitWindow> {
        self.entity_to_uikit.get(&entity)
    }

    pub(crate) fn is_initialized(&self, entity: Entity) -> bool {
        self.get(entity).is_some()
    }

    pub(crate) fn insert(&mut self, entity: Entity, uikit_window: UIKitWindow) {
        let prev = self.entity_to_uikit.insert(entity, uikit_window);
        debug_assert!(prev.is_none(), "tried to create existing window");
    }
}

/// Create and set up a new `UIWindow` with state taken from the passed in `Window` and scene.
pub(crate) fn setup_window(
    scene: Option<&UIWindowScene>,
    entity: Entity,
    window: &Window,
    mtm: MainThreadMarker,
) -> UIKitWindow {
    let view_controller = ViewController::new(mtm, entity);

    let uiwindow = BevyWindow::alloc(mtm).set_ivars(entity);
    let uiwindow: Retained<BevyWindow> = if let Some(scene) = scene {
        unsafe { msg_send![super(uiwindow), initWithWindowScene: scene] }
    } else {
        unsafe { msg_send![super(uiwindow), init] }
    };
    uiwindow.setRootViewController(Some(&view_controller));

    update_window(window, &uiwindow, scene);

    // Show the window
    uiwindow.makeKeyAndVisible();

    UIKitWindow {
        scene: scene.map(|scene| scene.retain()),
        uiwindow,
    }
}

/// Request new windows to be created for each entity with a newly-added [`Window`] component.
pub fn create_windows(
    mut created_windows: Query<Entity, (Added<Window>, Without<PrimaryWindow>)>,
    uikit_windows: NonSend<UIKitWindows>,
    mtm: NonSend<MainThread>,
) {
    for entity in &mut created_windows {
        if uikit_windows.is_initialized(entity) {
            // Don't request creation on user-created windows.
            continue;
        };

        // Check for window scene support.
        if available!(ios = 13.0, tvos = 13.0, visionos = 1.0, ..) {
            trace!("requesting window creation");
            let application = UIApplication::sharedApplication(mtm.0);
            let options = unsafe { UISceneActivationRequestOptions::new(mtm.0) };
            let user_activity = unsafe {
                NSUserActivity::initWithActivityType(
                    NSUserActivity::alloc(),
                    ns_string!(WINDOW_ACTIVITY_TYPE),
                )
            };
            let dict = NSDictionary::from_slices(
                &[ns_string!(USER_INFO_WINDOW_ENTITY_ID)],
                &[NSNumber::new_u64(entity.to_bits()).as_ref()],
            );
            let dict = unsafe { mem::transmute::<&NSDictionary<NSString>, &NSDictionary>(&*dict) };
            unsafe { user_activity.addUserInfoEntriesFromDictionary(&dict) };
            // TODO: Set `options.collectionJoinBehavior` on Mac Catalyst?
            let error_handler = RcBlock::new(|err: NonNull<NSError>| {
                let err = unsafe { err.as_ref() };
                error!(%err, "failed creating window, this is not possible on single-window iOS");
            });
            #[allow(deprecated, reason = "the replacement API requires newer OS versions")]
            unsafe {
                application.requestSceneSessionActivation_userActivity_options_errorHandler(
                    None, // Create a new scene
                    Some(&user_activity),
                    Some(&options),
                    Some(&error_handler),
                )
            };
        } else {
            error!("failed creating window, this is not possible on this version of iOS");
        }
    }
}

/// Propagate changes by the user in [`Window`] entities to UIKit.
pub fn changed_windows(
    changed_windows: Query<(Entity, &Window), Changed<Window>>,
    uikit_windows: NonSend<UIKitWindows>,
) {
    for (entity, window) in &changed_windows {
        trace!(?entity, "detected changes to Window");
        let Some(uikit_window) = uikit_windows.get(entity) else {
            // Not (yet) registered with UIKit, should be when the scene connects.
            continue;
        };

        update_window(
            window,
            &uikit_window.uiwindow,
            uikit_window.scene.as_deref(),
        );
    }
}

fn update_window(
    Window {
        canvas: _,                                           // Web-specific
        clip_children: _,                                    // Windows-specific
        composite_alpha_mode: _,                             // Handled by `bevy_render`
        decorations: _,                                      // TODO (usable on Mac Catalyst)
        desired_maximum_frame_latency: _,                    // Handled by `bevy_render`
        enabled_buttons,                                     // Handled
        fit_canvas_to_parent: _,                             // Web-specific
        focused: _,                        // TODO: State controlled by us (`keyWindow`)?
        fullsize_content_view: _,          // macOS-specific
        has_shadow: _,                     // macOS-specific
        ime_enabled: _,                    // TODO
        ime_position: _,                   // TODO
        internal: _,                       // TODO: Perhaps needs more exposed internals?
        mode: _,                           // TODO
        movable_by_window_background: _,   // macOS-specific
        name: _,                           // Not relevant on iOS
        position,                          // Handled
        prefers_home_indicator_hidden: _,  // TODO
        prefers_status_bar_hidden: _,      // TODO
        present_mode: _,                   // Handled by `bevy_render`
        prevent_default_event_handling: _, // Web-specific
        recognize_doubletap_gesture: _,    // TODO
        recognize_pan_gesture: _,          // TODO
        recognize_pinch_gesture: _,        // TODO
        recognize_rotation_gesture: _,     // TODO
        resizable: _,                      // TODO
        resize_constraints,                // Handled
        resolution,                        // Handled
        skip_taskbar: _,                   // Windows-specific
        title,                             // Handled
        titlebar_show_buttons: _,          // macOS-specific
        titlebar_show_title: _,            // macOS-specific
        titlebar_shown: _,                 // macOS-specific
        titlebar_transparent: _,           // `configureWithTransparentBackground`?
        transparent: _,                    // Unsupported
        visible: _,                        // Unsupported
        window_level: _,                   // Unsupported
        window_theme,                      // Handled
        preferred_screen_edges_deferring_system_gestures: _, // TODO
    }: &Window,
    window: &UIWindow,
    scene: Option<&UIWindowScene>,
) {
    // Avoid infinity, which NSLayoutConstraint doesn't like.
    fn avoid_inf(num: f32) -> CGFloat {
        num.min(f32::MAX) as CGFloat
    }

    unsafe {
        if let Some(scene) = scene {
            let title = NSString::from_str(&title);
            if scene.title() != title {
                trace!(?title, "setting UIWindowScene.title");
                scene.setTitle(Some(&title));
            }

            if let Some(size_restrictions) = scene.sizeRestrictions() {
                let min = CGSize {
                    width: avoid_inf(resize_constraints.min_width),
                    height: avoid_inf(resize_constraints.min_height),
                };
                if min != size_restrictions.minimumSize() {
                    trace!(?min, "setting UIWindowScene.sizeRestrictions.minimumSize");
                    size_restrictions.setMinimumSize(min);
                }

                let max = CGSize {
                    width: avoid_inf(resize_constraints.max_width),
                    height: avoid_inf(resize_constraints.max_height),
                };
                if max != size_restrictions.maximumSize() {
                    trace!(?max, "setting UIWindowScene.sizeRestrictions.maximumSize");
                    size_restrictions.setMaximumSize(max);
                }

                if cfg!(target_abi = "macabi") && available!(ios = 16.0, ..) {
                    let val = enabled_buttons.maximize;
                    if size_restrictions.allowsFullScreen() != val {
                        trace!(
                            ?val,
                            "setting UIWindowScene.sizeRestrictions.allowsFullScreen"
                        );
                        size_restrictions.setAllowsFullScreen(val);
                    }
                }
            }

            if available!(ios = 16.0, tvos = 16.0, visionos = 1.0, ..) {
                if let Some(behaviours) = scene.windowingBehaviors() {
                    let val = enabled_buttons.minimize;
                    if behaviours.isMiniaturizable() != val {
                        trace!(
                            ?val,
                            "setting UIWindowScene.windowingBehaviors.miniaturizable"
                        );
                        behaviours.setMiniaturizable(val);
                    }

                    let val = enabled_buttons.close;
                    if behaviours.isClosable() != val {
                        trace!(?val, "setting UIWindowScene.windowingBehaviors.closable");
                        behaviours.setClosable(val);
                    }
                }
            }

            // UIWindowSceneGeometry only exists on Mac Catalyst 16.0.
            // On iOS/tvOS/visionOS, it is not possible to modify the frame of the scene (?)
            if cfg!(target_abi = "macabi") && available!(ios = 16.0) {
                // TODO
                // let geometry = scene.effectiveGeometry();
                //
                // match position {
                //     WindowPosition::Automatic => todo!(),
                //     WindowPosition::Centered(_monitor) => todo!(),
                //     WindowPosition::At(pos) => todo!(),
                // }
                //
                // let preference = UIWindowSceneGeometryPreferencesMac::new();
                // preference.setSystemFrame(frame);
            }
        }

        // NOTE: UIUserInterfaceStyle is available on iOS 12, it's just the override there isn't,
        // so there might be a way to select this even there? But we won't bother.
        if available!(ios = 13.0, tvos = 13.0, visionos = 1.0, ..) {
            let style = match window_theme {
                Some(WindowTheme::Light) => UIUserInterfaceStyle::Light,
                Some(WindowTheme::Dark) => UIUserInterfaceStyle::Dark,
                None => UIUserInterfaceStyle::Unspecified,
            };
            if window.overrideUserInterfaceStyle() != style {
                trace!(?style, "setting UIWindow.overrideUserInterfaceStyle");
                window.setOverrideUserInterfaceStyle(style);
            }
        }
    }
}

/// Remove windows from the scene.
pub fn despawn_windows(
    mut removed_windows: RemovedComponents<Window>,
    mut uikit_windows: NonSendMut<UIKitWindows>,
) {
    for entity in removed_windows.read() {
        trace!(?entity, "detected removed Window");
        let Some(uikit_window) = uikit_windows.entity_to_uikit.remove(&entity) else {
            // Don't request removal on user-removed windows.
            continue;
        };

        // Request removal from UIKit too.
        if let Some(scene) = uikit_window.scene {
            let app = UIApplication::sharedApplication(scene.mtm());
            let options = unsafe { UISceneDestructionRequestOptions::new(scene.mtm()) };
            let error_handler = RcBlock::new(|err: NonNull<NSError>| {
                let err = unsafe { err.as_ref() };
                error!(%err, "failed removing window, this is not possible on single-window iOS");
            });
            unsafe {
                app.requestSceneSessionDestruction_options_errorHandler(
                    &scene.session(),
                    Some(&options),
                    Some(&error_handler),
                );
            }
        } else {
            error!("tried to remove main window, this is not possible on single-window iOS");
        }
    }
}

define_class!(
    #[unsafe(super(UIWindow))]
    #[name = "BevyWindow"]
    #[derive(Debug, PartialEq, Eq, Hash)]
    #[ivars = Entity]
    pub(crate) struct BevyWindow;
);

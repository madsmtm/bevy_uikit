#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![doc(
    html_logo_url = "https://bevyengine.org/assets/icon.png",
    html_favicon_url = "https://bevyengine.org/assets/icon.png"
)]

use bevy_app::{App, Last, Plugin};
use objc2::{available, ClassType, MainThreadMarker};

use crate::app::ApplicationDelegate;
pub use crate::app::{disallow_app_exit, uikit_runner};
use crate::scene_delegate::SceneDelegate;
pub use crate::settings::UIKitSettings;
use crate::view::{View, ViewController};
use crate::windows::BevyWindow;
pub use windows::{changed_windows, create_windows, despawn_windows, UIKitWindow, UIKitWindows};

mod app;
mod scene_delegate;
mod settings;
mod view;
mod windows;

// Used to pass the newly created window entity ID to `scene:willConnectToSession:options:`.
pub(crate) const WINDOW_ACTIVITY_TYPE: &str = "org.bevyengine.internal.new-window";
pub(crate) const USER_INFO_WINDOW_ENTITY_ID: &str = "BevyWindowEntityId";

#[derive(Default)]
pub struct UIKitPlugin;

/// A marker used to statically know that a system runs on the main thread.
#[derive(Debug)]
pub struct MainThread(MainThreadMarker);

impl Plugin for UIKitPlugin {
    fn name(&self) -> &str {
        "bevy_uikit::UIKitPlugin"
    }

    fn build(&self, app: &mut App) {
        let mtm = MainThreadMarker::new()
            .expect("must build the App on the main thread when using UIKit");

        // Initialize classes with Objective-C runtime.
        let _ = ApplicationDelegate::class();
        let _ = BevyWindow::class();
        let _ = ViewController::class();
        let _ = View::class();
        if !cfg!(feature = "no-scene") && available!(ios = 13.0, tvos = 13.0, visionos = 1.0, ..) {
            let _ = SceneDelegate::class();
        }

        app.init_non_send_resource::<UIKitWindows>()
            .insert_non_send_resource(MainThread(mtm))
            .init_resource::<UIKitSettings>()
            .set_runner(uikit_runner)
            .add_systems(Last, disallow_app_exit)
            .add_systems(Last, (create_windows, changed_windows, despawn_windows));
    }
}

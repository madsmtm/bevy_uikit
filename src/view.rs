#![expect(non_snake_case, reason = "UIKit does not use Rust naming conventions")]
use bevy_ecs::entity::Entity;
use bevy_window::WindowFocused;
use objc2::{define_class, msg_send, rc::Retained, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::NSObjectProtocol;
use objc2_ui_kit::{
    UIFocusAnimationCoordinator, UIFocusEnvironment, UIFocusUpdateContext, UIResponder, UIView,
    UIViewController,
};
use tracing::trace;

use crate::app::send_window_message;

define_class!(
    #[unsafe(super(UIViewController))]
    #[name = "BevyViewController"]
    #[derive(Debug, PartialEq, Eq, Hash)]
    #[ivars = Entity]
    pub(crate) struct ViewController;

    unsafe impl NSObjectProtocol for ViewController {}

    /// Overridden UIViewController methods.
    impl ViewController {
        #[unsafe(method(loadView))]
        fn loadView(&self) {
            let view = View::new(self.mtm(), *self.ivars(), self.preferredContentSize());
            self.setView(Some(&view));

            // Docs say to _not_ call super
        }
    }

    unsafe impl UIFocusEnvironment for ViewController {
        #[unsafe(method(didUpdateFocusInContext:withAnimationCoordinator:))]
        fn didUpdateFocusInContext_withAnimationCoordinator(
            &self,
            context: &UIFocusUpdateContext,
            coordinator: &UIFocusAnimationCoordinator,
        ) {
            trace!(
                ?context,
                ?coordinator,
                "didUpdateFocusInContext:withAnimationCoordinator:"
            );
            unsafe {
                msg_send![super(self), didUpdateFocusInContext: context, withAnimationCoordinator: coordinator]
            }
        }
    }
);

impl ViewController {
    pub(crate) fn new(mtm: MainThreadMarker, window: Entity) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(window);
        unsafe { msg_send![super(this), init] }
    }
}

define_class!(
    #[unsafe(super(UIView, UIResponder))] // TODO: MTKView?
    #[name = "BevyView"]
    #[derive(Debug, PartialEq, Eq, Hash)]
    #[ivars = Entity]
    pub(crate) struct View;

    /// Overridden UIResponder methods.
    impl View {
        #[unsafe(method(canBecomeFirstResponder))]
        fn canBecomeFirstResponder(&self) -> bool {
            true
        }

        #[unsafe(method(becomeFirstResponder))]
        fn becomeFirstResponder(&self) -> bool {
            let success = unsafe { msg_send![super(self), becomeFirstResponder] };
            if success {
                send_window_message(
                    self.mtm(),
                    WindowFocused {
                        window: *self.ivars(),
                        focused: true,
                    },
                );
            }
            success
        }

        #[unsafe(method(resignFirstResponder))]
        fn resignFirstResponder(&self) -> bool {
            let success = unsafe { msg_send![super(self), resignFirstResponder] };
            if success {
                send_window_message(
                    self.mtm(),
                    WindowFocused {
                        window: *self.ivars(),
                        focused: false,
                    },
                );
            }
            success
        }
    }
);

impl View {
    fn new(mtm: MainThreadMarker, window: Entity, size: CGSize) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(window);
        let frame = CGRect {
            origin: CGPoint::ZERO,
            size,
        };
        unsafe { msg_send![super(this), initWithFrame: frame] }
    }
}

# Direct UIKit backend for Bevy Engine

[![Latest version](https://badgen.net/crates/v/bevy_uikit)](https://crates.io/crates/bevy_uikit)
[![License](https://badgen.net/badge/license/MIT%20OR%20Apache-2.0/blue)](https://github.com/madsmtm/bevy_uikit/blob/main/README.md)
[![Documentation](https://docs.rs/bevy_uikit/badge.svg)](https://docs.rs/bevy_uikit/)
[![CI](https://github.com/madsmtm/bevy_uikit/actions/workflows/ci.yml/badge.svg)](https://github.com/madsmtm/bevy_uikit/actions/workflows/ci.yml)
[![Following Bevy's main branch](https://img.shields.io/badge/Bevy%20tracking-main-lightblue)](https://bevyengine.org/learn/quick-start/plugin-development/#main-branch-tracking)

We alreay have one UI backend in Bevy, namely [`bevy_winit`](https://docs.rs/bevy_winit/), which is based on [`winit`](https://docs.rs/winit/). So why another?

Put simply, the Winit iOS/UIKit backend (which I maintain :roll_eyes:) is quite terrible. So I created this to experiment with what's actually necessary to take game development to the next level on iOS (and visionOS).


## Development

Checkout https://github.com/madsmtm/bevy/tree/uikit in a folder relative to this project (or comment out the `[patch."https://github.com/madsmtm/bevy"]` in `.cargo/config.toml`).

Run the examples on Mac Catalyst with:
```
cargo bundle --target=aarch64-apple-ios-macabi --example simple && ./target/aarch64-apple-ios-macabi/debug/examples/bundle/ios/bevy_uikit.app/simple
```


## Notes

This intends to:
- Have proper multi-window (i.e. UIScene) support.
  - UIWindowScene -> (multiple UIWindow internally) -> multiple UIView?
    - UIWindow is really a user-interface element, doesn't have anything to do with windowing.
    - UIWindowScene is the actual window.
    - UISceneSession is the configuration, a bit similar in function to NSWindowController
    - Implication: Lifecycle events are window-based...
  - Overview: https://developer.apple.com/videos/play/wwdc2019/212/
    - Most applications should support multiple windows.
    - Oftentimes all scenes are the same (Safari, Pages, Notes, Calendar, other document-based apps), and that's a fine first implementation.
    - But sometimes they're different (Mail, Messages, other detail scenes)
      - Detail views that support drag/drop should support multi-window
      - Can be opened and closed in certain ways depending on configuration.
    - Scene configurations can be done dynamically in code (or statically in `Info.plist`)
    - The system can request a new window in certain cases (if multi-window is enabled).
      - Letting the application handle the list of windows in Winit is untenable!
    - Design-wise, opening a new window should be an explicit user-choice.
    - Scenes/windows can be opened or activated.
- Handle lifecycle events correctly.
  - Both the legacy application lifecycle, and scene lifecycle on newer iOS.
  - Described in more detail in https://developer.apple.com/videos/play/wwdc2019/258/
- Handle AppDelegate request to re-read preferences.
  - or locale, etc.: https://developer.apple.com/documentation/uikit/processing-queued-notifications
- Make providing a launch screen easy?
- Detect launch requests / determine cause of launch?
  - Do non-UI work in willFinishLaunching, and UI work in didFinishLaunching?
  - Good separation description in:
    - https://developer.apple.com/documentation/uikit/preparing-your-ui-to-run-in-the-foreground
    - https://developer.apple.com/documentation/uikit/preparing-your-ui-to-run-in-the-background
      - Can we get Bevy to automatically unload resources?
  - Detect file-open requests?
- State restoration?
  - Based on scenes, important now because scenes may be discarded and only the session remains
    - The scene and view state etc. may be unloaded, the session contains the relevant state to restore
  - https://developer.apple.com/documentation/uikit/about-the-ui-restoration-process
  - Somewhat important that we have a semi-global view of the user's scenes/windows (or at least the _structure_ of it)
    - Maybe we can do this nicely in Bevy?
- Support handling of certain background events?
  - And maybe registration of background tasks?
  - beginBackgroundTask might be useful for ensuring that state is written to disk?
    - Maybe performExpiringActivity instead? Similar to sudden termination in NSProcessInfo
    - Maybe [Service](https://developer.android.com/reference/kotlin/android/app/Service) on Android?
  - Update snapshots when something changes.
    - When are snapshots taken? I think we need to render here?
- Syncing events between scenes
- Shortcuts and menu items

Unknowns:
- What's the actual difference between `UIScene` and `UIWindow`?
- How should multi-window/multi-scene work?
  - Probably opt-in for the Bevy user.
  - How do we handle the system creating a window on the user's behalf (without the application explicitly requesting it)?
- SwiftUI handles the status bar item as a scene?
  - Along with Settings, DocumentGroup, WindowGroup and Window. And "Spaces", for immersive applications?
- We are not in control of when `Window` is created, so we might have to spawn an entity for the user?
  - Similarly, we _can_ control the destruction of the window/scene in multi-scene environments, but not in single-scene apps.
  - Maybe store all windows as a resource, and give read-only access somehow?
- Accessibility. `accesskit_winit` doesn't support iOS either.

How do we actually handle the different kinds of windows?
- UIKit wants you to declare them at a high level in `UISceneConfigurations` `Info.plist`.
  - With a unique application-defined name (which falls back to `"Default Configuration"`).
  - And the class name for the delegate (we'd probably need `BevySceneDelegate` or something?).
- And then we can later select then with `UISceneConfiguration::configurationWithName_sessionRole`.


## Notes on Android activities

Reference:
- https://developer.android.com/guide/components/activities/intro-activities
- https://developer.android.com/guide/components/activities/activity-lifecycle
- https://developer.android.com/guide/components/activities/state-changes
- https://developer.android.com/guide/components/activities/process-lifecycle
- https://developer.android.com/reference/kotlin/android/app/Activity#activity-lifecycle
- https://developer.android.com/reference/kotlin/android/app/Activity#ProcessLifecycle
- https://developer.android.com/develop/ui/compose/layouts/adaptive/support-multi-window-mode#lifecycle

Just to have a comparison point while I do this work. I'm by no means an expert on Android stuff though.

- "Process" ~ `UIApplication`.
- "Activity" ~ `UIWindowScene`.
- Developers have to add activities to `AndroidManifest.xml`.
  - And these declare the expected mime-types etc. accepted when launching.
- The user may launch an activity, or you may do so programmatically
- The activity has a unique application-defined name (which is also the name of the activity class)
- The system completely tears down and re-creates the activity on "configuration" changes (including screen orientation changes)
  - Although this [can be overridden](https://developer.android.com/guide/topics/resources/runtime-changes) for some events.
  - But this probably means that state restoration is _really_ important on Android
- Activities can be "stacked", and may act more similarly to top-level view controllers inside navigation controllers?
  - Collectively called a "task": https://developer.android.com/guide/components/activities/tasks-and-back-stack


## Related Bevy issues

Lifecycle API: https://github.com/bevyengine/bevy/issues/2432
Suspend rendering when in background: https://github.com/bevyengine/bevy/issues/2296


## Use case for multi-window

Get two views into the same world/state:
- Associate different cameras with each window

Run two instances of a game at the same time:
- Associate `SubApp` with each window? Can you create `SubApp`s dynamically?

# Direct UIKit backend for Bevy Engine

[![Latest version](https://badgen.net/crates/v/bevy_uikit)](https://crates.io/crates/bevy_uikit)
[![License](https://badgen.net/badge/license/MIT%20OR%20Apache-2.0/blue)](https://github.com/madsmtm/bevy_uikit/blob/main/README.md)
[![Documentation](https://docs.rs/bevy_uikit/badge.svg)](https://docs.rs/bevy_uikit/)

We alreay have one UI backend in Bevy, namely [`bevy_winit`](https://docs.rs/bevy_winit/), which is based on [`winit`](https://docs.rs/winit/). So why another?

Put simply, the Winit iOS/UIKit backend (which I maintain :roll_eyes:) is quite terrible. So I created this to experiment with what's actually necessary to take game development to the next level on iOS.

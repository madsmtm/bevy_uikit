use bevy::{color::palettes::css::PURPLE, prelude::*};
use bevy_uikit::UIKitPlugin;
use bevy_window::{ExitCondition, WindowEvent};
use tracing::info;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.5, 0.5, 0.9)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { ..default() }),
            exit_condition: ExitCondition::DontExit,
            ..default()
        }))
        .add_plugins(UIKitPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, change_clear_color)
        .add_systems(Update, window_messages)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);
}

fn change_clear_color(input: Res<ButtonInput<KeyCode>>, mut clear_color: ResMut<ClearColor>) {
    if input.just_pressed(KeyCode::Space) {
        clear_color.0 = PURPLE.into();
    }
}

fn window_messages(mut messages: MessageReader<WindowEvent>) {
    for message in messages.read() {
        info!(?message);
    }
}

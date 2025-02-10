use bevy::{color::palettes::css::PURPLE, prelude::*};
use bevy_uikit::UIKitPlugin;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.5, 0.5, 0.9)))
        .add_plugins(DefaultPlugins)
        .add_plugins(UIKitPlugin)
        .add_systems(Startup, setup)
        .add_systems(Update, change_clear_color)
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

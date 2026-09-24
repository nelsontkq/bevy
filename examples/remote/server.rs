//! A Bevy app that you can connect to with the BRP and edit.
//! Run this example with the `remote` feature enabled:
//! ```bash
//! cargo run --example server --features="bevy_remote"
//! ```
//! Spawn a ball:
//! ```bash
//! curl -X POST -d '{"jsonrpc":"2.0","id":1,"method":"example.spawn_ball","params":{"radius": 0.3}}' http://127.0.0.1:15702
//! ```

use bevy::{
    input::common_conditions::input_just_pressed,
    math::ops::{cos, sin},
    prelude::*,
    remote::{http::RemoteHttpPlugin, BrpResult, RemotePlugin},
};
use serde::{Deserialize, Serialize};

const BASE_RADIUS: f32 = 4.0;
const GRAVITY: f32 = 9.8;
const BOUNCE_SPEED: f32 = 5.0;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(RemotePlugin::default().with_method_main("example.spawn_ball", spawn_ball))
        .add_plugins(RemoteHttpPlugin::default())
        .add_systems(Startup, setup)
        .add_systems(Update, remove.run_if(input_just_pressed(KeyCode::Space)))
        .add_systems(Update, bounce)
        .run();
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(BASE_RADIUS))),
        MeshMaterial3d(materials.add(Color::WHITE)),
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(0.5))),
        MeshMaterial3d(materials.add(Color::srgb_u8(124, 144, 255))),
        Transform::from_xyz(0.0, 0.5, 0.0),
        Ball {
            radius: 0.5,
            velocity: BOUNCE_SPEED,
        },
    ));

    commands.insert_resource(TestResource {
        foo: Vec2::new(1.0, -1.0),
        bar: false,
    });

    commands.spawn((
        PointLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

/// An arbitrary resource that can be inspected and manipulated with remote methods.
#[derive(Resource, Reflect, Serialize, Deserialize)]
#[reflect(Resource, Serialize, Deserialize)]
pub struct TestResource {
    /// An arbitrary field of the test resource.
    pub foo: Vec2,

    /// Another arbitrary field.
    pub bar: bool,
}

/// A ball bouncing on the dish.
#[derive(Component, Reflect, Serialize, Deserialize)]
#[reflect(Component, Serialize, Deserialize)]
struct Ball {
    radius: f32,
    velocity: f32,
}

fn bounce(mut balls: Query<(&mut Transform, &mut Ball)>, time: Res<Time>) {
    for (mut transform, mut ball) in &mut balls {
        ball.velocity -= GRAVITY * time.delta_secs();
        transform.translation.y += ball.velocity * time.delta_secs();
        if transform.translation.y < ball.radius {
            transform.translation.y = ball.radius;
            ball.velocity = BOUNCE_SPEED;
        }
    }
}

fn remove(mut commands: Commands, balls: Query<Entity, With<Ball>>) {
    for ball in &balls {
        commands.entity(ball).remove::<Ball>();
    }
}

/// `example.spawn_ball`: Drops another bouncing ball somewhere on the dish.
#[derive(Reflect)]
struct SpawnBallParams {
    /// The radius of the ball. Defaults to 0.3.
    #[reflect(default = "default_radius")]
    radius: f32,
    /// Optional `[r, g, b]` color from 0 to 1.
    color: Option<[f32; 3]>,
}

fn default_radius() -> f32 {
    0.3
}

/// The ball that was spawned.
#[derive(Reflect)]
struct SpawnBallResponse {
    /// The entity holding the new ball.
    entity: Entity,
}

fn spawn_ball(
    In(params): In<SpawnBallParams>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    balls: Query<(), With<Ball>>,
) -> BrpResult<SpawnBallResponse> {
    let n = balls.iter().count() as f32;
    let angle = n * 2.4;
    // randomish
    let distance = (BASE_RADIUS - params.radius) * (n * 0.618).fract().sqrt();
    let color = match params.color {
        Some([r, g, b]) => Color::srgb(r, g, b),
        None => Color::hsl(angle.to_degrees() % 360.0, 0.8, 0.6),
    };
    let entity = commands
        .spawn((
            Mesh3d(meshes.add(Sphere::new(params.radius))),
            MeshMaterial3d(materials.add(color)),
            Transform::from_xyz(distance * cos(angle), 3.0, distance * sin(angle)),
            Ball {
                radius: params.radius,
                velocity: 0.0,
            },
        ))
        .id();
    Ok(SpawnBallResponse { entity })
}

use std::{
    env,
    io::Error as IoError,
    collections::HashMap,
    result::Result,
};

use rapier2d::prelude::*;

use serde::{Deserialize, Serialize};

mod websocket;
mod protocol;

#[derive(Serialize, Deserialize)]
struct Points(Vec<(f32, f32)>);

async fn app(
    send_map: websocket::SendMap,
    mut receive_receiver: websocket::ReceiveReceiver
) {
    let fps: f64 = 60.0;
    let duration: u64 = (1000.0 / fps) as u64;

    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();

    /* Create the ground. */
    let collider = ColliderBuilder::cuboid(100.0, 0.1).build();
    collider_set.insert(collider);

    let mut player_map: protocol::PlayerMap = HashMap::new();

    /* Create other structures necessary for the simulation. */
    let gravity = vector![0.0, -9.81];
    let integration_parameters = IntegrationParameters::default();
    let mut physics_pipeline = PhysicsPipeline::new();
    let mut island_manager = IslandManager::new();
    let mut broad_phase = BroadPhase::new();
    let mut narrow_phase = NarrowPhase::new();
    let mut joint_set = JointSet::new();
    let mut ccd_solver = CCDSolver::new();
    let physics_hooks = ();
    let event_handler = ();

    loop {
        while let Ok(Some(receive)) = receive_receiver.try_next() {
            match receive { 
                websocket::Receive::Connect(addr) => {
                    let rigid_body = RigidBodyBuilder::new_dynamic()
                        .translation(vector![0.0, 10.0])
                        .build();
                    let collider = ColliderBuilder::ball(0.5).restitution(0.7).build();
                    let ball_body_handle = rigid_body_set.insert(rigid_body);
                    collider_set.insert_with_parent(collider, ball_body_handle, &mut rigid_body_set);
                    player_map.insert(addr, protocol::Player::new(ball_body_handle));
                },
                websocket::Receive::Disconnect(addr) => {
                    if let Some(player) = player_map.get(&addr) {
                        rigid_body_set.remove(
                            player.handle,
                            &mut island_manager,
                            &mut collider_set,
                            &mut joint_set,
                        );
                    }
                },
                websocket::Receive::Message(addr, message) => {
                    if let Some(player) = player_map.get(&addr) {
                        rigid_body_set[player.handle]
                            .set_translation(vector!(0.0, 10.0), true);
                    }
                }
            }
        }

        physics_pipeline.step(
            &gravity,
            &integration_parameters,
            &mut island_manager,
            &mut broad_phase,
            &mut narrow_phase,
            &mut rigid_body_set,
            &mut collider_set,
            &mut joint_set,
            &mut ccd_solver,
            &physics_hooks,
            &event_handler,
        );

        let points = player_map
            .iter()
            .map(|(_, player)| player.handle)
            .flat_map(|handle| rigid_body_set.get(handle))
            .map(|ball_body| (
                ball_body.translation().x,
                ball_body.translation().y,
            ))
            .collect();
        let points = Points(points);
        let json = serde_json::to_string(&points).unwrap();

        for (_, sender) in send_map.lock().unwrap().iter_mut() {
            let _ = sender.try_send(websocket::Message::Text(json.clone()));
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(duration)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), IoError> {
    let addr = env::args().nth(1).unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let send_map = websocket::send_map();
    let (receive_sender, receive_receiver) = websocket::receive_channel(128);

    tokio::spawn(app(send_map.clone(), receive_receiver));
    websocket::launch(&addr, send_map, receive_sender, 128).await;

    Ok(())
}

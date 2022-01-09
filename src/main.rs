use std::{
    env,
    io::Error as IoError,
    collections::HashMap,
    result::Result,
    net::SocketAddr,
};

use rapier2d::prelude::*;

use serde::{Deserialize, Serialize};

use rand::Rng;

mod websocket;
mod protocol;

async fn app(
    send_map: websocket::SendMap,
    mut receive_receiver: websocket::ReceiveReceiver
) {
    let fps: f64 = 60.0;
    let duration: u64 = (1000.0 / fps) as u64;

    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();

    /* Create the ground. */
    let collider = ColliderBuilder::cuboid(100.0, 1.0).translation(vector![0.0, -1.0]).build();
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

    let mut rng: rand::rngs::StdRng = rand::SeedableRng::from_seed([13 as u8; 32]);
    loop {
        while let Ok(Some(receive)) = receive_receiver.try_next() {
            match receive { 
                websocket::Receive::Connect(addr) => {
                    let rigid_body = RigidBodyBuilder::new_dynamic()
                        .linear_damping(0.8)
                        .angular_damping(2.0)
                        .translation(vector![
                            rng.gen_range(-1.0...1.0),
                            rng.gen_range(0.5...1.0)
                        ])
                        .build();
                    let collider = ColliderBuilder::cuboid(0.3, 0.2)
                        .restitution(0.45)
                        .density(1.0)
                        .build();
                    let ball_body_handle = rigid_body_set.insert(rigid_body);
                    collider_set.insert_with_parent(collider, ball_body_handle, &mut rigid_body_set);
                    player_map.insert(addr, protocol::Player::new(ball_body_handle));
                    println!(
                        "player_number: {}, session_number: {}",
                        player_map.len(),
                        send_map.lock().unwrap().len()
                    );
                },
                websocket::Receive::Disconnect(addr) => {
                    // receive_channelのキューの上限を超えて同時切断が発生すると
                    // Disconnectイベントの発生漏れが起こる。
                    // そのため現在接続されているセッションに存在しないアドレスを
                    // このタイミングで確認し、削除する。
                    let send_map = send_map.lock().unwrap();
                    let delete_addrs = player_map
                        .iter()
                        .map(|(addr, _)| *addr)
                        .filter(|addr| !send_map.contains_key(&addr))
                        .collect::<Vec<SocketAddr>>();

                    for addr in delete_addrs.iter() {
                        rigid_body_set.remove(
                            player_map.get(&addr).unwrap().handle,
                            &mut island_manager,
                            &mut collider_set,
                            &mut joint_set,
                        );
                        player_map.remove(&addr);
                    }

                    println!(
                        "player_number: {}, session_number: {}",
                        player_map.len(),
                        send_map.len()
                    );
                },
                websocket::Receive::Message(addr, message) => {
                    #[derive(Serialize, Deserialize)]
                    struct Point { x: f32, y: f32 }
                    if let Some(player) = player_map.get(&addr) {
                        if let websocket::Message::Text(message) = message {
                            let point: serde_json::Result<Point> = serde_json::from_str(&message);
                            if let serde_json::Result::Ok(point) = point {
                                rigid_body_set[player.handle]
                                    .set_linvel(vector!(point.x, point.y) * 10.0, true);
                            }
                        }
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

        #[derive(Serialize, Deserialize)]
        struct Points(Vec<(f32, f32, f32, f32)>, u64);

        let points = send_map.lock().unwrap()
            .iter()
            .flat_map(|(addr, _)| player_map.get(addr))
            .flat_map(|player| rigid_body_set.get(player.handle))
            .map(|ball_body| (
                ball_body.translation().x,
                ball_body.translation().y,
                ball_body.rotation().re,
                ball_body.rotation().im,
            ))
            .collect();
        let mut points = Points(points, 0);

        for (_, sender) in send_map.lock().unwrap().iter_mut() {
            let json = serde_json::to_string(&points).unwrap();
            points.1 += 1;
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

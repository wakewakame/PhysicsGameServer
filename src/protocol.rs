use std::{
    collections::HashMap,
    net::SocketAddr,
    option::Option,
};
use rapier2d::prelude::*;
use nalgebra::{Vector2};

pub struct Player {
    pub handle: RigidBodyHandle,
    pub position: Option<Vector2<f32>>,
}
impl Player {
    pub fn new(handle: RigidBodyHandle) -> Player {
        Player{handle, position: None}
    }
}

pub type PlayerMap = HashMap<SocketAddr, Player>;

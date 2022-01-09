use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use futures_channel::mpsc::{channel, Sender, Receiver};
use futures_util::{future, pin_mut, stream::TryStreamExt, StreamExt};

use tokio::net::{TcpListener, TcpStream};

pub use tokio_tungstenite::tungstenite::protocol::Message;

pub enum Receive {
    Connect(SocketAddr),
    Disconnect(SocketAddr),
    Message(SocketAddr, Message),
}

pub type SendMap = Arc<Mutex<HashMap<SocketAddr, Sender<Message>>>>;
pub type ReceiveSender = Sender<Receive>;
pub type ReceiveReceiver = Receiver<Receive>;

pub fn send_map() -> SendMap { Arc::new(Mutex::new(HashMap::new())) }
pub fn receive_channel(buffer: usize) -> (ReceiveSender, ReceiveReceiver) { channel(buffer) }

async fn handle_connection(
    raw_stream: TcpStream,
    addr: SocketAddr,
    send_map: SendMap,
    mut receive_sender: ReceiveSender,
    send_channel_buffer_size: usize,
) {
    println!("Incoming TCP connection from: {}", addr);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    println!("WebSocket connection established: {}", addr);

    // Insert the write part of this peer to the peer map.
    let (tx, rx) = channel(send_channel_buffer_size);
    send_map.lock().unwrap().insert(addr, tx);
    let _ = receive_sender.try_send(Receive::Connect(addr));

    let (outgoing, incoming) = ws_stream.split();

    let broadcast_incoming = incoming.try_for_each(|msg| {
        let _ = receive_sender.try_send(Receive::Message(addr, msg));
        future::ok(())
    });

    let receive_from_others = rx.map(Ok).forward(outgoing);

    pin_mut!(broadcast_incoming, receive_from_others);
    future::select(broadcast_incoming, receive_from_others).await;

    println!("{} disconnected", &addr);
    send_map.lock().unwrap().remove(&addr);
    let _ = receive_sender.try_send(Receive::Disconnect(addr));
}

pub async fn launch(
    addr: &String,
    send_map: SendMap,
    receive_sender: ReceiveSender,
    send_channel_buffer_size: usize
) {
    // Create the event loop and TCP listener we'll accept connections on.
    let try_socket = TcpListener::bind(&addr).await;
    let listener = try_socket.expect("Failed to bind");
    println!("Listening on: {}", addr);

    // Let's spawn the handling of each connection in a separate task.
    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(
            handle_connection(
                stream,
                addr,
                send_map.clone(),
                receive_sender.clone(),
                send_channel_buffer_size,
            )
        );
    }
}

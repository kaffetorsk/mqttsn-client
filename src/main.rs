use tokio::net::UdpSocket;
use mqttsn_client::mqttsn::{MqttSnClient, MqttMessage};
use mqttsn_client::socket::TokioUdp;
use heapless::String;

#[tokio::main]
async fn main() {
    println!("Hello, world!");
    let socket = TokioUdp(UdpSocket::bind("127.0.0.1:3400").await.unwrap());
    let mut mqtt_client = MqttSnClient::new(
        &String::<32>::from("test1"), None, None, socket
    ).unwrap();
    mqtt_client.publish(
        MqttMessage {
            topic: "test/testing".into(),
            payload: "blablabla".into()
        }
    ).await.unwrap();
}

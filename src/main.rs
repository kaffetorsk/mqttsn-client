use tokio::net::UdpSocket;
use mqttsn_client::mqttsn::MqttSnClient;
use mqttsn_client::socket::TokioUdp;
use heapless::String;

#[tokio::main]
async fn main() {
    println!("Hello, world!");
    let socket = TokioUdp(UdpSocket::bind("127.0.0.1:3400").await.unwrap());
    let mqtt_client = MqttSnClient::new(
        &String::<32>::from("test1"), None, None, socket
    ).unwrap();
    mqtt_client.publish().await.unwrap();
}

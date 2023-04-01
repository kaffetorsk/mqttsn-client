use tokio::net::UdpSocket;
use mqttsn_client::mqttsn::{MqttSnClient, MqttMessage};
use mqttsn_client::socket::TokioUdp;
use heapless::String;
use tokio::{runtime, task};



fn main() {
    // Create the runtime
    let rt = runtime::Builder::new_current_thread()
        .build().unwrap();

    // Spawn the root task
    rt.block_on(async {
        // Construct a local task set that can run `!Send` futures.
        let local = task::LocalSet::new();
        // Run the local task set.
        local.run_until(async move {
            let socket = TokioUdp(UdpSocket::bind("127.0.0.1:3400").await.unwrap());
            let mut mqtt_client = MqttSnClient::new(
                &String::<32>::from("test1"), None, None, socket
            ).unwrap();





            mqtt_client.publish(
                MqttMessage::new("test/testing".into(), "blablabla".into())
            ).await.unwrap();
        }).await;
    })
}

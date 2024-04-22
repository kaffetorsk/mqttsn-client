use mqttsn_client::mqttsn::{MqttSnClient, MqttMessage};
use mqttsn_client::dtls_std::DtlsSocket;
use tokio::time::{sleep, Duration};
use log::*;
use embassy_sync::pubsub::PubSubChannel;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use std::net::ToSocketAddrs;

static MQTT_RECV: PubSubChannel::<CriticalSectionRawMutex, MqttMessage, 10, 2, 1> = PubSubChannel::<CriticalSectionRawMutex, MqttMessage, 10, 2, 1>::new();
static MQTT_SEND: PubSubChannel::<CriticalSectionRawMutex, MqttMessage, 10, 1, 2> = PubSubChannel::<CriticalSectionRawMutex, MqttMessage, 10, 1, 2>::new();

#[tokio::main]
async fn main() {
    env_logger::init();
    let socket = DtlsSocket::new().await.unwrap();
    let session = socket.connect(
        "illithid.duckdns.org:3443".to_socket_addrs().unwrap().next().unwrap()
    ).await.unwrap();
    info!("DTLS connected");

    let mut mqtt_client = MqttSnClient::new(
        "test1",
        MQTT_SEND.dyn_subscriber().unwrap(),
        MQTT_RECV.dyn_publisher().unwrap(),
        session
    ).unwrap();
    mqtt_client.connect(120).await.unwrap();
    info!("MQTT-SN connected");

    mqtt_client.subscribe("test/recv".into()).await.unwrap();
    debug!("subscribed");

    let mut mqtt_subscriber = MQTT_RECV.dyn_subscriber().unwrap();
    let mqtt_publisher = MQTT_SEND.dyn_publisher().unwrap();
    
    tokio::join!(
        async {
            loop {
                let msg = mqtt_subscriber.next_message_pure().await;
                info!("new message");
                dbg!(&msg);
            }
        },
        async {
            sleep(Duration::from_secs(12)).await;
            let msg = MqttMessage::new("test/send", "detterenpayload", Some(2)).unwrap();
            mqtt_publisher.publish_immediate(msg);
            sleep(Duration::from_secs(12)).await;
            let msg = MqttMessage::new("test/recv", "detterenpayload2", None).unwrap();
            mqtt_publisher.publish_immediate(msg);
        },
        mqtt_client.run(10)
    );
}

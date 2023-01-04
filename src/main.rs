use axum::body::Body;
use axum::extract::State;
use axum::http;
use axum::response::Response;
use axum::{
    handler::Handler,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use http::response::Builder;
use rand::{thread_rng, Rng};
use tokio::sync::Mutex;
use std::error::Error;
use std::sync::{mpsc, Arc};

use std::thread;
use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time;
use uuid::{uuid, Uuid};

const MAX_CONNECTION_ATTEMPS: i32 = 64;
const HEART_RATE_ID: Uuid = uuid!("00002a37-0000-1000-8000-00805f9b34fb");

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let manager = Manager::new().await.unwrap();

    // get the first bluetooth adapter
    let adapters = manager.adapters().await?;
    let central = adapters.into_iter().nth(0).unwrap();

    // start scanning for devices
    central.start_scan(ScanFilter::default()).await?;
    // instead of waiting, you can use central.events() to get a stream which will
    // notify you of new devices, for an example of that see examples/event_driven_discovery.rs
    time::sleep(Duration::from_secs(2)).await;

    let mut fail = 0;
    // find the device we're interested in
    println!();
    let pine_time = loop {
        match find_watch(&central).await {
            Some(T) => break T,
            None => {
                print!(". ");
                time::sleep(Duration::from_millis(200)).await;
                
                fail += 1;
                if fail > MAX_CONNECTION_ATTEMPS {
                    println!("Reached maximum count of connection attemps");
                    panic!()
                }
                continue;
            }
        }
    };

    // connect to the device
    pine_time.connect().await?;

    // discover services and characteristics
    pine_time.discover_services().await?;

    // find the characteristic we want
    let chars = pine_time.characteristics();
    let cmd_char = chars.iter().find(|c| c.uuid == HEART_RATE_ID).unwrap();

    let channel: (Sender<u8>, Receiver<u8>) = channel(1);

    tokio::spawn(serve(channel.1));

    //Try to print out the heart rate
    loop {
        let h_rate = pine_time.read(cmd_char).await?;
        println!("Heartrate: {} bpm", h_rate.get(1).unwrap());
        channel.0.send(*h_rate.get(1).unwrap()).await;
    }

    Ok(())
}

async fn find_watch(central: &Adapter) -> Option<Peripheral> {
    for p in central.peripherals().await.unwrap() {
        if p.properties()
            .await
            .unwrap()
            .unwrap()
            .local_name
            .map(|name| name.contains("Time"))
            .unwrap_or(false)
        {
            println!("Found watch!");
            return Some(p);
        }
    }
    None
}

async fn serve(mut receiver: Receiver<u8>) {
    let rcvr: Arc<Mutex<Rcvr>> = Arc::new(Mutex::new(Rcvr { receiver: receiver }));

    // build our application with a single route
    let html_content = std::fs::read_to_string("E:/ES/index.html").unwrap();
    let html_site = Html(html_content);
    let site = Router::new()
        .route(
            "/heart_rate",get(send_to_frontend))
        .with_state(rcvr)
        .route("/", get(|| async { html_site }));

    /*heart_rate = match receiver.recv().await {
        Some(T) => T,
        None => {
            return ;
        }
    }*/

    // run it with hyper on localhost:3000
    axum::Server::bind(&"127.0.0.1:3000".parse().unwrap())
        .serve(site.into_make_service())
        .await
        .unwrap();
}

struct Rcvr {
    receiver: Receiver<u8>,
}


impl Rcvr {
    async fn new(receiver: Receiver<u8>) -> Self {
        Rcvr { receiver }
    }
}

async fn test() -> String {
    "Hello".to_string()
}

async fn send_to_frontend(State(rcvr): State<Arc<Mutex<Rcvr>>>,) -> String {
    match rcvr.lock().await.receiver.recv().await {
        Some(T) => T.to_string(),
        None => 0.to_string(),
    }
}

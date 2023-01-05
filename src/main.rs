use axum::extract::State;
use axum::{
    response::{Html},
    routing::get,
    Router,
};
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use tokio::sync::Mutex;
use std::error::Error;
use std::sync::{Arc};

use std::time::Duration;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::time;
use uuid::{uuid, Uuid};

use colored::Colorize;

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
                    let max_msg = String::from("Reached maximum count of connection attemps").red().bold();
                    println!("{}",max_msg);
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

    let sending_channel: (Sender<u8>, Receiver<u8>) = channel(1);
    let mut disconnect_channel: (Sender<i32>, Receiver<i32>) = channel(1);


    tokio::spawn(serve(sending_channel.1, disconnect_channel.0));

    
    //Try to print out the heart rate
    loop {
        let h_rate = pine_time.read(cmd_char).await?;
        println!("Heartrate: {} bpm", h_rate.get(1).unwrap());
        sending_channel.0.send(*h_rate.get(1).unwrap()).await;

        match disconnect_channel.1.try_recv() {
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => (),
            Ok(T) => match T {
                0 => {
                    let dis_msg = String::from("Disconnected").bright_blue().bold();
                    pine_time.disconnect().await;
                    println!("{}", dis_msg);
                    return Ok(());
                },
                _ => ()
            }
            _ => {
                pine_time.disconnect().await;
                return Ok(())
            }
        }
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
            let found_msg = String::from("Found watch!").green();
            println!("{}", found_msg);
            return Some(p);
        }
    }
    None
}

async fn serve(mut receiver: Receiver<u8>, mut sender: Sender<i32>) {
    let rcvr: Arc<Mutex<Rcvr>> = Arc::new(Mutex::new(Rcvr::new(receiver).await));
    let sndr: Arc<Mutex<Sndr>> = Arc::new(Mutex::new(Sndr::new(sender).await));

    // build our application with a single route
    let html_content = std::fs::read_to_string("E:/ES/index.html").unwrap();
    let html_site = Html(html_content);
    let site = Router::new()
        .route(
            "/heart_rate",get(send_to_frontend))
        .with_state(rcvr)
        .route("/", get(|| async { html_site }))
        .route("/disconnect", get(diconn_handler))
        .with_state(sndr);
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

struct Sndr {
    sender: Sender<i32>
}

impl Sndr {
    async fn new(sender: Sender<i32>) -> Self {
        Sndr { sender }
    }
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

async fn diconn_handler(State(sender): State<Arc<Mutex<Sndr>>>) {
    sender.lock().await.sender.send(0).await;
}

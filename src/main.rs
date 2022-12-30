

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Manager, Peripheral};
use rand::{Rng, thread_rng};
use std::error::Error;
use std::thread;
use std::time::Duration;
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
    let pine_time = loop {
        match find_watch(&central).await{
            Some(T) => break T,
            None => {
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



    //Try to print out the heart rate
    loop {
        let h_rate = pine_time.read(cmd_char).await?;
        println!("Heartrate: {} bpm" , h_rate.get(1).unwrap());
            
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
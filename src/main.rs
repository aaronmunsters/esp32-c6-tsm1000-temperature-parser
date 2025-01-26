use std::net::Ipv4Addr;
use std::sync::Mutex;

use esp_idf_svc::hal::delay::BLOCK;
use esp_idf_svc::hal::gpio;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::uart::*;
use esp_idf_svc::hal::units::Hertz;
use esp_idf_svc::http::server::EspHttpServer;
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use esp_idf_svc::ipv4;
use esp_idf_svc::sntp::{EspSntp, SyncStatus};
use esp_idf_svc::wifi::ClientConfiguration;
use esp_idf_svc::{
    eventloop::EspSystemEventLoop, http::server::Configuration as HttpServerConfig,
    nvs::EspDefaultNvsPartition, wifi::EspWifi,
};
use esp_idf_svc::{hal::delay::Delay, wifi::Configuration as WifiConfiguration};

use sensor_storage::{RecordStatus, SensorReadings};
use sensor_storage_to_json::to_json;

use chrono::Utc;
use dotenvy_macro::dotenv;

const PSWD: &'static str = dotenv!("PSWD");
const SSID: &'static str = dotenv!("SSID");

type ReadingTaint = i64;
pub const VALID_RESPONSE_CAPACITY: usize = 128;
pub const SENSOR_ERROR_CAPACITY: usize = 128;
pub const PARSE_ERROR_CAPACITY: usize = 128;
pub const ERROR_RECORDING_CAPACITY: usize = 128;
static SENSOR_STORAGE: Mutex<
    SensorReadings<
        ReadingTaint,
        VALID_RESPONSE_CAPACITY,
        SENSOR_ERROR_CAPACITY,
        PARSE_ERROR_CAPACITY,
        ERROR_RECORDING_CAPACITY,
    >,
> = Mutex::new(SensorReadings::new());

fn update_sensor_storage(taint: ReadingTaint, response: &[u8; 7]) -> RecordStatus {
    SENSOR_STORAGE
        .lock()
        .unwrap()
        .record_reading(taint, response)
}

fn reading_taint() -> ReadingTaint {
    Utc::now().timestamp()
}

fn main() {
    esp_idf_svc::sys::link_patches(); // Required by template; patch to runtime
    esp_idf_svc::log::EspLogger::initialize_default(); // Bind the log crate to the ESP Logging facilities

    let delay: Delay = Delay::new_default();

    let peripherals = Peripherals::take().unwrap();

    let mut wifi_driver = EspWifi::new(
        peripherals.modem,
        EspSystemEventLoop::take().unwrap(),
        Some(EspDefaultNvsPartition::take().unwrap()),
    )
    .unwrap();

    wifi_driver
        .set_configuration(&WifiConfiguration::Client(ClientConfiguration {
            ssid: SSID.try_into().unwrap(),
            password: PSWD.try_into().unwrap(),
            ..Default::default()
        }))
        .unwrap();

    wifi_driver.start().unwrap();
    wifi_driver.connect().unwrap();
    while !wifi_driver.is_connected().unwrap() {
        delay.delay_ms(250);
        let config = wifi_driver.get_configuration().unwrap();
        log::info!("Waiting for station {:?}", config);
    }

    let rx = peripherals.pins.gpio18;
    let tx = peripherals.pins.gpio19;
    let config = config::Config::new().baudrate(Hertz(1200));
    let mut uart = UartDriver::new(
        peripherals.uart1,
        tx,
        rx,
        Option::<gpio::Gpio0>::None,
        Option::<gpio::Gpio1>::None,
        &config,
    )
    .unwrap();
    let (_uart_tx, uart_rx) = uart.split();
    let response: &mut [u8; 7] = &mut [0; 7];

    // Set up HTTP server
    let mut server = EspHttpServer::new(&HttpServerConfig::default()).unwrap();
    // http://<sta ip>/ handler
    server
        .fn_handler("/", Method::Get, |request| {
            let mut response = request.into_ok_response().unwrap();
            response.write_all(include_bytes!("index.html")).unwrap();
            Ok::<(), ()>(())
        })
        .unwrap();

    server
        .fn_handler("/responses.json", Method::Get, |request| {
            let storage = SENSOR_STORAGE.lock().unwrap();
            let json = to_json(&storage);
            let mut response = request
                .into_response(200, Some("OK"), &[("Content-Type", "application/json")])
                .unwrap();
            response.write_all(json.as_bytes()).unwrap();
            Ok::<(), ()>(())
        })
        .unwrap();

    // Synchronize through NTP
    let ntp = EspSntp::new_default().unwrap();
    println!("Synchronizing with NTP Server");
    while ntp.get_sync_status() != SyncStatus::Completed {
        delay.delay_ms(250);
        log::info!("Waiting for NTP Server");
    }
    println!("Time Sync Completed");

    // Default configuration
    let mut ip_info: ipv4::Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);

    log::info!("Connection established!");

    let mut loop_count = 0;
    log::info!("Entering loop!");
    loop {
        // Check if new reading is in...
        if uart_rx.count().unwrap() >= 7 {
            uart_rx.read(response, BLOCK).unwrap();
            let reading = update_sensor_storage(reading_taint(), response);
            match reading {
                ::sensor_storage::RecordStatus::NewReading(_)
                | ::sensor_storage::RecordStatus::ParseError(_) => {
                    log::info!("[{loop_count:5}] New reading: {:?}", reading)
                }
                ::sensor_storage::RecordStatus::NoNewReading => {}
            }
        };
        loop_count += 1;

        let current_ip = wifi_driver.sta_netif().get_ip_info().unwrap().ip;
        if current_ip != ip_info {
            ip_info = current_ip;
            log::info!("[{loop_count:5}] IP info updated: {current_ip:?}");
        }

        delay.delay_ms(100);
    }
}

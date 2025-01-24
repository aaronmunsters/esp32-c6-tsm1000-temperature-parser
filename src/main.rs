use std::net::Ipv4Addr;

use esp_idf_svc::hal::delay::BLOCK;
use esp_idf_svc::hal::gpio;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::uart::*;
use esp_idf_svc::hal::units::Hertz;
use esp_idf_svc::ipv4;
use esp_idf_svc::wifi::ClientConfiguration;
use esp_idf_svc::{hal::delay::Delay, wifi::Configuration};

use esp_idf_svc::{eventloop::EspSystemEventLoop, nvs::EspDefaultNvsPartition, wifi::EspWifi};
mod sensor_storage;
use sensor_storage::*;

use dotenvy_macro::dotenv;

const PSWD: &'static str = dotenv!("PSWD");
const SSID: &'static str = dotenv!("SSID");

fn main() {
    esp_idf_svc::sys::link_patches(); // Required by template; patch to runtime
    esp_idf_svc::log::EspLogger::initialize_default(); // Bind the log crate to the ESP Logging facilities

    let mut sensor_storage = setup_sensor_storage();

    let peripherals = Peripherals::take().unwrap();

    let mut wifi_driver = EspWifi::new(
        peripherals.modem,
        EspSystemEventLoop::take().unwrap(),
        Some(EspDefaultNvsPartition::take().unwrap()),
    )
    .unwrap();

    wifi_driver
        .set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: SSID.try_into().unwrap(),
            password: PSWD.try_into().unwrap(),
            ..Default::default()
        }))
        .unwrap();

    wifi_driver.start().unwrap();
    wifi_driver.connect().unwrap();
    while !wifi_driver.is_connected().unwrap() {
        let config = wifi_driver.get_configuration().unwrap();
        log::info!("Waiting for station {:?}", config);
    }

    let delay: Delay = Delay::new_default();
    let mut loop_count = 0;
    log::info!("Entering loop!");

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

    // Default configuration
    let mut ip_info: ipv4::Ipv4Addr = Ipv4Addr::new(0, 0, 0, 0);

    log::info!("Connection established!");
    loop {
        // Check if new reading is in...
        if uart_rx.count().unwrap() >= 7 {
            uart_rx.read(response, BLOCK).unwrap();
            let reading = sensor_storage.record_reading((), response);
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

pub type ReadingTaint = ();
pub const VALID_RESPONSE_CAPACITY: usize = 32;
pub const SENSOR_ERROR_CAPACITY: usize = 32;
pub const PARSE_ERROR_CAPACITY: usize = 32;
pub const ERROR_RECORDING_CAPACITY: usize = 32;

pub fn setup_sensor_storage() -> sensor_storage::SensorReadings<
    ReadingTaint,
    VALID_RESPONSE_CAPACITY,
    SENSOR_ERROR_CAPACITY,
    PARSE_ERROR_CAPACITY,
    ERROR_RECORDING_CAPACITY,
> {
    sensor_storage::SensorReadings::<
        ReadingTaint,
        VALID_RESPONSE_CAPACITY,
        SENSOR_ERROR_CAPACITY,
        PARSE_ERROR_CAPACITY,
        ERROR_RECORDING_CAPACITY,
    >::new()
}

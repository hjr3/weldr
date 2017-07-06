use std::time::Duration;

#[derive(Debug, Default, Clone)]
pub struct Config {
    pub health_check: HealthCheck,
    pub timeout: Timeout,
}

#[derive(Debug, Clone)]
pub struct HealthCheck {
    /// The time (in seconds) between two consecutive health checks
    pub interval: Duration,

    /// The URI path to health check
    pub uri_path: String,

    /// The number of consecutive health check failures to mark an active server as down
    pub failures: u64,

    /// The number of consecutive health check passes to mark a down server as active
    pub passes: u64,
}

impl Default for HealthCheck {
    fn default() -> HealthCheck {
        HealthCheck {
            interval: Duration::from_secs(10),
            uri_path: "/".to_string(),
            failures: 3,
            passes: 2,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Timeout {
    /// Amount of time to wait connecting
    pub connect: Option<Duration>,

    /// Amount of time to wait writing request
    pub write: Option<Duration>,

    /// Amount of time to wait reading response
    pub read: Option<Duration>,
}

impl Default for Timeout {
    fn default() -> Timeout {
        Timeout {
            connect: Some(Duration::from_millis(200)),
            write: Some(Duration::from_secs(2)),
            read: Some(Duration::from_secs(2)),
        }
    }
}

#[test]
fn test_config() {
    let conf = Config::default();
    assert_eq!(Duration::from_secs(10), conf.health_check.interval);
    assert_eq!("/", conf.health_check.uri_path);
    assert_eq!(Some(Duration::from_millis(200)), conf.timeout.connect);
    assert_eq!(Some(Duration::from_secs(2)), conf.timeout.write);
    assert_eq!(Some(Duration::from_secs(2)), conf.timeout.read);
}

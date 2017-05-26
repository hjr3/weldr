#[derive(Default)]
pub struct Config {
    pub health_check: HealthCheck,
}

pub struct HealthCheck {
    /// The time (in seconds) between two consecutive health checks
    pub interval: u64,

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
            interval: 10,
            uri_path: "/".to_string(),
            failures: 3,
            passes: 2,
        }
    }
}

#[test]
fn test_config() {
    let conf = Config::default();
    assert_eq!(10, conf.health_check.interval);
    assert_eq!("/", conf.health_check.uri_path);
}

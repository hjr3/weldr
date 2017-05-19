pub struct Config {
    pub health: Health,
}

impl Config {
    pub fn new(timeout: u64, uri: String) -> Config {
        Config {
            health: Health {
                timeout: timeout,
                uri: uri,
            }
        }
    }
}

pub struct Health {
    pub timeout: u64,
    pub uri: String,
}

#[test]
fn test_config() {
    let conf = Config::new(10, "/heart_beat".to_string());
    assert_eq!(10, conf.health.timeout);
    assert_eq!("/heart_beat", conf.health.uri);
}

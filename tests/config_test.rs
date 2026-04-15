use whisper_local::config::Config;

#[test]
fn default_config_has_expected_values() {
    let c = Config::default();
    assert_eq!(c.mic_name, "");
    assert_eq!(c.whisper.base_url, "http://localhost:10010");
    assert_eq!(c.whisper.start_url, "http://localhost:9999/start");
    assert_eq!(c.whisper.model_param, "whisper-1");
    assert_eq!(c.whisper.start_body.port, 10010);
}

#[test]
fn roundtrip_toml() {
    let mut c = Config::default();
    c.mic_name = "My Mic".into();
    c.whisper.base_url = "http://other:8080".into();
    let s = toml::to_string(&c).unwrap();
    let back: Config = toml::from_str(&s).unwrap();
    assert_eq!(back.mic_name, "My Mic");
    assert_eq!(back.whisper.base_url, "http://other:8080");
}

#[test]
fn derived_urls() {
    let c = Config::default();
    assert_eq!(c.whisper.transcribe_url(), "http://localhost:10010/v1/audio/transcriptions");
    assert_eq!(c.whisper.health_url(), "http://localhost:10010/health");
}

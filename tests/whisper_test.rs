use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use whisper_local::config::WhisperCfg;

#[tokio::test]
async fn health_up_then_transcribe_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": "hello world"})))
        .mount(&server)
        .await;

    let cfg = WhisperCfg { base_url: server.uri(), ..Default::default() };
    let text = tokio::task::spawn_blocking(move || {
        whisper_local::whisper::transcribe(&make_fake_wav(), "", &cfg).unwrap()
    }).await.unwrap();
    assert_eq!(text, "hello world");
}

#[tokio::test]
async fn health_down_triggers_start_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET")).and(path("/health"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server).await;
    Mock::given(method("GET")).and(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/start"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok":true})))
        .mount(&server).await;
    Mock::given(method("POST")).and(path("/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"text": "up"})))
        .mount(&server).await;

    let cfg = WhisperCfg {
        base_url: server.uri(),
        start_url: format!("{}/start", server.uri()),
        ..Default::default()
    };
    let text = tokio::task::spawn_blocking(move || {
        whisper_local::whisper::transcribe(&make_fake_wav(), "", &cfg).unwrap()
    }).await.unwrap();
    assert_eq!(text, "up");
}

fn make_fake_wav() -> Vec<u8> {
    let samples: Vec<f32> = (0..1600).map(|i| (i as f32 * 0.05).sin()).collect();
    whisper_local::audio::encode_wav(&samples).unwrap()
}

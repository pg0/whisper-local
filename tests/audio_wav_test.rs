use whisper_local::audio::encode_wav;

#[test]
fn wav_header_is_valid() {
    let samples: Vec<f32> = (0..1600).map(|i| (i as f32 * 0.01).sin()).collect();
    let wav = encode_wav(&samples).unwrap();
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    // data chunk: 1600 samples * 2 bytes = 3200
    assert!(wav.len() >= 44 + 3200);
}

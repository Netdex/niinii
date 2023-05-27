use vvcore::VoicevoxCore;
use vvcore::AccelerationMode;

pub static SPEAKER_ID: u32 = 1;
pub static OJT_PATH: &str = "./voicevox_core/open_jtalk_dic_utf_8-1.11";

fn setup() -> VoicevoxCore {
    let dir = std::ffi::CString::new("./voicevox_core/open_jtalk_dic_utf_8-1.11").unwrap();
    let vvc = VoicevoxCore::new_from_options(AccelerationMode::Auto, 0, false, dir.as_c_str()).unwrap();

    vvc.load_model(SPEAKER_ID).unwrap();

    vvc
}

#[test]
fn audio_query() {
    let vvc = setup();
    let opt = VoicevoxCore::make_default_audio_query_options();
    let audio_query = vvc.audio_query("おはよう", SPEAKER_ID, opt);

    assert_eq!(audio_query.is_ok(), true);
}

#[test]
fn synthesis() {
    let vvc = setup();
    let opt = VoicevoxCore::make_default_audio_query_options();
    let audio_query = vvc.audio_query("おはよう", SPEAKER_ID, opt).unwrap();

    let opt = VoicevoxCore::make_default_synthesis_options();
    let wav = vvc.synthesis(audio_query.as_str(), SPEAKER_ID, opt);

    assert_eq!(wav.is_ok(), true);
}

#[test]
fn decode() {
    let vvc = setup();

    // 「テスト」という文章に対応する入力
    const F0_LENGTH: usize = 69;
    let mut f0 = [0.; F0_LENGTH];
    f0[9..24].fill(5.905218);
    f0[37..60].fill(5.565851);

    const PHONEME_SIZE: usize = 45;
    let mut phoneme = [0.; PHONEME_SIZE * F0_LENGTH];
    let mut set_one = |index, range| {
        for i in range {
            phoneme[i * PHONEME_SIZE + index] = 1.;
        }
    };
    set_one(0, 0..9);
    set_one(37, 9..13);
    set_one(14, 13..24);
    set_one(35, 24..30);
    set_one(6, 30..37);
    set_one(37, 37..45);
    set_one(30, 45..60);
    set_one(0, 60..69);

    let r = vvc.decode(&f0, &phoneme, 0);

    assert!(r.is_ok());
    assert_eq!(r.unwrap().as_slice().len(), F0_LENGTH * 256);

}

#[test]
fn is_gpu_mode() {
    let dir = std::ffi::CString::new(OJT_PATH).unwrap();
    let vvc = VoicevoxCore::new_from_options(AccelerationMode::CPU, 0, false, dir.as_c_str()).unwrap();

    assert_eq!(vvc.is_gpu_mode(), false);
}

#[test]
fn is_model_loaded() {
    let vvc = setup();

    assert_eq!(vvc.is_model_loaded(SPEAKER_ID), true);
}

#[test]
fn load_model() {
    let vvc = setup();

    assert_eq!(vvc.load_model(0).unwrap(), ());
}

#[test]
fn predict_duration() {
    let vvc = setup();

    // 「こんにちは、音声合成の世界へようこそ」という文章を変換して得た phoneme_vector
    let phoneme_vector = [
        0, 23, 30, 4, 28, 21, 10, 21, 42, 7, 0, 30, 4, 35, 14, 14, 16, 30, 30, 35, 14, 14, 28,
        30, 35, 14, 23, 7, 21, 14, 43, 30, 30, 23, 30, 35, 30, 0,
    ];

    let r = vvc.predict_duration(&phoneme_vector, 0);
    assert!(r.is_ok());
    assert_eq!(r.unwrap().as_slice().len(), phoneme_vector.len());

}

#[test]
fn predict_intonation() {
    let vvc = setup();

    // 「テスト」という文章に対応する入力
    let vowel_phoneme_vector = [0, 14, 6, 30, 0];
    let consonant_phoneme_vector = [-1, 37, 35, 37, -1];
    let start_accent_vector = [0, 1, 0, 0, 0];
    let end_accent_vector = [0, 1, 0, 0, 0];
    let start_accent_phrase_vector = [0, 1, 0, 0, 0];
    let end_accent_phrase_vector = [0, 0, 0, 1, 0];

    let r = vvc.predict_intonation(
        &vowel_phoneme_vector,
        &consonant_phoneme_vector,
        &start_accent_vector,
        &end_accent_vector,
        &start_accent_phrase_vector,
        &end_accent_phrase_vector,
        0,
    );

    assert!(r.is_ok());
    assert_eq!(r.unwrap().as_slice().len(), vowel_phoneme_vector.len());
}

#[test]
fn tts() {
    let vvc = setup();

    let opt = VoicevoxCore::make_default_tts_options();
    let text: &str = "こんにちは";
    let speaker: u32 = SPEAKER_ID;
    let wav = vvc.tts(text, speaker, opt);

    assert_eq!(wav.is_ok(), true);
}

#[test]
fn tts_simple() {
    let vvc = setup();

    let text: &str = "こんにちは";
    let speaker: u32 = SPEAKER_ID;
    let wav = vvc.tts_simple(text, speaker);

    assert_eq!(wav.is_ok(), true);
}

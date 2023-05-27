use std::io::Write;
use vvcore::*;

fn main() -> Result<(), vvcore::ResultCode> {
    println!("VOICEVOX CORE version: {}", VoicevoxCore::get_version());

    let dir = std::ffi::CString::new("open_jtalk_dic_utf_8-1.11").unwrap();
    let vvc = VoicevoxCore::new_from_options(AccelerationMode::Auto, 0, true, dir.as_c_str())?;

    let text: &str = "こんにちは";
    let speaker: u32 = 1;
    let wav = vvc.tts_simple(text, speaker)?;

    let mut file = std::fs::File::create("audio.wav").unwrap();
    file.write_all(&wav.as_slice()).unwrap();

    Ok(())
}

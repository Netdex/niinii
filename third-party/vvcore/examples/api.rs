use vvcore::*;

fn main() {
    let version = VoicevoxCore::get_version();
    println!("Voicevox version: {}", version);

    let metas_json = VoicevoxCore::get_metas_json();
    println!("Available voice models: {}", metas_json);

    let supported_devices_json = VoicevoxCore::get_supported_devices_json();
    println!("Supported devices: {}", supported_devices_json);

    {
        let mut opt = VoicevoxCore::make_default_initialize_options();
        let dir = std::ffi::CString::new("open_jtalk_dic_utf_8-1.11").unwrap();
        opt.open_jtalk_dict_dir = dir.as_ptr();

        let vvc = match VoicevoxCore::new(opt) {
            Ok(vvc) => vvc,
            Err(e) => panic!("failed to initialize voicevox {:?}", e),
        };

        let speaker_id = 99999;
        let result = vvc.load_model(speaker_id);
        match result {
            Ok(_) => panic!("unexpected"),
            Err(error) => println!(
                "Error loading model for speaker {}: {}",
                speaker_id,
                VoicevoxCore::error_result_to_message(error)
            ),
        }

        let speaker_id = 0;
        let result = vvc.load_model(speaker_id);
        match result {
            Ok(_) => println!("Model for speaker {} loaded successfully", speaker_id),
            Err(_) => panic!("unexpected"),
        }

        let is_loaded = vvc.is_model_loaded(speaker_id);
        println!("Is model for speaker {} loaded: {}", speaker_id, is_loaded);

        let is_gpu = vvc.is_gpu_mode();
        println!("Is running in GPU mode: {}", is_gpu);
    }
}

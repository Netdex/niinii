## VOICEVOX CORE Rust Bindings

This is an unofficial Rust FFI wrapper for VOICEVOX CORE.
It provides a high-level API for calling VOICEVOX CORE.
It also provides a low-level API for directly calling the ffi provided by VOICEVOX CORE.


## Running the Sample

### Required

Please download VOICEVOX CORE using the following method.
https://github.com/VOICEVOX/voicevox_core#%E7%92%B0%E5%A2%83%E6%A7%8B%E7%AF%89

### Sample

```
use std::io::Write;
use vvcore::*;

fn main() {
    let dir = std::ffi::CString::new("open_jtalk_dic_utf_8-1.11").unwrap();
    let vvc = VoicevoxCore::new_from_options(AccelerationMode::Auto, 0, true, dir.as_c_str()).unwrap();

    let text: &str = "こんにちは";
    let speaker: u32 = 1;
    let wav = vvc.tts_simple(text, speaker).unwrap();

    let mut file = std::fs::File::create("audio.wav").unwrap();
    file.write_all(&wav.as_slice()).unwrap();
}
```

### Build and Execution

Please note that the files inside the downloaded voicevox_core directory are required at runtime,
so please place the built binary file in the downloaded voicevox_core directory and execute it.

## Compatibility

The following functions are available as high-level APIs.
In the high-level API, initialization functions and free functions are implemented by RAII.
Also, all functions can be referenced as unsafe functions in the api module.

 - [x] voicevox_make_default_initialize_options
 - [x] voicevox_get_version
 - [x] voicevox_load_model
 - [x] voicevox_is_gpu_mode
 - [x] voicevox_is_model_loaded
 - [x] voicevox_get_metas_json
 - [x] voicevox_get_supported_devices_json
 - [x] voicevox_predict_duration
 - [x] voicevox_predict_intonation
 - [x] voicevox_decode
 - [x] voicevox_make_default_audio_query_options
 - [x] voicevox_audio_query
 - [x] voicevox_make_default_synthesis_options
 - [x] voicevox_synthesis
 - [x] voicevox_make_default_tts_options
 - [x] voicevox_tts
 - [x] voicevox_error_result_to_message
 - [x] ~~voicevox_initialize~~
 - [x] ~~voicevox_finalize~~
 - [x] ~~voicevox_predict_duration_data_free~~
 - [x] ~~voicevox_predict_intonation_data_free~~
 - [x] ~~voicevox_decode_data_free~~
 - [x] ~~voicevox_audio_query_json_free~~
 - [x] ~~voicevox_wav_free~~

## Running the Test

### Example of running on Linux.

Note that it can only be executed in a single thread.

```
## Clone repository
git clone https://github.com/iwase22334/voicevox-core-rs
cd voicevox-core-rs

## Download voicevox core
binary=download-linux-x64
curl -sSfL https://github.com/VOICEVOX/voicevox_core/releases/latest/download/${binary} -o download
chmod +x download
./download

## Run test
(export LD_LIBRARY_PATH=./voicevox_core:$LD_LIBRARY_PATH && cargo test -- --test-threads=1)
```

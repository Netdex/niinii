use std::{path::PathBuf, thread::JoinHandle};

use once_cell::sync::OnceCell;
use std::sync::mpsc::Sender;

use crate::settings::Settings;

use self::protocol::ModelData;

pub mod protocol;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Channel Error: closed")]
    Channel,
    #[error("Unsupported: closed")]
    NotSupported,
}

#[allow(dead_code)]
enum Request {
    Text(String),
    Stop,
}

struct State {
    tx_channel: Sender<Request>,
    model_data: ModelData,
    _thread: JoinHandle<()>,
}

pub struct TtsEngine {
    vvcore_path: PathBuf,
    state: OnceCell<State>,
}

impl TtsEngine {
    pub fn new(settings: &Settings) -> Self {
        TtsEngine {
            vvcore_path: settings.vv_model_path.clone().into(),
            state: OnceCell::new(),
        }
    }

    pub fn request_tts(&self, text: impl Into<String>) -> Result<(), Error> {
        let text = text.into();
        let state = self.state()?;
        state
            .tx_channel
            .send(Request::Text(text))
            .map_err(|_| Error::Channel)
    }

    pub fn stop(&self) {
        if let Ok(state) = self.state() {
            let _ = state.tx_channel.send(Request::Stop);
        }
    }

    pub fn get_model_data(&self) -> Result<&ModelData, Error> {
        let state = self.state()?;
        Ok(&state.model_data)
    }

    #[cfg(not(feature = "voicevox"))]
    fn state(&self) -> Result<&State, Error> {
        let Self {
            vvcore_path: _vvcore_path,
            state: _state,
        } = self;
        Err(Error::NotSupported)
    }

    #[cfg(feature = "voicevox")]
    fn state(&self) -> Result<&State, Error> {
        self.state.get_or_try_init(|| {
            use tokio::sync::oneshot;
            use vvcore::*;

            let models_path = self.vvcore_path.join("model");
            tracing::debug!(?models_path, "check");
            if !models_path.try_exists()? {
                return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
            }
            std::env::set_var(
                "VV_MODELS_ROOT_DIR",
                models_path.into_os_string().into_string().unwrap(),
            );

            let open_jtalk_dic_path = self.vvcore_path.join("open_jtalk_dic_utf_8-1.11");
            tracing::debug!(?open_jtalk_dic_path, "check");
            if !open_jtalk_dic_path.try_exists()? {
                return Err(std::io::Error::from(std::io::ErrorKind::NotFound).into());
            }
            let open_jtalk_dic_str =
                std::ffi::CString::new(open_jtalk_dic_path.into_os_string().into_string().unwrap());

            let (tx_request, rx_request) = std::sync::mpsc::channel();
            let (tx, rx) = oneshot::channel();

            let thread = std::thread::spawn(move || {
                let version = VoicevoxCore::get_version();
                let supported_devices_json = VoicevoxCore::get_supported_devices_json();
                tracing::debug!(version, supported_devices_json);
                let metas_json = VoicevoxCore::get_metas_json();
                let model_data: ModelData = serde_json::from_str(metas_json).unwrap();
                tracing::debug!(?model_data);
                tx.send(model_data).unwrap();

                let vvcore = VoicevoxCore::new_from_options(
                    AccelerationMode::CPU,
                    0,
                    false,
                    open_jtalk_dic_str.unwrap().as_c_str(),
                )
                .unwrap();

                let speaker_id: u32 = 11;
                tracing::debug!(speaker_id, "load_model");
                vvcore.load_model(speaker_id).unwrap();

                let (_stream, stream_handle) = rodio::OutputStream::try_default().unwrap();
                let sink = rodio::Sink::try_new(&stream_handle).unwrap();

                while let Ok(message) = rx_request.recv() {
                    match message {
                        Request::Text(text) => {
                            tracing::debug!(speaker_id, text, "speaker");
                            let wav = vvcore
                                .tts_simple(&text, speaker_id)
                                .unwrap()
                                .as_slice()
                                .to_vec();
                            let cursor = std::io::Cursor::new(wav);
                            let source = rodio::Decoder::new(cursor).unwrap();
                            if !sink.empty() {
                                // https://github.com/RustAudio/rodio/pull/494
                                sink.clear();
                                sink.play();
                            }
                            sink.append(source);
                        }
                        Request::Stop => {
                            if !sink.empty() {
                                sink.clear();
                                sink.play();
                            }
                        }
                    }
                }
            });

            let model_data = rx.blocking_recv().unwrap();

            Ok::<_, Error>(State {
                tx_channel: tx_request,
                model_data,
                _thread: thread,
            })
        })
    }
}

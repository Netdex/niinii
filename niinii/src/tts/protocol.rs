use serde::{Deserialize, Serialize};

pub type ModelData = Vec<Model>;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Model {
    name: String,
    styles: Vec<Style>,
    speaker_uuid: String,
    version: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Style {
    name: String,
    id: u32,
}

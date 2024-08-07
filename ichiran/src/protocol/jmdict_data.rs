use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};
use tokio::fs::File;

use crate::IchiranError;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KwPos {
    pub id: u32,
    pub kw: String,
    pub descr: String,
    pub ents: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct JmDictData {
    pub kwpos_by_kw: HashMap<String, KwPos>,
}
impl JmDictData {
    pub async fn new(jmdict_path: &Path) -> Result<Self, IchiranError> {
        let mut kwpos_by_kw: HashMap<String, KwPos> = HashMap::new();

        let mut kwpos_rdr = csv_async::AsyncReaderBuilder::new()
            .delimiter(b'\t')
            .create_deserializer(File::open(jmdict_path.join("kwpos.csv")).await?);

        let mut records = kwpos_rdr.deserialize::<KwPos>();
        while let Some(result) = records.next().await {
            let record: KwPos = result?;
            kwpos_by_kw.insert(record.kw.clone(), record);
        }

        let mut jmdict_data = Self { kwpos_by_kw };
        jmdict_data.add_errata();
        Ok(jmdict_data)
    }

    fn add_errata(&mut self) {
        // cop-da renamed to cop, but cop-da still exists
        if let Some(cop) = self.kwpos_by_kw.get("cop").cloned() {
            self.kwpos_by_kw.insert("cop-da".to_owned(), cop);
        }
    }
}

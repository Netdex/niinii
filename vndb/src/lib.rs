//! Minimal VNDB Kana API client. Implements only the endpoints niinii needs:
//! visual-novel search and character-by-VN lookup.
//!
//! https://api.vndb.org/kana

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_BASE: &str = "https://api.vndb.org/kana";

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error("vndb: http {status}: {body}")]
    Http { status: u16, body: String },
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    base: String,
}

impl Client {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("niinii-vndb/0.1")
            .timeout(Duration::from_secs(15))
            .build()
            .expect("reqwest client");
        Self {
            http,
            base: DEFAULT_BASE.to_string(),
        }
    }

    async fn post<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, Error> {
        let url = format!("{}{}", self.base, path);
        let resp = self.http.post(&url).json(body).send().await?;
        let status = resp.status();
        let bytes = resp.bytes().await?;
        if !status.is_success() {
            return Err(Error::Http {
                status: status.as_u16(),
                body: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Look up a single VN by its VNDB id (e.g. "v17"). Returns `None` if
    /// the id is unknown.
    pub async fn vn_by_id(&self, vn_id: &str) -> Result<Option<VnSummary>, Error> {
        let body = serde_json::json!({
            "filters": ["id", "=", vn_id],
            "fields": "title,alttitle",
            "results": 1,
        });
        let resp: Page<VnSummary> = self.post("/vn", &body).await?;
        Ok(resp.results.into_iter().next())
    }

    /// Search for visual novels with optional filters and sort.
    pub async fn search_vn(&self, params: &SearchParams) -> Result<Vec<VnSummary>, Error> {
        let mut conds: Vec<serde_json::Value> = Vec::new();
        if !params.query.trim().is_empty() {
            conds.push(serde_json::json!(["search", "=", params.query.trim()]));
        }
        if let Some(c) = or_group("lang", params.langs.iter().map(|s| serde_json::json!(s))) {
            conds.push(c);
        }
        if let Some(c) = or_group(
            "platform",
            params.platforms.iter().map(|s| serde_json::json!(s)),
        ) {
            conds.push(c);
        }
        if let Some(c) = or_group("length", params.lengths.iter().map(|n| serde_json::json!(n))) {
            conds.push(c);
        }
        let filters: serde_json::Value = match conds.len() {
            0 => serde_json::Value::Null,
            1 => conds.into_iter().next().unwrap(),
            _ => {
                let mut v = vec![serde_json::Value::String("and".into())];
                v.extend(conds);
                serde_json::Value::Array(v)
            }
        };
        let mut body = serde_json::json!({
            "fields": "title,alttitle,released,length,rating,votecount,description,developers.name,platforms",
            "results": 25,
            "sort": params.sort.api_str(),
            "reverse": params.reverse,
        });
        if !filters.is_null() {
            body["filters"] = filters;
        }
        let resp: Page<VnSummary> = self.post("/vn", &body).await?;
        Ok(resp.results)
    }

    /// Fetch all characters appearing in the given VN, paginating until done.
    pub async fn characters(&self, vn_id: &str) -> Result<Vec<Character>, Error> {
        let mut out = Vec::new();
        let mut page = 1u32;
        loop {
            let body = serde_json::json!({
                "filters": ["vn", "=", ["id", "=", vn_id]],
                "fields": "name,original,aliases,sex,description,vns.id,vns.role",
                "results": 100,
                "page": page,
            });
            let resp: Page<Character> = self.post("/character", &body).await?;
            let more = resp.more;
            out.extend(resp.results);
            if !more {
                break;
            }
            page += 1;
        }
        Ok(out)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Page<T> {
    results: Vec<T>,
    #[serde(default)]
    more: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VnSummary {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub alttitle: Option<String>,
    /// Release date in "YYYY-MM-DD" form (or partial).
    #[serde(default)]
    pub released: Option<String>,
    /// VNDB length bucket: 1=very short ... 5=very long.
    #[serde(default)]
    pub length: Option<u8>,
    /// Rating in 0-100 (computed from votes).
    #[serde(default)]
    pub rating: Option<f32>,
    #[serde(default)]
    pub votecount: Option<u32>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub developers: Vec<NameRef>,
    #[serde(default)]
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NameRef {
    pub name: String,
}

/// Parameters for [`Client::search_vn`]. Empty multi-value filters mean "no
/// filter on that field"; multiple values within a field are OR'd, and
/// distinct fields are AND'd together.
#[derive(Debug, Clone, Default)]
pub struct SearchParams {
    pub query: String,
    /// Two-letter language codes (e.g. "en", "ja"). Filters on releases.
    pub langs: Vec<String>,
    /// Platform short codes (e.g. "win", "swi", "psv").
    pub platforms: Vec<String>,
    /// VNDB length buckets: each value is in 1..=5.
    pub lengths: Vec<u8>,
    pub sort: SortKey,
    pub reverse: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    SearchRank,
    Title,
    Released,
    Rating,
    VoteCount,
    Popularity,
}

impl Default for SortKey {
    fn default() -> Self {
        Self::SearchRank
    }
}

impl SortKey {
    pub fn api_str(self) -> &'static str {
        match self {
            SortKey::SearchRank => "searchrank",
            SortKey::Title => "title",
            SortKey::Released => "released",
            SortKey::Rating => "rating",
            SortKey::VoteCount => "votecount",
            SortKey::Popularity => "popularity",
        }
    }
}

impl VnSummary {
    pub fn length_label(&self) -> Option<&'static str> {
        Some(match self.length? {
            1 => "very short",
            2 => "short",
            3 => "medium",
            4 => "long",
            5 => "very long",
            _ => return None,
        })
    }
}

/// VNDB sex codes. The API returns a 2-element array `[apparent, spoiler]`;
/// we only surface the apparent value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum Sex {
    #[serde(rename = "m")]
    Male,
    #[serde(rename = "f")]
    Female,
    #[serde(rename = "b")]
    Both,
}

impl Sex {
    pub fn label(self) -> &'static str {
        match self {
            Sex::Male => "male",
            Sex::Female => "female",
            Sex::Both => "both",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CharacterVnRef {
    pub id: String,
    #[serde(default)]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Character {
    pub name: String,
    #[serde(default)]
    pub original: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Apparent sex (spoiler-free). VNDB returns a 2-element array; we keep
    /// only the first slot.
    #[serde(default, deserialize_with = "deserialize_sex_first")]
    pub sex: Option<Sex>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub vns: Vec<CharacterVnRef>,
}

impl Character {
    /// Role for the given VN id, if present in `vns`.
    pub fn role_for(&self, vn_id: &str) -> Option<&str> {
        self.vns
            .iter()
            .find(|v| v.id == vn_id)
            .and_then(|v| v.role.as_deref())
    }
}

/// Build an `["or", [field,"=",v1], [field,"=",v2], ...]` filter group, or
/// return the single `[field,"=",v]` term when there's just one value, or
/// `None` when there are no values.
fn or_group(
    field: &str,
    values: impl IntoIterator<Item = serde_json::Value>,
) -> Option<serde_json::Value> {
    let terms: Vec<serde_json::Value> = values
        .into_iter()
        .map(|v| serde_json::json!([field, "=", v]))
        .collect();
    match terms.len() {
        0 => None,
        1 => Some(terms.into_iter().next().unwrap()),
        _ => {
            let mut out = vec![serde_json::Value::String("or".into())];
            out.extend(terms);
            Some(serde_json::Value::Array(out))
        }
    }
}

fn deserialize_sex_first<'de, D>(de: D) -> Result<Option<Sex>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Option<Vec<Option<Sex>>> = Option::deserialize(de)?;
    Ok(raw.and_then(|v| v.into_iter().next().flatten()))
}

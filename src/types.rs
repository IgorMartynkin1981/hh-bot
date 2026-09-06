use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub login: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub headless: bool,
    pub slow_mo_ms: u64,
    pub storage_dir: PathBuf,
    pub max_applies_per_run: u32,
    pub profile_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            headless: false,
            slow_mo_ms: 400,
            storage_dir: base.join("hh_bot"),
            max_applies_per_run: 10,
            profile_dir: base.join("hh_bot").join("chrome_profile"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResumeData {
    pub title: String,
    pub desired_salary: Option<String>,
    pub about: String,
    pub skills: Vec<String>,
    pub keywords: Vec<String>,
    pub experience_years: u32,
    pub area: Option<String>,
}

impl ResumeData {
    pub fn search_query(&self) -> String {
        self.keywords.join(" ")
    }
}

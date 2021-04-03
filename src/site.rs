
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SiteList {
    sites: Vec<Site>,
}

impl SiteList {
    pub fn new() -> Self {
        SiteList{ sites: Vec::new()}
    }
}

impl SiteList {
    pub fn sites(&self) -> &Vec<Site> {
        &self.sites
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Site {
    name: String,
    description: Option<String>,
    url: String,
    claim_set: Vec<String>,
}



use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct SiteList {
    sites: Vec<Site>,
}

impl SiteList {
    pub fn new() -> Self {
        SiteList{ sites: Vec::new()}
    }
}
#[derive(Deserialize, Debug, Clone)]
pub struct Site {
    name: String,
    description: Option<String>,
    url: String,
    claim_set: Vec<String>,
}


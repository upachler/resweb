use std::collections::HashMap;

const DASHBOARD_HTML_FILENAME: &'static str = "dashboard.html";
const DASHBOARD_HTML_CONTENT: &[u8] = std::include_bytes!("dashboard.html");

pub fn resources() -> HashMap<&'static str, &'static [u8]> {
    let mut m = HashMap::new();
    m.insert(DASHBOARD_HTML_FILENAME, DASHBOARD_HTML_CONTENT);
    m
}
use std::collections::HashMap;

const DASHBOARD_HTML_FILENAME: &'static str = "dashboard.html";
const DASHBOARD_HTML_CONTENT: &[u8] = std::include_bytes!("dashboard.html");

const FAVICON_FILENAME: &'static str = "favicon-32x32.png";
const FAVICON_CONTENT: &[u8] = std::include_bytes!("favicon-32x32.png");

const STYLE_CSS_FILENAME: &'static str = "style.css";
const STYLE_CSS_CONTENT: &[u8] = std::include_bytes!("style.css");

pub fn resources() -> HashMap<&'static str, &'static [u8]> {
    let mut m = HashMap::new();
    m.insert(DASHBOARD_HTML_FILENAME, DASHBOARD_HTML_CONTENT);
    m.insert(FAVICON_FILENAME, FAVICON_CONTENT);
    m.insert(STYLE_CSS_FILENAME, STYLE_CSS_CONTENT);
    m
}
use std::{fs::File, path::{PathBuf}};
use serde::Deserialize;

use clap::{App, Arg};



const CARGO_PKG_VERSION: &'static str = env!("CARGO_PKG_VERSION");
const CARGO_PKG_NAME: &'static str = env!("CARGO_PKG_NAME");

#[derive(Deserialize, Debug)]
struct AppConfigContent {
    port: u16,
    static_file_path: Option<String>,
    authorization_server_url: String,
    client_id: String,
    site_list: crate::site::SiteList,
}

impl AppConfigContent {
    fn into_config(&self) -> Result<crate::AppConfig,String> {
        let static_file_path = match &self.static_file_path {
            None => None,
            Some(s) => {
                match PathBuf::from(s).canonicalize() {
                    Err(e) => return Err(e.to_string()),
                    Ok(p) => { 
                        if !p.exists() {
                            return Err("static_file_path does not exist".into())
                        }
                        Some(Box::from(p.as_path().clone()))
                    },
                }
            }
        };
        let authorization_server_url = match url::Url::parse(&self.authorization_server_url) {
            Ok(u) => u,
            Err(e) => return Err(e.to_string())
        };
        Ok(crate::AppConfig{
            port: self.port,
            authorization_server_url,
            site_list: self.site_list.clone(),
            client_id: self.client_id.clone(),
            static_file_path
        })
    }
}

impl Default for AppConfigContent {
    fn default() -> Self {
        AppConfigContent {
            port: 8081,
            static_file_path: None,
            authorization_server_url: "".into(),
            client_id: "".into(),
            site_list: crate::site::SiteList::new()
        }
    }
}


pub fn read_config() -> Result<crate::AppConfig, ()> {

    let m = App::new(CARGO_PKG_NAME)
    .version(CARGO_PKG_VERSION)
    .arg(Arg::with_name("CONFIG_FILE")
        .required(true)
        .takes_value(true)
        .help("configuration file in YAML format")
    )
    .get_matches();

    
    let config_file_path = m.value_of("CONFIG_FILE").unwrap();

    let config_file = match File::open(config_file_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cannot open configuration file {}: {}", config_file_path, e.to_string());
            return Err(())
        }
    };

    let r: serde_yaml::Result<AppConfigContent> = if config_file_path.ends_with(".yaml") || config_file_path.ends_with(".yml") {
        serde_yaml::from_reader(config_file)
    } else {
        eprintln!("config file name {} must end in .yml or .yaml", config_file_path);
        return Err(());
    };
    
    let cfg = match r {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("error parsing configuration file {}: {}", config_file_path, e.to_string());
            return Err(())
        }
    };

    match cfg.into_config() {
        Ok(c) => Ok(c),
        Err(e) => {
            eprintln!("configuration validation failed:");
            eprintln!("{}", e.to_string());
            Err(())
        }
    }
}

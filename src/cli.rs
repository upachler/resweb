use std::{error::Error, fs::File, net::IpAddr, path::{PathBuf}, str::FromStr};
use serde::Deserialize;

use clap::{App, Arg, SubCommand, AppSettings};

use crate::{AppConfig, CommonConfig, InitTemplatesConfig, error::StringError};



pub const CARGO_PKG_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const CARGO_PKG_NAME: &'static str = env!("CARGO_PKG_NAME");

pub const SERVE_SCMD_NAME: &str = "serve";
pub const INIT_TEMPLATES_SCMD_NAME: &str = "init-templates";
pub const DEVELOPMENT_ARG_NAME: &str = "development";
pub const TEMPLATE_DIR_ARG_NAME: &str = "template_dir";

#[derive(Deserialize, Debug)]
struct ServeConfigContent {
    port: u16,
    interface_addresses: Option<Vec<String>>,
    static_file_path: Option<String>,
    authorization_server_url: String,
    client_id: String,
    site_list: crate::site::SiteList,
    #[serde(default)]
    development: bool,
}

impl ServeConfigContent {
    fn into_config(&self) -> Result<crate::ServeConfig,String> {
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

        // parse interface addresses - if none are given, attempt to determine
        // all existing interfaces and use all of them for binding
        let interface_addresses = if let Some(a) = &self.interface_addresses {
            a.clone()
        } else {
            Vec::new()
        };
        let interface_addresses = if !interface_addresses.is_empty() {
            let converted_ifs = interface_addresses.iter()
            .map(|addr_s|IpAddr::from_str(addr_s))
            .collect::<Vec<_>>();
            if let Some(err) = converted_ifs.iter().find_map(|r|r.as_ref().err()) {
                return Err(String::from("cannot parse interface_addresses: ") + &err.to_string())
            }
            converted_ifs.iter()
            .map(|r|r.as_ref().unwrap().clone())
            .collect::<Vec<_>>()
        } else {
            match get_if_addrs::get_if_addrs() {
                Ok(addrs) => addrs.iter().map(|i|i.ip()).collect(),
                Err(e) => return Err(String::from("error finding all available interfaces failed (search attempted because none were specified): ") + &e.to_string())
            }
        };

        
        Ok(crate::ServeConfig{
            common: CommonConfig::default(),
            port: self.port,
            interface_addresses,
            authorization_server_url,
            site_list: self.site_list.clone(),
            client_id: self.client_id.clone(),
            dev_mode_enabled: self.development,
            static_file_path
        })
    }
}

impl Default for ServeConfigContent {
    fn default() -> Self {
        ServeConfigContent {
            port: 8081,
            static_file_path: None,
            interface_addresses: None,
            authorization_server_url: "".into(),
            client_id: "".into(),
            site_list: crate::site::SiteList::new(), 
            development: false,
        }
    }
}


pub fn read_config() -> Result<crate::AppConfig, Box<dyn Error>> {

    let am = App::new(CARGO_PKG_NAME)
    .version(CARGO_PKG_VERSION)
    .setting(AppSettings::SubcommandRequiredElseHelp)
    .arg(Arg::with_name(TEMPLATE_DIR_ARG_NAME)
        .takes_value(true)
        .help("specifies the path to the template directory, if customizied templates should be used")
        .short("t")
    )
    .subcommand(SubCommand::with_name(SERVE_SCMD_NAME)
        .about((String::new() + "Runs " + CARGO_PKG_NAME + " in server mode, which is typically what you want.").as_str())
        .arg(Arg::with_name("CONFIG_FILE")
            .required(true)
            .takes_value(true)
            .help("configuration file in YAML format")
        )
        .arg(Arg::with_name("development")
            .short("d")
            .long("development")
            .help("if specified, enables auto-reloading of handlebars templates from the template directory ")
        )
    )
    .subcommand(SubCommand::with_name(INIT_TEMPLATES_SCMD_NAME)
        .about("Generates a template directory. Run once before starting development")
        .help((String::new() + "Generate a directory with handlebars templates that can be used as the basis for custom templates. The target directory can be configured using the --" + TEMPLATE_DIR_ARG_NAME + " switch.").as_str())
    )
    .get_matches();

    let mut common = CommonConfig::default();
    if let Some(m) = am.value_of(TEMPLATE_DIR_ARG_NAME) {
        common.template_dir = m.into();
    }

    if let Some(m) = am.subcommand_matches(SERVE_SCMD_NAME) {
        let config_file_path = m.value_of("CONFIG_FILE").unwrap();

        let config_file = match File::open(config_file_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("cannot open configuration file {}: {}", config_file_path, e.to_string());
                return Err(Box::new(e))
            }
        };

        let r: serde_yaml::Result<ServeConfigContent> = if config_file_path.ends_with(".yaml") || config_file_path.ends_with(".yml") {
            serde_yaml::from_reader(config_file)
        } else {
            let msg = std::fmt::format(format_args!("config file name {} must end in .yml or .yaml", config_file_path));
            eprintln!("{}", msg);
            return Err(Box::new(StringError::from(msg)));
        };
        
        let cfg_content = match r {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("error parsing configuration file {}: {}", config_file_path, e.to_string());
                return Err(Box::new(e))
            }
        };

        let mut cfg = match cfg_content.into_config() {
            Ok(v) => v,
            Err(msg) => {
                eprintln!("configuration validation failed:");
                eprintln!("{}", msg.to_string());
                return Err(Box::new(StringError::from(msg)))
            }
        };

        if let Some(v) = m.value_of(TEMPLATE_DIR_ARG_NAME) {
            cfg.common.template_dir = String::from(v);
        }
        
        if m.is_present(DEVELOPMENT_ARG_NAME) {
            cfg.dev_mode_enabled = true;
        }

        Ok(AppConfig::Serve(cfg))
    } else if let Some(_) = am.subcommand_matches(INIT_TEMPLATES_SCMD_NAME){
        Ok(AppConfig::InitTemplates(InitTemplatesConfig { common }))
    } else {
        Err(Box::new(StringError::from("no command specified, should never happen as clap's configuration should prevent that")))
    }
}

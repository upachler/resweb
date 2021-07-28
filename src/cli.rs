use std::{error::Error, fs::{File, canonicalize}, net::IpAddr, path::{PathBuf}, str::FromStr};
use serde::Deserialize;

use clap::{App, AppSettings, Arg, ArgMatches, SubCommand};

use crate::{AppConfig, CommonConfig, InitTemplatesConfig, error::StringError};



pub const CARGO_PKG_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const CARGO_PKG_NAME: &'static str = env!("CARGO_PKG_NAME");

pub const SERVE_SCMD_NAME: &str = "serve";
pub const INIT_TEMPLATES_SCMD_NAME: &str = "init-templates";
pub const DEVELOPMENT_ARG_NAME: &str = "development";
pub const TEMPLATE_DIR_ARG_NAME: &str = "template-dir";

#[derive(Deserialize, Debug)]
struct ServeConfigContent {
    port: u16,
    interface_addresses: Option<Vec<String>>,
    authorization_server_url: String,
    client_id: String,
    site_list: crate::site::SiteList,
    #[serde(default)]
    development: bool,
}

impl ServeConfigContent {
    fn into_config(&self) -> Result<crate::ServeConfig,String> {
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
        })
    }
}

impl Default for ServeConfigContent {
    fn default() -> Self {
        ServeConfigContent {
            port: 8081,
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
        .help(&std::fmt::format(std::format_args!("specifies the path to the template directory, if customizied templates should be used. The value defaults to {} and is ignored if the directory does not exist.", CommonConfig::DEFAULT_TEMPLATE_DIR)))
        .short("t")
        .long(TEMPLATE_DIR_ARG_NAME)
    )
    .subcommand(SubCommand::with_name(SERVE_SCMD_NAME)
        .about((String::new() + "Runs " + CARGO_PKG_NAME + " in server mode, which is typically what you want.").as_str())
        .arg(Arg::with_name("CONFIG_FILE")
            .required(true)
            .takes_value(true)
            .help("configuration file in YAML format")
        )
        .arg(Arg::with_name(DEVELOPMENT_ARG_NAME)
            .short("d")
            .long(DEVELOPMENT_ARG_NAME)
            .help("if specified, enables auto-reloading of handlebars templates from the template directory ")
        )
    )
    .subcommand(SubCommand::with_name(INIT_TEMPLATES_SCMD_NAME)
        .about("Generates a template directory. Run once before starting development")
        .help((String::new() + "Generate a directory with handlebars templates that can be used as the basis for custom templates. The target directory can be configured using the --" + TEMPLATE_DIR_ARG_NAME + " switch.").as_str())
    )
    .get_matches();

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

        if m.is_present(DEVELOPMENT_ARG_NAME) {
            cfg.dev_mode_enabled = true;
        }
        match init_common_config(&am, &mut cfg.common, true) {
            Ok(_) => Ok(AppConfig::Serve(cfg)),
            Err(e) => Err(e)
        }
        
    } else if let Some(_m) = am.subcommand_matches(INIT_TEMPLATES_SCMD_NAME){
        let mut cfg = InitTemplatesConfig { common: CommonConfig::default() };
        match init_common_config(&am, &mut cfg.common, false) {
            Ok(_) => Ok(AppConfig::InitTemplates(cfg)),
            Err(e) => Err(e)
        }
    } else {
        Err(Box::new(StringError::from("no command specified, should never happen as clap's configuration should prevent that")))
    }
}

fn init_common_config(m: &ArgMatches, common: &mut CommonConfig, require_templatedir_exists: bool) -> Result<(), Box<dyn Error>> {
    if let Some(v) = m.value_of(TEMPLATE_DIR_ARG_NAME) {
        match PathBuf::from(v).canonicalize() {
            Ok(p) => {
                if require_templatedir_exists && !p.exists(){
                    let msg = std::fmt::format(format_args!("specified path '{}' does not exist", p.to_string_lossy()));
                    Err(Box::new(StringError::from(msg)))
                } else {
                    common.template_dir = Some(p);
                    Ok(())
                }
            },
            Err(e) => Err(Box::new(e))
        }
    } else {
        match PathBuf::from(CommonConfig::DEFAULT_TEMPLATE_DIR)
            .canonicalize() {
            Ok(p) => {
                common.template_dir = Some(p); 
                Ok(())
            },
            Err(e) => Err(Box::new(e)),
        }
    }
}
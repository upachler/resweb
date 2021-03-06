
mod auth;
mod graphql_schema;
mod cookie_auth;
mod site;
mod cli;
mod templates;
mod error;
mod option_condition;

use actix_web::middleware::Condition;
use serde::{Serialize};

use actix_files::NamedFile;
use actix_session::CookieSession;
use actix_web::dev::ServiceRequest;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_httpauth::extractors::bearer::{BearerAuth, Config};
use actix_web_httpauth::extractors::AuthenticationError;
use actix_web_httpauth::middleware::HttpAuthentication;
use auth::{Claims, OidcAuth};
use serde_json::Map;
use site::{Operator, Operand, Site};
use option_condition::OptionCondition;

use std::fs::{DirBuilder, OpenOptions};
use std::io::Write;
use std::{fmt, path::PathBuf, sync::Arc};


use juniper::{EmptyMutation, EmptySubscription};
use juniper_actix::{graphiql_handler, graphql_handler, playground_handler};
use web::Payload;





use handlebars::Handlebars;

use graphql_schema::{Context, Query, Schema};

const GRAPHQL_PATH: &str = "/graphql";
const EXCHANGE_TOKEN_PATH: &str = "/web/.exchange-token";
const HBS_SUFFIX: &str = ".hbs";

#[derive(fmt::Debug)]
pub enum Error {
    JWKSFetchError,
    CannotFindAuthorizationSigningKey(String),
    TokenExchangeFailure(String),
    TokenExchangeResponseError(auth::ErrorResponse),
    JWTValidationFailed,
}

#[derive(Debug, Clone)]
pub enum AppConfig {
    Serve(ServeConfig),
    InitTemplates(InitTemplatesConfig)
}

impl AppConfig {
    pub fn common(&self) -> &CommonConfig {
        match self {
            Self::Serve(s) => &s.common,
            Self::InitTemplates(i) => &i.common
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommonConfig {
    logging: Option<String>,
    template_dir: Option<PathBuf>,
}

impl CommonConfig {
    pub const DEFAULT_TEMPLATE_DIR: &'static str = "templates";
}

impl Default for CommonConfig {
    fn default() -> Self {
        CommonConfig {
            logging: None,
            template_dir: None
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServeAuthConfig {
    authorization_server_url: url::Url,
    client_id: String,
}
#[derive(Debug, Clone)]
pub struct ServeConfig {
    common: CommonConfig,
    port: u16,
    interface_addresses: Vec<std::net::IpAddr>,
    auth: Option<ServeAuthConfig>,
    scope: String,
    site_list: site::SiteList,
    dev_mode_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct InitTemplatesConfig {
    common: CommonConfig
}

struct WebContext<'a> {
    hb: Handlebars<'a>,
    app_config: ServeConfig,
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::CannotFindAuthorizationSigningKey(kid) => {
                write!(f, "No key with KID {} was found", kid)
            }
            Error::JWKSFetchError => {
                write!(f, "Error while fetching JWKs from authorization server")
            }
            Error::TokenExchangeFailure(msg) => {
                write!(f, "Token exchange with authorization server failed: {}", msg)
            }
            Error::TokenExchangeResponseError(r) => {
                write!(f, "Authorization server returned an error response on token exchange: {:?}", r)
            }
            Error::JWTValidationFailed => {
                write!(f, "token validation failed")
            }
        }
    }
}

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Found().header("location", "/web/index.html").finish()
}

fn matches_operand(v: &serde_json::Value, op: &Operand) -> bool {
    match op {
        Operand::Value {value} => v.eq(value),
        Operand::Regex {regex} => if let Some(s)=v.as_str() {
                regex.is_match(s)
            } else {
                false
            }
    }
}

fn is_site_for_claims(site: &Site, claims: &Claims) -> bool {
    
    for r in &site.claim_rules {

        let v = if let Some(v) = claims.get_path(r.path.as_str()) {
            v
        } else {
            continue
        };


        let rule_matches = match r.operator.clone() {
            Operator::Matches => matches_operand(v, &r.operand),
            Operator::ContainsMatch => if let Some(a) = v.as_array() {
                    a.iter().find(|v| matches_operand(v, &r.operand)).is_some()
                } else {
                    false
                }
        };

        if rule_matches {
            return true
        }
    }
    
    false
}

#[derive(Serialize)]
struct HbsContext <'a> {
    access_token: &'a serde_json::Value,
    sites: Vec<&'a Site>,
}

#[get("/{template_name:.*}")]
async fn handle_web(req: HttpRequest, wc: web::Data<WebContext<'_>>, web::Path(template_name): web::Path<String>) -> impl Responder{
    wc.handle_web(req, &wc, &template_name).await
}

impl WebContext<'_> {

    pub async fn handle_web(&self, req: HttpRequest, wc: &WebContext<'_>, template_name: &String) -> impl Responder{

        if wc.hb.has_template(template_name) {
            
            let ext = req.extensions();
            let claims_opt = ext.get::<Claims>();

            let sites = if let Some(claims) = claims_opt {
                // with claims, we check against them
                wc.app_config.site_list.sites()
                .iter().filter(|site|is_site_for_claims(site, claims))
                .collect()    
            } else if wc.app_config.auth.is_none() {
                // no claims, but auth disabled means we do not check for matching
                // rules, but simply deliver all elements (intended for testing)
                wc.app_config.site_list.sites().iter().collect()
            } else {
                // no claims, auth enabled -> no elements visible
                Vec::new()
            };
            let empty = serde_json::Value::Object(Map::new());
            let ctx = HbsContext {
                access_token: 
                    if let Some(claims) = claims_opt {
                        claims.value()
                    } else {
                        &empty
                    },
                sites: sites
            };
            let content_type = match template_name.rsplit_once(".") {
                Some((_, suffix)) => match suffix.to_ascii_lowercase().as_str() {
                    "html" => "text/html",
                    "txt" => "text/plain",
                    "js" => "application/javascript",
                    "json" => "application/json",
                    _ => "text/plain"
                }
                None => "text/plain"
            };
            return match wc.hb.render(&template_name, &ctx) {
                Ok(body) => HttpResponse::Ok()
                    .set_header("Content-Type", content_type)
                    .body(body),
                Err(e) => HttpResponse::InternalServerError()
                    .set_header("Content-Type", "text/plain")
                    .body(e.desc)                   
            }
        }


        // check for non-template files on file system
        if let Some(template_dir) = wc.app_config.common.template_dir.clone() {
            let dir_content_response = template_dir
                .join(PathBuf::from(&template_name))
                .canonicalize().ok()
                .filter(|p|p.starts_with(&template_dir))
                .and_then(|p|NamedFile::open(p).ok())
                .and_then(|n| n.into_response(&req).ok());
            
            if let Some(response) = dir_content_response {
                return response
            }
        }

        // check for non-template files in builtin list
        if let Some(v) = templates::resources().get(template_name.as_str()) {
            return HttpResponse::Ok().body(*v);
        }

        HttpResponse::NotFound().finish()
    }
}

#[get("/graphql")]
async fn handle_graphql_get(
    req: HttpRequest,
    payload: Payload,
    schema: web::Data<Schema>,
) -> impl Responder {
    let context = Context {};
    graphql_handler(&schema, &context, req, payload).await
}

#[post("/graphql")]
async fn handle_graphql_post(
    req: HttpRequest,
    payload: Payload,
    schema: web::Data<Schema>,
) -> impl Responder {
    let context = Context {};
    graphql_handler(&schema, &context, req, payload).await
}

#[get("/graphiql")]
async fn handle_graphiql() -> impl Responder {
    graphiql_handler(GRAPHQL_PATH, None).await
}

#[get("/playground")]
async fn handle_playground() -> impl Responder {
    playground_handler(GRAPHQL_PATH, None).await
}

async fn validator(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, actix_web::Error> {
    let config = req
        .app_data::<Config>()
        .map(|data| data.clone())
        .unwrap_or_else(Default::default);
    let auth = req.app_data::<Arc<OidcAuth>>()
        .map(|data| data.clone())
        .unwrap();
    match auth.validate_token(credentials.token()).await {
        Ok(_res) => Ok(req),
        Err(_) => Err(AuthenticationError::from(config).into()),
    }
}

#[derive(Clone)]
struct ResWebCookieAuthHandler {
    auth_uri: String,
    oidc_auth: Arc<OidcAuth>,
}

impl ResWebCookieAuthHandler {
    fn new(oidc_auth: Arc<OidcAuth>, auth_uri: String) -> ResWebCookieAuthHandler {
        ResWebCookieAuthHandler {
            auth_uri,
            oidc_auth
        }
    }
}

impl cookie_auth::CookieAuthHandler for ResWebCookieAuthHandler {

    fn oidc_auth(&self) -> Arc<OidcAuth> {
        self.oidc_auth.clone()
    }

    fn client_id(&self) -> &str {
        &self.oidc_auth.client_id()
    }

    fn token_exchange_path(&self) -> &str {
        EXCHANGE_TOKEN_PATH
    }

    fn auth_uri(&self) -> &str {
        &self.auth_uri
    }
}


fn main() {

    // NOTE: we read the config before we initialize logging, because
    // logging config may be in AppConfig - and we cannot reconfigure
    // the logger once it has been initialized...
    let cfg_result = cli::read_config();

    let effective_filters = if let Ok(env_filters) = std::env::var("RUST_LOG") {
            env_filters
        } else {
            let mut filters = "info".into();
            if let Ok(ref cfg) = cfg_result {
                if let Some(configured_filters) = cfg.common().logging.clone() {
                    filters = configured_filters
                } else if let AppConfig::Serve(serve_config) = cfg {
                    if serve_config.dev_mode_enabled {
                        filters = "debug".into()
                    }
                }
            };
            filters
        };
           
    pretty_env_logger::formatted_timed_builder()
    .parse_filters(&effective_filters)
    .init();

    let cfg = match cfg_result {
        Ok(c) => c,
        Err(e) => {
            log::error!("could not establish configuration, because: {}", e);
            std::process::exit(1)
        }
    };

    log::info!("{} version {}", cli::CARGO_PKG_NAME, cli::CARGO_PKG_VERSION);
    if let Some(tdir) = &cfg.common().template_dir {
        log::info!("configured template directory is '{}'", tdir.to_string_lossy());
    }

    match cfg {
        AppConfig::Serve(cfg) => {
            log::info!("Configured to listen on port {} on interfaces {:#?}", cfg.port, cfg.interface_addresses);
        
            tokio::runtime::Builder::new()
            .enable_all()
            .basic_scheduler()
            .build()
            .unwrap()
            .block_on(launch_async(cfg))
            .unwrap()
        }
        AppConfig::InitTemplates(cfg) => {
            match init_templates(&cfg) {
                Ok(path) => log::info!("templates written to directory '{}'", path.to_string_lossy()),
                Err(e) => log::error!("could not write templates ({})", e)
            }
        }
    }
}

async fn launch_async(serve_config: ServeConfig) -> std::io::Result<()> {
    let local_set = tokio::task::LocalSet::new();
    let actix_sys = actix_web::rt::System::run_in_tokio("server", &local_set);

    local_set.spawn_local(async {
        async_main(serve_config).await
    });
    local_set.run_until( async move {
        actix_sys.await
    }).await
}

async fn async_main(serve_config: ServeConfig) -> std::io::Result<()> {
    let addrs = serve_config.interface_addresses
    .iter()
    .filter(|ip|ip.is_ipv4())
    .map(|ip|ip.to_string() + ":" + &serve_config.port.to_string())
    .collect::<Vec<_>>();
    
    let oidc = match &serve_config.auth {
        Some(auth_config) => {
            let auth = Arc::new(OidcAuth::new(auth_config.authorization_server_url.to_string(), &auth_config.client_id, None));
            match auth.get_oidc_config().await {
                Err(e) => {
                    log::error!("cannot load oidc config from IDP at {}", auth_config.authorization_server_url.to_string());
                    let msg = e.to_string();
                    log::error!("detail: {}", msg);
                    return Ok(())
                },
                Ok(oidc_config) => Some((oidc_config, auth))
            }
        },
        None => None
    };

    let template_dir = {
        let d = resolve_template_dir(&serve_config.common);
        if d.exists() {
            log::info!("Using template directory '{}'.", d.to_string_lossy());
            log::info!("Files not found in the template directory will be served from the internal file store");
            Some(d)
        } else {
            log::info!("Using internal template filestore");
            None
        }
    };

    let mut actix_srv = HttpServer::new(move || {
        let mut hb = Handlebars::new();
        hb.set_dev_mode(serve_config.dev_mode_enabled);
        let builtins = templates::resources();
        let builtin_templates = builtins
            .iter()
            .filter_map(|t| match t.0.strip_suffix(HBS_SUFFIX){
                Some(n) => Some((n, t.1)),
                None => None
            });
        
        for t in builtin_templates {
            match hb.register_template_string(t.0, String::from_utf8_lossy(t.1)) {
                Ok(_) => (),
                Err(e) => panic!("could not parse internal template {}", e)
            }
        }
        if let Some(d) = template_dir.clone() {
            hb.register_templates_directory(HBS_SUFFIX, d).unwrap();
        }
        let web_context = web::Data::new(WebContext{hb, app_config: serve_config.clone()});

        let cookie_auth = if let Some((oidc_config, auth)) = &oidc {
            let h = ResWebCookieAuthHandler::new(auth.clone(), oidc_config.authorization_endpoint.clone());
            Some(cookie_auth::CookieAuth::new(h))
        } else {
            None
        };
        
        App::new()
            .service(hello)
            .service(
                web::scope("web")
                .app_data(web_context)
                .wrap(OptionCondition::new(
                    cookie_auth
                ))
                .wrap(Condition::new(oidc.is_some(),
                    CookieSession::private(&[0; 32]) // <- create cookie based session middleware
                    .secure(false)
                ))
                .service(handle_web)
            )
            .service(
                web::scope("gql")
                .data(Schema::new(
                    Query,
                    EmptyMutation::<Context>::new(),
                    EmptySubscription::<Context>::new(),
                ))
                .wrap(HttpAuthentication::bearer(validator))
                .service(handle_graphql_get)
                .service(handle_graphql_post)
                .service(handle_graphiql)
                .service(handle_playground)
            )
    });

    for addr in addrs.iter() {
        actix_srv = actix_srv.bind(addr)?;
    }

    actix_srv.run().await
}

fn resolve_template_dir(cfg: &CommonConfig) -> PathBuf {
    if let Some(p) = cfg.template_dir.clone() {
        p
    } else {
        PathBuf::from(CommonConfig::DEFAULT_TEMPLATE_DIR)
    }
}

fn init_templates(cfg: &InitTemplatesConfig) -> Result<PathBuf,Box<dyn std::error::Error>> {
    let path = resolve_template_dir(&cfg.common);

    match DirBuilder::new()
        .recursive(true)
        .create(&path) {
        Err(e) => return Err(Box::from(e)),
        Ok(_) => (),
    };

    let resources = templates::resources();
    for file_content in resources.iter() {
        let file_path = path.join(file_content.0);
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&file_path) {
            Ok(f) => f,
            Err(e) => {
                log::error!("error writing to '{}'", file_path.to_string_lossy());
                return Err(Box::new(e))
            }
        };
        if let Err(e) = file.write_all(file_content.1) {
            return Err(Box::new(e))
        }
    };
    
    log::info!("Created default templates for customization at path '{}'", path.to_string_lossy());
    log::info!("To start customizing, let {} serve in development mode. To find out how, consult the help, like this:\n", cli::CARGO_PKG_NAME);
    log::info!("\t{} help {}\n", cli::CARGO_PKG_NAME, cli::SERVE_SCMD_NAME);

    Ok(path)
}
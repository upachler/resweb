
mod auth;
mod graphql_schema;
mod cookie_auth;
mod site;
mod cli;

use actix_files::NamedFile;
use actix_session::CookieSession;
use actix_web::dev::ServiceRequest;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_httpauth::extractors::bearer::{BearerAuth, Config};
use actix_web_httpauth::extractors::AuthenticationError;
use actix_web_httpauth::middleware::HttpAuthentication;
use auth::{OidcAuth};

use std::{fmt, path::PathBuf, sync::Arc};


use juniper::{EmptyMutation, EmptySubscription};
use juniper_actix::{graphiql_handler, graphql_handler, playground_handler};
use web::Payload;


use handlebars::Handlebars;

use graphql_schema::{Context, Query, Schema};

const CLIENT_ID: &str = "resweb";
const GRAPHQL_PATH: &str = "/graphql";
const EXCHANGE_TOKEN_PATH: &str = "/web/.exchange-token";

#[derive(fmt::Debug)]
pub enum Error {
    JWKSFetchError,
    CannotFindAuthorizationSigningKey(String),
    TokenExchangeFailure(String),
    TokenExchangeResponseError(auth::ErrorResponse),
    JWTValidationFailed,
}


#[derive(Debug, Clone)]
pub struct AppConfig {
    port: u16,
    interface_addresses: Vec<std::net::IpAddr>,
    static_file_path: Option<Box<std::path::Path>>,
    authorization_server_url: url::Url,
    client_id: String,
    site_list: site::SiteList,
}


struct WebContext<'a> {
    hb: Handlebars<'a>,
    app_config: AppConfig,
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
    HttpResponse::Found().header("location", "/web/dashboard").finish()
}

#[get("/{template_name:.*}")]
async fn handle_web(req: HttpRequest, wc: web::Data<WebContext<'_>>, web::Path(template_name): web::Path<String>) -> impl Responder {
    if wc.hb.has_template(&template_name) {
        match wc.hb.render(&template_name, wc.app_config.site_list.sites()) {
            Ok(body) => HttpResponse::Ok().body(body),
            Err(e) => HttpResponse::InternalServerError().body(e.desc)
        }
    } else if let Some(path) = &wc.app_config.static_file_path {
        // serve static files      
        path
        .join(PathBuf::from(&template_name))
        .canonicalize().ok()
        .filter(|p|p.starts_with(&path))
        .and_then(|p|NamedFile::open(p).ok())
        .and_then(|n| n.into_response(&req).ok())
        .unwrap_or(HttpResponse::NotFound().finish())
    } else {
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
        Ok(res) => Ok(req),
        Err(_) => Err(AuthenticationError::from(config).into()),
    }
}

#[derive(Clone)]
struct ResWebCookieAuthHandler {
    client_id: String,
    auth_uri: String,
    oidc_auth: Arc<OidcAuth>,
}

impl cookie_auth::CookieAuthHandler for ResWebCookieAuthHandler {

    fn oidc_auth(&self) -> Arc<OidcAuth> {
        self.oidc_auth.clone()
    }

    fn client_id(&self) -> &str {
        &self.client_id
    }

    fn token_exchange_path(&self) -> &str {
        EXCHANGE_TOKEN_PATH
    }

    fn auth_uri(&self) -> &str {
        &self.auth_uri
    }
}


fn main() {
    
    let cfg = match cli::read_config() {
        Ok(c) => c,
        Err(_) => std::process::exit(1)
    };

    tokio::runtime::Builder::new()
    .enable_all()
    .build()
    .unwrap()
    .block_on(async_main(cfg))
    .unwrap()
}

async fn async_main(app_config: AppConfig) -> std::io::Result<()> {
    let addrs = app_config.interface_addresses
    .iter()
    .filter(|ip|ip.is_ipv4())
    .map(|ip|ip.to_string() + ":" + &app_config.port.to_string())
    .collect::<Vec<_>>();
    
    let _actix_sys = actix_web::rt::System::run_in_tokio("server", &tokio::task::LocalSet::new());

    let auth = Arc::new(OidcAuth::new(app_config.authorization_server_url.to_string(), &app_config.client_id, None));
    let oidc_config = match auth.get_oidc_config().await {
        Err(_e) => return Ok(()),
        Ok(c) => c
    };

    let mut actix_srv = HttpServer::new(move || {
        let mut hb = Handlebars::new();
        hb.register_templates_directory(".html", "templates").unwrap();
        let web_context = web::Data::new(WebContext{hb, app_config: app_config.clone()});

        let cookie_auth = cookie_auth::CookieAuth::new(ResWebCookieAuthHandler{
            oidc_auth: auth.clone(),
            client_id: CLIENT_ID.into(),
            auth_uri: oidc_config.authorization_endpoint.clone(),
        });
        
        App::new()
            .service(hello)
            .service(
                web::scope("web")
                .app_data(web_context)
                .wrap(cookie_auth)
                .wrap(
                    CookieSession::private(&[0; 32]) // <- create cookie based session middleware
                    .secure(false)
                )
                .service(handle_web)
            )
            .service(
                web::scope("gql")
                .data(Schema::new(
                    Query,
                    EmptyMutation::<Context>::new(),
                    EmptySubscription::<Context>::new(),
                ))
                .data(auth.clone())
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

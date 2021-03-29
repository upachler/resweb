
mod auth;
mod graphql_schema;
mod cookie_auth;

use actix_web::dev::ServiceRequest;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use actix_web_httpauth::extractors::bearer::{BearerAuth, Config};
use actix_web_httpauth::extractors::AuthenticationError;
use actix_web_httpauth::middleware::HttpAuthentication;

use std::fmt;

use juniper::{EmptyMutation, EmptySubscription};
use juniper_actix::{graphiql_handler, graphql_handler, playground_handler};
use web::Payload;


use handlebars::Handlebars;

use graphql_schema::{Context, Query, Schema};

const GRAPHQL_PATH: &str = "/graphql";

#[derive(fmt::Debug)]
pub enum Error {
    JWKSFetchError,
    CannotFindAuthorizationSigningKey(String),
    TokenExchangeError,
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
            Error::TokenExchangeError => {
                write!(f, "Token exchange with authorization server failed")
            }
        }
    }
}

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Found().header("location", "/web/dashboard").finish()
}

#[get("/{template_name}")]
async fn handle_web(hb: web::Data<Handlebars<'_>>, template_name: web::Path<String>) -> impl Responder {
    let data = serde_json::json!({
        "foo": "bar"
    });

    match hb.render(&template_name, &data) {
        Ok(body) => HttpResponse::Ok().body(body),
        Err(e) => HttpResponse::InternalServerError().body(e.desc)
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
    match auth::validate_token(credentials.token()).await {
        Ok(res) => {
            if res == true {
                Ok(req)
            } else {
                Err(AuthenticationError::from(config).into())
            }
        }
        Err(_) => Err(AuthenticationError::from(config).into()),
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addrs = ["127.0.0.1:8081"];
    let _actix_sys = actix_web::rt::System::new("server");

    let mut actix_srv = HttpServer::new(|| {
        let mut hb = Handlebars::new();
        hb.register_templates_directory(".html", "templates").unwrap();
        let hb_data = web::Data::new(hb);
    
        App::new()
            .service(hello)
            .service(
                web::scope("web")
                .app_data(hb_data)
                .wrap(cookie_auth::CookieAuth)
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

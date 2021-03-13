use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use alcoholic_jwt::{JWK, JWKS};
use serde::Deserialize;
use std::fmt;

use juniper::{EmptyMutation, EmptySubscription};
use juniper_actix::{graphiql_handler, graphql_handler, playground_handler};
use web::Payload;

use graphql_schema::{Context, Query, Schema};

mod graphql_schema;

const GRAPHQL_PATH: &str = "/graphql";

#[derive(Deserialize)]
struct OidcConfig {
    jwks_uri: String,
}

#[derive(fmt::Debug)]
enum Error {
    CannotFindAuthorizationSigningKey(String),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::CannotFindAuthorizationSigningKey(kid) => {
                write!(f, "No key with KID {} was found", kid)
            }
        }
    }
}

#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello Resources!")
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

async fn fetch_jwks(uri: &str) -> Result<JWKS, Box<dyn std::error::Error>> {
    let res = reqwest::get(uri).await?;
    let jwks = res.json::<JWKS>().await?;
    Ok(jwks)
}

async fn provide_jwk(kid: &str) -> Result<JWK, Box<dyn std::error::Error>> {
    let discovery_uri = "http://localhost:8080/auth/realms/test/";
    let oidc_config_uri = String::from(discovery_uri) + ".well-known/openid-configuration";
    let oidc_config = reqwest::get(oidc_config_uri)
        .await?
        .json::<OidcConfig>()
        .await?;

    let jwks = fetch_jwks(&oidc_config.jwks_uri).await?;
    println!("jwks: {:?}", jwks);

    jwks.find(kid)
        .map(|jwk| jwk.clone())
        .ok_or_else(|| Error::CannotFindAuthorizationSigningKey(kid.into()).into())
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let addrs = ["127.0.0.1:8081"];

    // only for testing right now
    if let Err(e) = provide_jwk("foo").await {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            e.to_string(),
        ));
    }

    let mut actix_srv = HttpServer::new(|| {
        App::new()
            .data(Schema::new(
                Query,
                EmptyMutation::<Context>::new(),
                EmptySubscription::<Context>::new(),
            ))
            .service(handle_graphql_get)
            .service(handle_graphql_post)
            .service(handle_graphiql)
            .service(handle_playground)
            .service(hello)
    });

    for addr in addrs.iter() {
        actix_srv = actix_srv.bind(addr)?;
    }

    actix_srv.run().await
}

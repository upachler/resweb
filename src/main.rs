
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, get, post, web};
use juniper::{EmptySubscription, EmptyMutation};
use juniper_actix::{graphiql_handler, graphql_handler, playground_handler};
use web::Payload;

use graphql_schema::{Context, Schema, Query};

mod graphql_schema;

const GRAPHQL_PATH: &str = "/graphql";


#[get("/")]
async fn hello() -> impl Responder {
    HttpResponse::Ok().body("Hello Resources!")
}

#[get("/graphql")]
async fn handle_graphql_get(req: HttpRequest, payload: Payload, schema: web::Data<Schema>) -> impl Responder {
    let context = Context{};
    graphql_handler(&schema, &context, req, payload).await
}

#[post("/graphql")]
async fn handle_graphql_post(req: HttpRequest, payload: Payload, schema: web::Data<Schema>) -> impl Responder {
    let context = Context{};
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

#[actix_web::main]
async fn main() -> std::io::Result<()>{
    let addrs = ["127.0.0.1:8080"];


    let mut srv = HttpServer::new(|| {
        App::new()
        .data(Schema::new(Query, EmptyMutation::<Context>::new(), EmptySubscription::<Context>::new()))
        .service(handle_graphql_get)
        .service(handle_graphql_post)
        .service(handle_graphiql)
        .service(handle_playground)
        .service(hello)
    });

    
    for addr in addrs.iter() {
        srv = srv.bind(addr)?;
    }

    srv.run().await
}
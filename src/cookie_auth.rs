
use std::{future::{Future, Ready}, pin::Pin, sync::Arc, task::{Context, Poll}};

use actix_session::UserSession;
use actix_web::{Error, HttpMessage, HttpResponse, dev::{Body, Service, ServiceRequest, ServiceResponse, Transform}, error::{ErrorBadRequest, ErrorInternalServerError}, web};
use url::{Url};

use crate::auth::OidcAuth;

const SESSION_AUTH_KEY: &str = "auth_r";

pub trait CookieAuthHandler : Clone {
    fn oidc_auth(&self) -> Arc<OidcAuth>;
    fn client_id(&self) -> &str;
    fn auth_uri(&self) -> &str;
    fn token_exchange_path(&self) -> &str;
}

pub struct CookieAuth<H> 
where H: CookieAuthHandler
{
    handler: H
}

impl<H> CookieAuth<H>
where  
    H: CookieAuthHandler
{
    pub fn new(handler: H) -> Self {
        Self {handler}
    }

}

// Middleware factory is `Transform` trait from actix-service crate
// `S` - type of the next service
// `B` - type of response's body
impl<S,H> Transform<S> for CookieAuth<H>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<Body>, Error = Error>,
    S::Future: 'static,
    H: CookieAuthHandler,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<Body>;
    type Error = Error;
    type InitError = ();
    type Transform = CookieAuthMiddleware<S,H>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(CookieAuthMiddleware { service, handler: self.handler.clone() }))
    }
}

pub struct CookieAuthMiddleware<S,H> 
where 
    H: CookieAuthHandler
{
    service: S,
    handler: H,
}

impl<S,H> CookieAuthMiddleware<S,H> 
where
    H: CookieAuthHandler,
    S: Service<Request = ServiceRequest, Response = ServiceResponse<Body>, Error = Error>,
    S::Future: 'static,
{
    async fn do_stuff(&mut self, req: ServiceRequest) -> Result<ServiceResponse<Body>, Error>{
        // check if this request is a token exchange attempt
        if req.method() == http::Method::GET && req.uri().path() == self.handler.token_exchange_path() {
            let query_str = req.query_string();
            let q = match web::Query::<WebTokenExcechangeQuery>::from_query(query_str) {
                Ok(q) => q.into_inner(),
                Err(e) => return Box::pin(async{Err(ErrorInternalServerError(e))}),
            };

            let absolute_request_uri = String::from(req.connection_info().scheme()) + "://" + req.connection_info().host() + &req.uri().to_string();
            let token_exchange_uri = match token_exchange_url(self.handler.token_exchange_path(), &absolute_request_uri) {
                Ok(u) => u,
                Err(e) => return Box::pin(async move {Err(ErrorInternalServerError(e))})
            };
            let auth = self.handler.oidc_auth();
            let fut = async move {
                let res = handle_web_token_exchange(auth, &token_exchange_uri, &req, &q).await;
                Ok(req.into_response(res))
            };
            return Box::pin(fut)
        }

        // all other requests are checked for existing auth cookie sessions, and redirected if need be

        // if we were already authorized by another stage during request processing,
        // be happy and use those claims
        let mut claims = req.extensions().get::<crate::auth::Claims>();

        if claims.is_none() {

            let auth_r = req.get_session().get::<String>(SESSION_AUTH_KEY);
            let access_token_r = match auth_r {
                Ok(t) => t,
                Err(e)  => return Box::pin(async {Err(e)}),
            };

            // if we have a token, validate it and store it in request if valid
            claims = if let Some(t) = access_token_r {
                let r = self.handler.oidc_auth().validate_token(&t).await;
                if let Ok(c) = r {
                    req.extensions_mut().insert::<crate::auth::Claims>(c);
                    Some(&c)
                } else {
                    None
                }
            } else {
                None
            };
        }

        if claims.is_some() {
            // if we finally have claims
            return Box::pin(self.service.call(req))
        } else {
            // otherwise (no or invalid token, therefore no claims), 
            // we redirect the user back to the authorization server
            let (hreq, _) = req.into_parts();
            
            let mut auth_request_uri = match Url::parse(&self.handler.auth_uri()) {
                Err(e) => return Box::pin(async move { Err(ErrorInternalServerError(e))}),
                Ok(u) => u
            };
            auth_request_uri.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", self.handler.client_id())
            .append_pair("state", &hreq.uri().to_string());

            let current_request_uri = String::new() + hreq.connection_info().scheme() + "://" + hreq.connection_info().host() + &hreq.uri().to_string();
            
            match token_exchange_url(self.handler.token_exchange_path(), &current_request_uri) {
                Ok(token_exchange_uri) => auth_request_uri
                    .query_pairs_mut()
                    .append_pair("redirect_uri", &token_exchange_uri),
                Err(e) => return Box::pin(async move {Err(ErrorBadRequest(e))})
            };
                
            let hres = HttpResponse::Found().header("location", auth_request_uri.to_string()).finish();
            let res = ServiceResponse::new(hreq, hres);
            return Box::pin(async move {
                Ok(res) 
            })
        }
    }
}

impl<S,H> Service for CookieAuthMiddleware<S,H>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<Body>, Error = Error>,
    S::Future: 'static,
    H: CookieAuthHandler,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<Body>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        Box::pin(async {
            self.do_stuff(req).await
        })
        
    }
}

#[derive(serde::Deserialize)]
pub struct WebTokenExcechangeQuery {
    code: String,
    state: Option<String>,
}

fn token_exchange_url(token_exchange_path: &str, request_url: &str) -> Result<String,url::ParseError> {
    let mut url = url::Url::parse(request_url)?;
    url.set_path(token_exchange_path);
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}

async fn handle_web_token_exchange(auth: Arc<OidcAuth>, token_exchange_url: &str, req: &ServiceRequest, q: &WebTokenExcechangeQuery) -> HttpResponse<Body> {
    let redirect_uri = token_exchange_url;
    let token_response = match auth.exchange_code_for_token(&q.code, Some(&redirect_uri), q.state.as_deref()).await {
        Ok(r) => r,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };

    match req.get_session().set(SESSION_AUTH_KEY, token_response.access_token) {
        Ok(_) => (),
        Err(_e) => return HttpResponse::InternalServerError().finish(),
    }

    let location = match q.state.clone() {
        Some(s) => s,
        None => "/".into(),
    };
    
    HttpResponse::Found()
    .set_header("location", location)
    .finish()
}

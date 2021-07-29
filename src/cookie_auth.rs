
use std::{cell::RefCell, future::{Future, Ready}, pin::Pin, sync::Arc, task::{Context, Poll}};

use actix_session::UserSession;
use actix_web::{Error, HttpMessage, HttpResponse, dev::{Body, Service, ServiceRequest, ServiceResponse, Transform}, error::{ErrorBadRequest, ErrorInternalServerError}, web};
use url::{Url};

use crate::auth::OidcAuth;

const SESSION_AUTH_KEY: &str = "auth_r";

pub trait CookieAuthHandler : Clone {
    const DEFAULT_SCOPES: &'static str = "openid";

    fn oidc_auth(&self) -> Arc<OidcAuth>;
    fn client_id(&self) -> &str;
    fn auth_uri(&self) -> &str;
    fn scopes(&self) -> &str {
        Self::DEFAULT_SCOPES
    }
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
    S: Service<Request = ServiceRequest, Response = ServiceResponse<Body>, Error = Error> + 'static,
    S::Future: 'static,
    H: CookieAuthHandler + 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<Body>;
    type Error = Error;
    type InitError = ();
    type Transform = CookieAuthMiddleware<S,H>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(CookieAuthMiddleware { service: Arc::new(RefCell::new(service)), handler: self.handler.clone() }))
    }
}

pub struct CookieAuthMiddleware<S,H> 
where 
    H: CookieAuthHandler
{
    service: Arc<RefCell<S>>,
    handler: H,
}

impl<S,H> CookieAuthMiddleware<S,H> 
where
    H: CookieAuthHandler,
    S: Service<Request = ServiceRequest, Response = ServiceResponse<Body>, Error = Error>,
    S::Future: 'static,
{
    async fn handle_token_exchange_attempt(handler: H, req: &ServiceRequest) -> Result<HttpResponse, Error> {
        let query_str = req.query_string();
        let q = match web::Query::<WebTokenExcechangeQuery>::from_query(query_str) {
            Ok(q) => q.into_inner(),
            Err(e) => return Err(ErrorInternalServerError(e)),
        };

        let absolute_request_uri = String::from(req.connection_info().scheme()) + "://" + req.connection_info().host() + &req.uri().to_string();
        let token_exchange_uri = match token_exchange_url(handler.token_exchange_path(), &absolute_request_uri) {
            Ok(u) => u,
            Err(e) => return Err(ErrorInternalServerError(e))
        };
        let auth = handler.oidc_auth();
        let fut = async move {
            let res = handle_web_token_exchange(auth, &token_exchange_uri, req, &q).await;
            Ok(res)
        };
        return fut.await
    }

    async fn check_requires_auth(handler: H, req: &ServiceRequest) -> Option<Result<HttpResponse, Error>>{
        // check if this request is a token exchange attempt
        if req.method() == http::Method::GET && req.uri().path() == handler.token_exchange_path() {
            return Some(Self::handle_token_exchange_attempt(handler, req).await)
        }

        // all other requests are checked for existing auth cookie sessions, and redirected if need be

        // if we were already authorized by another stage during request processing,
        // be happy and use those claims
        let has_claims = req.extensions().contains::<crate::auth::Claims>();
        if !has_claims {

            let auth_r = req.get_session().get::<String>(SESSION_AUTH_KEY);
            let access_token_r = match auth_r {
                Ok(t) => t,
                Err(e)  => return Some(Err(e)),
            };

            // if we have a token, validate it and store it in request if valid
            if let Some(t) = access_token_r {
                let r = handler.oidc_auth().validate_token(&t).await;
                if let Ok(c) = r {
                    let mut exts = req.extensions_mut();
                    exts.insert::<crate::auth::Claims>(c);
                    exts.get::<crate::auth::Claims>()
                } else {
                    None
                }
            } else {
                None
            };
        }

        let has_claims = req.extensions().contains::<crate::auth::Claims>();
        if !has_claims {
            // if we still have  no claims
            // (no or invalid token, therefore no claims), 
            // we redirect the user back to the authorization server

            Some(Self::redirect_to_auth_server(handler, req).await)
        } else {
            None
        }
    }

    async fn redirect_to_auth_server(handler: H, hreq: &ServiceRequest) -> Result<HttpResponse, Error> {
        
        let mut auth_request_uri = match Url::parse(&handler.auth_uri()) {
            Err(e) => return Err(ErrorInternalServerError(e)),
            Ok(u) => u
        };
        auth_request_uri.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", handler.client_id())
        .append_pair("state", &hreq.uri().to_string())
        .append_pair("scope", handler.scopes());

        let current_request_uri = String::new() + hreq.connection_info().scheme() + "://" + hreq.connection_info().host() + &hreq.uri().to_string();
        
        match token_exchange_url(handler.token_exchange_path(), &current_request_uri) {
            Ok(token_exchange_uri) => auth_request_uri
                .query_pairs_mut()
                .append_pair("redirect_uri", &token_exchange_uri),
            Err(e) => return Err(ErrorBadRequest(e))
        };
            
        let hres = HttpResponse::Found().header("location", auth_request_uri.to_string()).finish();
        let res = hres;
        Ok(res) 
    }
}

impl<S,H> Service for CookieAuthMiddleware<S,H>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<Body>, Error = Error> + 'static,
    S::Future: 'static,
    H: CookieAuthHandler + 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<Body>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.borrow_mut().poll_ready(cx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future{
        
        let service = self.service.clone();
        let handler = self.handler.clone();

        Box::pin(
            async move {
                if let Some(auth_result) = Self::check_requires_auth(handler, &req).await {
                    return match auth_result {
                        Ok(res) => Ok(req.into_response(res)),
                        Err(e) => Err(e)
                    }
                }

                service.borrow_mut().call(req).await
            }
        )
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

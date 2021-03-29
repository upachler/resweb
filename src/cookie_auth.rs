use std::{future::{Future, Ready}, pin::Pin, task::{Context, Poll}};

use actix_session::UserSession;
use actix_web::{Error, HttpResponse, dev::{Body, Service, ServiceRequest, ServiceResponse, Transform}};

pub struct CookieAuth;

// Middleware factory is `Transform` trait from actix-service crate
// `S` - type of the next service
// `B` - type of response's body
impl<S> Transform<S> for CookieAuth
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<Body>, Error = Error>,
    S::Future: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<Body>;
    type Error = Error;
    type InitError = ();
    type Transform = CookieAuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(CookieAuthMiddleware { service }))
    }
}

pub struct CookieAuthMiddleware<S> {
    service: S,
}

impl<S> Service for CookieAuthMiddleware<S>
where
    S: Service<Request = ServiceRequest, Response = ServiceResponse<Body>, Error = Error>,
    S::Future: 'static,
{
    type Request = ServiceRequest;
    type Response = ServiceResponse<Body>;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, req: ServiceRequest) -> Self::Future {
        let auth_r = req.get_session().get::<String>("auth_r");
        let access_token_r = match auth_r {
            Ok(Some(t)) => t,
            Err(e) => return Box::pin(async {Err(e)}),
            Ok(None) => {
                let (hreq, _) = req.into_parts();
                let hres = HttpResponse::Found().header("location", "http://disney.de").finish();
                let res = ServiceResponse::new(hreq, hres);
                return Box::pin(async move {
                    Ok(res) 
                })
            },
        };

        Box::pin(self.service.call(req))        
    }
}
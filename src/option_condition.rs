use actix_web::dev::{Service, Transform};
use futures_util::future::{ok, Either, FutureExt, LocalBoxFuture};
use std::task::{Context, Poll};

pub struct OptionCondition<T> {
    trans_opt: Option<T>
}

impl <T> OptionCondition<T> {
    pub fn new(trans_opt: Option<T>) -> Self {
        Self {trans_opt}
    } 
}

impl<T> From<Option<T>> for OptionCondition<T> {
    fn from(trans_opt: Option<T>) -> Self {
        Self { trans_opt }
    }
}

pub enum OptionMiddleware<E, D> {
    Enable(E),
    Disable(D),
}

impl<E, D> Service for OptionMiddleware<E, D>
where
    E: Service,
    D: Service<Request = E::Request, Response = E::Response, Error = E::Error>,
{
    type Request = E::Request;
    type Response = E::Response;
    type Error = E::Error;
    type Future = Either<E::Future, D::Future>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        use OptionMiddleware::*;
        match self {
            Enable(service) => service.poll_ready(cx),
            Disable(service) => service.poll_ready(cx),
        }
    }

    fn call(&mut self, req: E::Request) -> Self::Future {
        use OptionMiddleware::*;
        match self {
            Enable(service) => Either::Left(service.call(req)),
            Disable(service) => Either::Right(service.call(req)),
        }
    }
}

impl<S, T> Transform<S> for OptionCondition<T>
where
    S: Service + 'static,
    T: Transform<S, Request = S::Request, Response = S::Response, Error = S::Error>,
    T::Future: 'static,
    T::InitError: 'static,
    T::Transform: 'static,
{
    type Request = S::Request;
    type Response = S::Response;
    type Error = S::Error;
    type InitError = T::InitError;
    type Transform = OptionMiddleware<T::Transform, S>;
    type Future = LocalBoxFuture<'static, Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        match &self.trans_opt {
            Some(trans) => {
                let f = trans.new_transform(service).map(|res| {
                    res.map(
                        OptionMiddleware::Enable as fn(T::Transform) -> Self::Transform,
                    )
                });
                Either::Left(f)
            },
            None => {
                Either::Right(ok(OptionMiddleware::Disable(service)))
            }
        }
        .boxed_local()
    }
}

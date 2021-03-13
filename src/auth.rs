use actix_web::client::Client;
use alcoholic_jwt::{token_kid, validate, Validation, JWK, JWKS};
use serde::{Deserialize, Serialize};
use std::error::Error;

const AUTHORITY_URI: &str = "http://localhost:8080/auth/realms/test";

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    company: String,
    exp: usize,
}


#[derive(Deserialize)]
struct OidcConfig {
    jwks_uri: String,
}

async fn fetch_jwks(client: &Client, uri: &str) -> Result<JWKS, Box<dyn std::error::Error>> {
    let mut res = client.get(uri).send().await?;
    let jwks = res.json::<JWKS>().await?;
    Ok(jwks)
}

async fn provide_jwk(kid: &str) -> Result<JWK, Box<dyn std::error::Error>> {
    let oidc_config_uri = String::from(AUTHORITY_URI) + "/.well-known/openid-configuration";

    let client = Client::default();
    let oidc_config = client.get(oidc_config_uri)
        .send()
        .await?
        .json::<OidcConfig>()
        .await?;

    let jwks = fetch_jwks(&client, &oidc_config.jwks_uri).await?;
    println!("jwks: {:?}", jwks);

    jwks.find(kid)
        .map(|jwk| jwk.clone())
        .ok_or_else(|| crate::Error::CannotFindAuthorizationSigningKey(kid.into()).into())
}

pub async fn validate_token(token: &str) -> Result<bool, crate::Error> {
    let validations = vec![Validation::Issuer(AUTHORITY_URI.into()), Validation::SubjectPresent];
    let kid = match token_kid(&token) {
        Ok(res) => res.expect("failed to decode kid"),
        Err(_) => return Err(crate::Error::JWKSFetchError),
    };
    let jwk = provide_jwk(&kid).await.expect("Specified key not found in set");
    let res = validate(token, &jwk, validations);
    Ok(res.is_ok())
}

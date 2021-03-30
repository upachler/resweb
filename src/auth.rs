use actix_web::client::Client;
use alcoholic_jwt::{token_kid, validate, Validation, JWK, JWKS};
use serde::{Deserialize, Serialize};

use crate::graphql_schema;

const AUTHORITY_URI: &str = "http://localhost:8080/auth/realms/test";

struct OIDCAuth {
    client_id: String,
    client_secret: Option<String>,
    authority_uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    company: String,
    exp: usize,
}


#[derive(Deserialize, Clone, Debug)]
pub struct OidcConfig {
    pub jwks_uri: String,
    pub token_endpoint: String,
    pub authorization_endpoint: String,
}

#[derive(Deserialize, Debug)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub refresh_token: String,
    pub scope: String,
}

#[derive(Deserialize, Debug)]
pub struct ErrorResponse {
    pub error: String,
    pub error_description: Option<String>,
    pub error_uri: Option<String>,
}

async fn fetch_jwks(client: &Client, uri: &str) -> Result<JWKS, Box<dyn std::error::Error>> {
    let mut res = client.get(uri).send().await?;
    let jwks = res.json::<JWKS>().await?;
    Ok(jwks)
}

pub async fn get_oidc_config() -> Result<OidcConfig, Box<dyn std::error::Error>> {
    provide_oidc_config(&Client::default()).await
}

async fn provide_oidc_config(client: &Client) -> Result<OidcConfig, Box<dyn std::error::Error>> {
    let oidc_config_uri = String::from(AUTHORITY_URI) + "/.well-known/openid-configuration";

    let mut res = client.get(oidc_config_uri)
        .send()
        .await?;
    let oidc_config =
        res.json::<OidcConfig>()
        .await?;

    Ok(oidc_config)
}

async fn provide_jwk(kid: &str) -> Result<JWK, Box<dyn std::error::Error>> {
    let client = Client::default();
    let oidc_config = provide_oidc_config(&client).await?;
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

#[derive(Serialize)]
struct AuthServerTokenExchangePayload<'a> {
    grant_type: &'a str,
    client_id: &'a str,
    code: &'a str,
    redirect_uri: Option<&'a str>,
    state: Option<&'a str>,
}

pub async fn exchange_code_for_token(code: &str, redirect_uri: Option<&str>, state: Option<&str>) -> Result<TokenResponse,crate::Error> {
    let client = Client::new();
    
    let oidc_config = match provide_oidc_config(&client).await {
        Ok(c) => c,
        Err(e) => return Err(crate::Error::TokenExchangeFailure(e.to_string()))
    };

    let q = AuthServerTokenExchangePayload {
        grant_type: "authorization_code",
        client_id: "resweb",
        code,
        state,
        redirect_uri
    };
    let post_result = client.post(&oidc_config.token_endpoint)
    .send_form(&q)
    .await;

    let mut response = match post_result {
        Err(e) => return Err(crate::Error::TokenExchangeFailure(e.to_string())),
        Ok(r) => r
    };

    let resonse_status = response.status().as_u16();
    return match resonse_status {
        200 => {
            match response.json::<TokenResponse>().await {
                Ok(r) => Ok(r),
                Err(e) => return Err(crate::Error::TokenExchangeFailure(e.to_string()))
            }
        }
        400 => {
            match response.json::<ErrorResponse>().await {
                Ok(r) => Err(crate::Error::TokenExchangeResponseError(r)),
                Err(e) => return Err(crate::Error::TokenExchangeFailure(e.to_string()))
            }
        }
        _ => return Err(crate::Error::TokenExchangeFailure("invalid auth server token endpoint response status code ".to_owned() + &resonse_status.to_string()))
    };
}
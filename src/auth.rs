use actix_web::client::Client;
use alcoholic_jwt::{token_kid, validate, Validation, JWK, JWKS};
use serde::{Deserialize, Serialize};

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
    token_endpoint: String,
}

#[derive(Deserialize, Debug)]
pub struct TokenResponse {
    access_token: String,
    token_type: String,
    refresh_token: String,
    scope: String,
}

async fn fetch_jwks(client: &Client, uri: &str) -> Result<JWKS, Box<dyn std::error::Error>> {
    let mut res = client.get(uri).send().await?;
    let jwks = res.json::<JWKS>().await?;
    Ok(jwks)
}

async fn provide_oidc_config(client: &Client) -> Result<OidcConfig, Box<dyn std::error::Error>> {
    let oidc_config_uri = String::from(AUTHORITY_URI) + "/.well-known/openid-configuration";

    let oidc_config = client.get(oidc_config_uri)
        .send()
        .await?
        .json::<OidcConfig>()
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

pub async fn exchange_code_for_token(code: &str, redirect_uri: Option<&str>, state: Option<&str>) -> Result<TokenResponse,crate::Error> {
    let client = Client::new();
    
    let oidc_config = provide_oidc_config(&client).await.expect("could not retrieve config");
    
    let mut q = String::new()
    + "grant_type=authorization_code"
    + "&code="+code;

    if let Some(uri) = redirect_uri {
        q = q + "q&redirect_uri=" + uri;
    }

    if let Some(s) = state {
        q = q + "&state" + s;
    }

    let post_result = client.post(&oidc_config.token_endpoint)
    .send_form(&q)
    .await;

    let mut response = match post_result {
        Err(_) => return Err(crate::Error::TokenExchangeError),
        Ok(r) => r
    };

    
    return match response.status().as_u16() {
        200 | 400 => {
            match response.json::<TokenResponse>().await {
                Ok(r) => Ok(r),
                Err(_) => return Err(crate::Error::TokenExchangeError)
            }
        }
        _ => return Err(crate::Error::TokenExchangeError)
    };
}
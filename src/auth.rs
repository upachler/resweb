use actix_web::client::Client;
use alcoholic_jwt::{token_kid, validate, Validation, JWK, JWKS};
use serde::{Deserialize, Serialize};

pub struct OidcAuth {
    client_id: String,
    client_secret: Option<String>,
    authority_uri: String,
}

impl OidcAuth {
    pub fn new(authority_uri: String, client_id: &str, client_secret: Option<&str>) -> Self {
        OidcAuth {
            authority_uri,
            client_id: client_id.into(),
            client_secret: client_secret.map(String::from),
        }
    }
}

pub struct Claims(serde_json::Value);

impl Claims {
    pub fn get_path(&self, path: &str) -> Option<&serde_json::Value> {
        let mut it = path.split(".");
        let mut key_opt = it.next();

        let mut v_opt = Some(&self.0);
        while let Some(key) = key_opt {
            if let Some(v) = v_opt {
                v_opt = v.get(key)
            } else {
                break;
            }
            key_opt = it.next();
        }

        v_opt
    }
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

impl OidcAuth {
    pub async fn get_oidc_config(&self) -> Result<OidcConfig, Box<dyn std::error::Error>> {
        self.provide_oidc_config(&Client::default()).await
    }

    async fn provide_oidc_config(
        &self,
        client: &Client,
    ) -> Result<OidcConfig, Box<dyn std::error::Error>> {
        let oidc_config_uri =
            String::from(&self.authority_uri) + "/.well-known/openid-configuration";

        let mut res = client.get(oidc_config_uri).send().await?;
        let oidc_config = res.json::<OidcConfig>().await?;

        Ok(oidc_config)
    }

    async fn provide_jwk(&self, kid: &str) -> Result<JWK, Box<dyn std::error::Error>> {
        let client = Client::default();
        let oidc_config = self.provide_oidc_config(&client).await?;
        let jwks = fetch_jwks(&client, &oidc_config.jwks_uri).await?;
        log::debug!("fetched server public JSON web keys: {:?}", jwks);

        jwks.find(kid)
            .map(|jwk| jwk.clone())
            .ok_or_else(|| crate::Error::CannotFindAuthorizationSigningKey(kid.into()).into())
    }

    pub async fn validate_token(&self, token: &str) -> Result<Claims, crate::Error> {
        let validations = vec![
            Validation::NotExpired,
            Validation::Issuer(self.authority_uri.clone()),
            Validation::SubjectPresent,
        ];
        let kid = match token_kid(&token) {
            Ok(res) => res.expect("failed to decode kid"),
            Err(_) => return Err(crate::Error::JWKSFetchError),
        };
        let jwk = self
            .provide_jwk(&kid)
            .await
            .expect("Specified key not found in set");
        let res = validate(token, &jwk, validations);

        match res {
            Ok(c) => Ok(Claims(c.claims)),
            Err(e) => {
                log::debug!("token validation failed: {:?}; token was: {}", e, token);
                Err(crate::Error::JWTValidationFailed)
            }
        }
    }
}

#[derive(Serialize)]
struct AuthServerTokenExchangePayload<'a> {
    grant_type: &'a str,
    client_id: &'a str,
    code: &'a str,
    redirect_uri: Option<&'a str>,
    state: Option<&'a str>,
}

impl OidcAuth {
    pub async fn exchange_code_for_token(
        &self,
        code: &str,
        redirect_uri: Option<&str>,
        state: Option<&str>,
    ) -> Result<TokenResponse, crate::Error> {
        let client = Client::new();

        let oidc_config = match self.provide_oidc_config(&client).await {
            Ok(c) => c,
            Err(e) => return Err(crate::Error::TokenExchangeFailure(e.to_string())),
        };

        let q = AuthServerTokenExchangePayload {
            grant_type: "authorization_code",
            client_id: &self.client_id,
            code,
            state,
            redirect_uri,
        };
        let mut post_req = client.post(&oidc_config.token_endpoint);
        if let Some(client_secret) = &self.client_secret {
            post_req = post_req.basic_auth(&self.client_id, Some(client_secret));
        }
        let post_result = post_req.send_form(&q).await;

        let mut response = match post_result {
            Err(e) => return Err(crate::Error::TokenExchangeFailure(e.to_string())),
            Ok(r) => r,
        };

        let resonse_status = response.status().as_u16();
        return match resonse_status {
            200 => match response.json::<TokenResponse>().await {
                Ok(r) => Ok(r),
                Err(e) => return Err(crate::Error::TokenExchangeFailure(e.to_string())),
            },
            400 => match response.json::<ErrorResponse>().await {
                Ok(r) => Err(crate::Error::TokenExchangeResponseError(r)),
                Err(e) => return Err(crate::Error::TokenExchangeFailure(e.to_string())),
            },
            _ => {
                return Err(crate::Error::TokenExchangeFailure(
                    "invalid auth server token endpoint response status code ".to_owned()
                        + &resonse_status.to_string(),
                ))
            }
        };
    }
}

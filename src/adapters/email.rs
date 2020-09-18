use oauth2::basic::BasicClient;
use oauth2::{AuthUrl, ClientId, ClientSecret, TokenUrl};
use std::result::Result as StdResult;

#[derive(Debug, Fail)]
enum GmailError {
    #[fail(display = "invalid Auth URL")]
    InvalidAuthUrl,
    #[fail(display = "invalid Token URL")]
    InvalidTokenUrl,
    #[fail(display = "the Gmail builder was not used correctly")]
    IncompleteBuilder,
}

pub struct GmailBuilder {
    client_id: Option<ClientId>,
    secret: Option<ClientSecret>,
    auth_url: Option<AuthUrl>,
    token_url: Option<TokenUrl>,
}

impl GmailBuilder {
    pub fn new() -> Self {
        GmailBuilder {
            client_id: None,
            secret: None,
            auth_url: None,
            token_url: None,
        }
    }
    pub fn client_id(self, client_id: &str) -> Self {
        GmailBuilder {
            client_id: Some(ClientId::new(client_id.to_string())),
            ..self
        }
    }
    pub fn secret(self, secret: &str) -> Self {
        GmailBuilder {
            secret: Some(ClientSecret::new(secret.to_string())),
            ..self
        }
    }
    pub fn auth_url(self, secret: &str) -> Result<Self, GmailError> {
        Ok(GmailBuilder {
            auth_url: Some(
                AuthUrl::new(secret.to_string()).map_err(|_| GmailError::InvalidAuthUrl)?,
            ),
            ..self
        })
    }
    pub fn token_url(self, url: &str) -> Result<Self, GmailError> {
        Ok(GmailBuilder {
            token_url: Some(
                TokenUrl::new(url.to_string()).map_err(|_| GmailError::InvalidTokenUrl)?,
            ),
            ..self
        })
    }
    pub fn build(self) -> Result<Gmail, GmailError> {
        Ok(Gmail {
            client: BasicClient::new(
                self.client_id.ok_or(GmailError::IncompleteBuilder)?,
                self.secret,
                self.auth_url.ok_or(GmailError::IncompleteBuilder)?,
                self.token_url,
            ),
        })
    }
}

pub struct Gmail {
    client: BasicClient,
}

impl Gmail {

}

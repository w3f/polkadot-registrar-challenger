use crate::primitives::Result;
use jwt::algorithm::openssl::PKeyWithDigest;
use jwt::algorithm::AlgorithmType;
use jwt::header::{Header, HeaderType};
use jwt::{SignWithKey, Token};
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Value, ValueRef};
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::result::Result as StdResult;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
pub struct EmailId(String);

impl EmailId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for EmailId {
    fn from(val: String) -> Self {
        EmailId(val)
    }
}

impl From<&str> for EmailId {
    fn from(val: &str) -> Self {
        EmailId(val.to_string())
    }
}

impl ToSql for EmailId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(ValueRef::Text(self.0.as_bytes())))
    }
}

impl FromSql for EmailId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Text(val) => Ok(EmailId(
                String::from_utf8(val.to_vec()).map_err(|_| FromSqlError::InvalidType)?,
            )),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

#[derive(Debug, Fail)]
pub enum ClientError {
    #[fail(display = "the builder was not used correctly")]
    IncompleteBuilder,
    #[fail(display = "the access token was not requested for the client")]
    MissingAccessToken,
    #[fail(display = "Unrecognized data returned from the Twitter API")]
    UnrecognizedData,
}

pub struct ClientBuilder {
    jwt: Option<JWT>,
    token_url: Option<String>,
    user_id: Option<String>,
}

impl ClientBuilder {
    pub fn new() -> Self {
        ClientBuilder {
            jwt: None,
            token_url: None,
            user_id: None,
        }
    }
    pub fn jwt(self, jwt: JWT) -> Self {
        ClientBuilder {
            jwt: Some(jwt),
            ..self
        }
    }
    pub fn user_id(self, user_id: String) -> Self {
        ClientBuilder {
            user_id: Some(user_id),
            ..self
        }
    }
    pub fn token_url(self, url: String) -> Self {
        ClientBuilder {
            token_url: Some(url),
            ..self
        }
    }
    pub fn build(self) -> Result<Client> {
        Ok(Client {
            client: ReqClient::new(),
            jwt: self.jwt.ok_or(ClientError::IncompleteBuilder)?,
            token_url: self.token_url.ok_or(ClientError::IncompleteBuilder)?,
            token_id: None,
            user_id: self.user_id.ok_or(ClientError::IncompleteBuilder)?,
        })
    }
}

pub struct Client {
    client: ReqClient,
    jwt: JWT,
    token_url: String,
    token_id: Option<String>,
    user_id: String,
}

use reqwest::header::{self, HeaderValue};
use reqwest::Client as ReqClient;

impl Client {
    pub async fn token_request(&mut self) -> Result<()> {
        #[derive(Debug, Deserialize)]
        struct TokenResponse {
            access_token: String,
            #[allow(dead_code)]
            expires_in: usize,
        }

        let request = self
            .client
            .post(&self.token_url)
            .header(
                self::header::CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
            .body(reqwest::Body::from(format!(
                "grant_type={}&assertion={}",
                "urn:ietf:params:oauth:grant-type:jwt-bearer", self.jwt.0
            )))
            .build()?;

        println!("DEBUG token request: {:?}", request);

        let response = self
            .client
            .execute(request)
            .await?
            .json::<TokenResponse>()
            .await?;

        println!("DEBUG token response: {:?}", response);

        self.token_id = Some(response.access_token);

        Ok(())
    }
    pub async fn get_request<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let request = self
            .client
            .get(url)
            .header(
                self::header::CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
            .header(
                self::header::AUTHORIZATION,
                HeaderValue::from_str(&format!(
                    "Bearer {}",
                    self.token_id
                        .as_ref()
                        .ok_or(ClientError::MissingAccessToken)?
                ))?,
            )
            .build()?;

        println!("DEBUG endpoint request: {:?}", request);

        Ok(self.client.execute(request).await?.json::<T>().await?)
    }
    pub async fn request_message(&self, email_id: &EmailId) -> Result<()> {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiMessage {
            pub id: String,
            pub thread_id: String,
            pub payload: ApiPayload,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiPayload {
            pub headers: Vec<Header>,
            pub parts: Vec<ApiPart>,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct Header {
            pub name: String,
            pub value: String,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiPart {
            pub part_id: String,
            pub mime_type: String,
            pub filename: String,
            pub body: ApiBody,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct ApiBody {
            pub size: i64,
            pub data: Option<String>,
        }

        let message = self
            .get_request::<ApiMessage>(&format!(
                "https://gmail.googleapis.com/gmail/v1/users/{userId}/messages/{id}",
                userId = self.user_id,
                id = email_id.as_str(),
            ))
            .await?;

        Ok(())
    }
}

use std::collections::BTreeMap;

pub struct JWTBuilder<'a> {
    claims: BTreeMap<&'a str, &'a str>,
}

pub struct JWT(String);

impl<'a> JWTBuilder<'a> {
    pub fn new() -> JWTBuilder<'a> {
        JWTBuilder {
            claims: BTreeMap::new(),
        }
    }
    pub fn issuer(mut self, iss: &'a str) -> Self {
        self.claims.insert("iss", iss);
        self
    }
    pub fn scope(mut self, scope: &'a str) -> Self {
        self.claims.insert("scope", scope);
        self
    }
    pub fn sub(mut self, sub: &'a str) -> Self {
        self.claims.insert("sub", sub);
        self
    }
    pub fn audience(mut self, aud: &'a str) -> Self {
        self.claims.insert("aud", aud);
        self
    }
    pub fn expiration(mut self, exp: &'a str) -> Self {
        self.claims.insert("exp", exp);
        self
    }
    pub fn issued_at(mut self, iat: &'a str) -> Self {
        self.claims.insert("iat", iat);
        self
    }
    pub fn sign(self, secret: &str) -> Result<JWT> {
        let rsa = Rsa::private_key_from_pem(secret.replace("\\n", "\n").as_bytes())?;

        let pkey = PKeyWithDigest {
            digest: MessageDigest::sha256(),
            key: PKey::from_rsa(rsa)?,
        };

        let header = Header {
            algorithm: AlgorithmType::Rs256,
            key_id: None,
            type_: Some(HeaderType::JsonWebToken),
            content_type: None,
        };

        let jwt = JWT(Token::new(header, self.claims)
            .sign_with_key(&pkey)?
            .as_str()
            .to_string());

        //println!("DEBUG jwt: {}", jwt.0);

        Ok(jwt)
    }
}

#[test]
fn test_email_client() {
    use crate::primitives::unix_time;
    use tokio::runtime::Runtime;

    let mut rt = Runtime::new().unwrap();
    rt.block_on(async move {
        let config = crate::open_config().unwrap();

        let jwt = JWTBuilder::new()
            .issuer(&config.google_issuer)
            .scope(&config.google_scope)
            .sub(&config.google_email)
            .audience("https://oauth2.googleapis.com/token")
            .expiration(&(unix_time() + 3_000).to_string()) // + 50 min
            .issued_at(&unix_time().to_string())
            .sign(&config.google_private_key)
            .unwrap();

        let mut client = ClientBuilder::new()
            .jwt(jwt)
            .user_id(config.google_email)
            .token_url("https://oauth2.googleapis.com/token".to_string())
            .build()
            .unwrap();

        client.token_request().await.unwrap();

        /*
        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/{}/messages",
            config.google_email
        );

        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/{userId}/messages/{id}",
            userId = config.google_email,
            id = "174d4b1765d632b1",
        );

        let res = client.get_request(&url).await.unwrap();
        */

        let res = client
            .request_message(&EmailId::from("174d4b1765d632b1"))
            .await
            .unwrap();
    });
}

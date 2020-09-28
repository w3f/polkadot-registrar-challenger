use crate::db::Database2;
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
use std::convert::{TryFrom, TryInto};
use std::result::Result as StdResult;
use tokio::time::{self, Duration};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone)]
struct Sender(String);

impl TryFrom<String> for Sender {
    type Error = ClientError;

    fn try_from(val: String) -> StdResult<Self, ClientError> {
        if val.contains("\u{003c}") {
            let parts = val.split("\u{003c}");
            if let Some(email) = parts.into_iter().nth(1) {
                Ok(Sender(email.replace("\u{003e}", "")))
            } else {
                Err(ClientError::UnrecognizedData)
            }
        } else {
            Ok(Sender(val))
        }
    }
}

impl TryFrom<&str> for Sender {
    type Error = ClientError;

    fn try_from(val: &str) -> StdResult<Self, ClientError> {
        <Sender as TryFrom<String>>::try_from(val.to_string())
    }
}

#[derive(Debug, Clone)]
struct ReceivedMessageContext {
    sender: Sender,
    body: String,
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
    db: Database2,
    jwt: Option<JWT>,
    token_url: Option<String>,
    user_id: Option<String>,
}

impl ClientBuilder {
    pub fn new(db: Database2) -> Self {
        ClientBuilder {
            db: db,
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
            db: self.db,
            jwt: self.jwt.ok_or(ClientError::IncompleteBuilder)?,
            token_url: self.token_url.ok_or(ClientError::IncompleteBuilder)?,
            token_id: None,
            user_id: self.user_id.ok_or(ClientError::IncompleteBuilder)?,
        })
    }
}

pub struct Client {
    client: ReqClient,
    db: Database2,
    jwt: JWT,
    token_url: String,
    token_id: Option<String>,
    user_id: String,
}

use reqwest::header::{self, HeaderValue};
use reqwest::Client as ReqClient;

impl Client {
    async fn token_request(&mut self) -> Result<()> {
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
    async fn get_request<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
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
    async fn request_inbox(&self) -> Result<Vec<EmailId>> {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiInbox {
            messages: Vec<ApiInboxEntry>,
            next_page_token: String,
            result_size_estimate: i64,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiInboxEntry {
            id: EmailId,
            thread_id: String,
        }

        Ok(self
            .get_request::<ApiInbox>(&format!(
                "https://gmail.googleapis.com/gmail/v1/users/{userId}/messages",
                userId = self.user_id,
            ))
            .await?
            .messages
            .into_iter()
            .map(|entry| entry.id)
            .collect())
    }
    async fn request_message(&self, email_id: &EmailId) -> Result<Vec<ReceivedMessageContext>> {
        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiMessage {
            id: String,
            thread_id: String,
            payload: ApiPayload,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiPayload {
            headers: Vec<ApiHeader>,
            parts: Vec<ApiPart>,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct ApiHeader {
            name: String,
            value: String,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ApiPart {
            part_id: String,
            mime_type: String,
            filename: String,
            body: ApiBody,
        }

        #[derive(Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct ApiBody {
            size: i64,
            data: String,
        }

        let response = self
            .get_request::<ApiMessage>(&format!(
                "https://gmail.googleapis.com/gmail/v1/users/{userId}/messages/{id}",
                userId = self.user_id,
                id = email_id.as_str(),
            ))
            .await?;

        let mut messages = vec![];
        let sender: Sender = response
            .payload
            .headers
            .iter()
            .find(|header| header.name == "From")
            .ok_or(ClientError::IncompleteBuilder)?
            .value
            .clone()
            .try_into()?;

        for part in response.payload.parts {
            messages.push(ReceivedMessageContext {
                sender: sender.clone(),
                body: String::from_utf8_lossy(
                    &base64::decode_config(part.body.data, base64::URL_SAFE)
                        .map_err(|_| ClientError::UnrecognizedData)?,
                )
                .lines()
                .nth(0)
                .ok_or(ClientError::UnrecognizedData)?
                .to_string(),
            });
        }

        Ok(messages)
    }
    pub async fn start(self) {
        let mut interval = time::interval(Duration::from_secs(5));

        loop {
            interval.tick().await;
        }
    }
    async fn handle_incoming_messages(&self) -> Result<()> {
        let messages = self.request_inbox().await?;
        let unknown_email_ids = self.db.find_untracked_emails(&messages).await?;

        let mut interval = time::interval(Duration::from_secs(1));
        for email_id in unknown_email_ids {
            interval.tick().await;

            let res = self.request_message(email_id).await?;
        }

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
    use crate::primitives::{unix_time, Challenge};
    use crate::Database2;
    use tokio::runtime::Runtime;

    // Generate a random db path
    fn db_path() -> String {
        format!("/tmp/sqlite_{}", Challenge::gen_random().as_str())
    }

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

        let mut client = ClientBuilder::new(Database2::new(&db_path()).unwrap())
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

        //let res = client.request_inbox().await.unwrap();

        let res = client
            .request_message(&EmailId::from("174d4b1765d632b1"))
            .await
            .unwrap();

        println!("RES: {:?}", res);
    });
}

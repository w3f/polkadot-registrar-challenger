use crate::comms::{CommsMessage, CommsVerifier};
use crate::db::Database2;
use crate::primitives::{unix_time, Account, AccountType, ChallengeStatus, NetAccount, Result};
use crate::verifier::Verifier2;
use jwt::algorithm::openssl::PKeyWithDigest;
use jwt::algorithm::AlgorithmType;
use jwt::header::{Header, HeaderType};
use jwt::{SignWithKey, Token};
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use reqwest::header::{self, HeaderValue};
use reqwest::Client as ReqClient;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::de::DeserializeOwned;
use serde::ser::Serialize;
use std::collections::BTreeMap;
use std::convert::TryInto;
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

trait ConvertEmailInto<T> {
    type Error;

    fn convert_into(self) -> StdResult<T, Self::Error>;
}

impl ConvertEmailInto<Account> for String {
    type Error = ClientError;

    fn convert_into(self) -> StdResult<Account, Self::Error> {
        if self.contains("<") {
            let parts = self.split("<");
            if let Some(email) = parts.into_iter().nth(1) {
                Ok(Account::from(email.replace(">", "")))
            } else {
                Err(ClientError::UnrecognizedData)
            }
        } else {
            Ok(Account::from(self))
        }
    }
}

impl ConvertEmailInto<Account> for &str {
    type Error = ClientError;

    fn convert_into(self) -> StdResult<Account, Self::Error> {
        <String as ConvertEmailInto<Account>>::convert_into(self.to_string())
    }
}

#[derive(Debug, Clone)]
struct ReceivedMessageContext {
    sender: Account,
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
    comms: CommsVerifier,
    issuer: Option<String>,
    scope: Option<String>,
    subject: Option<String>,
    private_key: Option<String>,
    token_url: Option<String>,
    server: Option<String>,
    user: Option<String>,
    password: Option<String>,
}

impl ClientBuilder {
    pub fn new(db: Database2, comms: CommsVerifier) -> Self {
        ClientBuilder {
            db: db,
            comms: comms,
            issuer: None,
            scope: None,
            subject: None,
            private_key: None,
            token_url: None,
            server: None,
            user: None,
            password: None,
        }
    }
    pub fn issuer(mut self, issuer: String) -> Self {
        self.issuer = Some(issuer);
        self
    }
    pub fn scope(mut self, scope: String) -> Self {
        self.scope = Some(scope);
        self
    }
    pub fn subject(mut self, subject: String) -> Self {
        self.subject = Some(subject);
        self
    }
    pub fn private_key(mut self, private_key: String) -> Self {
        self.private_key = Some(private_key);
        self
    }
    pub fn token_url(mut self, url: String) -> Self {
        self.token_url = Some(url);
        self
    }
    pub fn email_server(mut self, server: String) -> Self {
        self.server = Some(server);
        self
    }
    pub fn email_user(mut self, user: String) -> Self {
        self.user = Some(user);
        self
    }
    pub fn email_password(mut self, password: String) -> Self {
        self.password = Some(password);
        self
    }
    pub fn build(self) -> Result<Client> {
        Ok(Client {
            client: ReqClient::new(),
            db: self.db,
            comms: self.comms,
            issuer: self.issuer.ok_or(ClientError::IncompleteBuilder)?,
            scope: self.scope.ok_or(ClientError::IncompleteBuilder)?,
            subject: self.subject.ok_or(ClientError::IncompleteBuilder)?,
            private_key: self.private_key.ok_or(ClientError::IncompleteBuilder)?,
            token_url: self.token_url.ok_or(ClientError::IncompleteBuilder)?,
            token_id: None,
            server: self.server.ok_or(ClientError::IncompleteBuilder)?,
            user: self.user.ok_or(ClientError::IncompleteBuilder)?,
            password: self.password.ok_or(ClientError::IncompleteBuilder)?,
        })
    }
}

#[derive(Clone)]
pub struct Client {
    client: ReqClient,
    db: Database2,
    comms: CommsVerifier,
    issuer: String,
    scope: String,
    subject: String,
    private_key: String,
    token_url: String,
    token_id: Option<String>,
    server: String,
    user: String,
    password: String,
}

impl Client {
    async fn token_request(&mut self) -> Result<()> {
        let jwt = JWTBuilder::new()
            .issuer(&self.issuer)
            .scope(&self.scope)
            .sub(&self.subject)
            .audience(&self.token_url)
            .expiration(&(unix_time() + 3_000).to_string()) // + 50 min
            .issued_at(&unix_time().to_string())
            .sign(&self.private_key)?;

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
                "urn:ietf:params:oauth:grant-type:jwt-bearer", jwt.0
            )))
            .build()?;

        trace!("Token request: {:?}", request);

        let response = self
            .client
            .execute(request)
            .await?
            .json::<TokenResponse>()
            .await?;

        trace!("Token response: {:?}", response);

        self.token_id = Some(response.access_token);

        Ok(())
    }
    async fn get_request<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        self.client
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
            .send()
            .await?
            .json::<T>()
            .await
            .map_err(|err| err.into())
    }
    async fn post_request<T: DeserializeOwned, B: Serialize>(
        &self,
        url: &str,
        body: &B,
    ) -> Result<T> {
        println!(">>>>> {}", serde_json::to_string(body).unwrap());
        use self::header::HeaderName;

        let res = self
            .client
            .post(url)
            .header(
                self::header::CONTENT_TYPE,
                HeaderValue::from_static("message/rfc822"),
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
            .body(serde_json::to_string(body)?)
            .build()?;

        println!("REQUEST --> {:?}", res);
        println!(
            "BODY --> {:?}",
            String::from_utf8_lossy(res.body().unwrap().as_bytes().unwrap())
        );

        let res = self.client.execute(res).await?;

        println!("**** {}", res.text().await.unwrap());

        unimplemented!();
        res.json::<T>().await.map_err(|err| err.into())
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
                userId = self.subject,
            ))
            .await?
            .messages
            .into_iter()
            .map(|entry| entry.id)
            .collect())
    }
    async fn request_message(&self, email_id: &EmailId) -> Result<Vec<ReceivedMessageContext>> {
        let response = self
            .get_request::<ApiMessage>(&format!(
                "https://gmail.googleapis.com/gmail/v1/users/{userId}/messages/{id}",
                userId = self.subject,
                id = email_id.as_str(),
            ))
            .await?;

        let mut messages = vec![];
        let sender: Account = response
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
    async fn send_message(&self, account: &Account, msg: String) -> Result<()> {
        use lettre::Transport;
        use lettre::smtp::{SmtpClient, ClientSecurity};
        use lettre::smtp::authentication::Credentials;
        use lettre_email::EmailBuilder;

        let email = EmailBuilder::new()
            // Addresses can be specified by the tuple (email, alias)
            .to(account.as_str())
            .from(self.user.as_str())
            .subject("W3F Registrar Verification Service")
            .text(msg)
            .build()
            .unwrap();

        // TODO: Can cloning/to_string be avoided here?
        let mut transport = SmtpClient::new_simple(&self.server)?
            .credentials(Credentials::new(self.user.to_string(), self.password.to_string()))
            .transport();

        let x = transport.send(email.into())?;

        Ok(())
    }
    pub async fn start(mut self) {
        self.token_request()
            .await
            .map_err(|err| {
                error!("{}", err);
                std::process::exit(1);
            })
            .unwrap();

        self.start_responder().await;

        loop {
            let _ = self.local().await.map_err(|err| {
                error!("{}", err);
                err
            });
        }
    }
    async fn local(&self) -> Result<()> {
        use CommsMessage::*;

        match self.comms.recv().await {
            AccountToVerify {
                net_account,
                account,
            } => {
                self.handle_account_verification(net_account, account)
                    .await?
            }
            _ => {}
        }

        Ok(())
    }
    async fn start_responder(&self) {
        let c_self = self.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(3));

            loop {
                interval.tick().await;

                let _ = c_self.handle_incoming_messages().await.map_err(|err| {
                    error!("{}", err);
                    err
                });
            }
        });
    }
    async fn handle_account_verification(
        &self,
        _net_account: NetAccount,
        account: Account,
    ) -> Result<()> {
        let challenge_data = self
            .db
            .select_challenge_data(&account, &AccountType::Email)
            .await?;

        // Only require the verifier to send the initial message
        let verifier = Verifier2::new(&challenge_data);
        self.send_message(&account, verifier.init_message_builder())
            .await?;

        Ok(())
    }
    async fn handle_incoming_messages(&self) -> Result<()> {
        let inbox = self.request_inbox().await?;
        let unknown_email_ids = self.db.find_untracked_email_ids(&inbox).await?;

        let mut interval = time::interval(Duration::from_secs(1));
        for email_id in unknown_email_ids {
            interval.tick().await;

            // Request information/body about the Email ID.
            let user_messages = self.request_message(email_id).await?;
            if user_messages.is_empty() {
                warn!("Received an empty message. Ignoring.");
                self.db.track_email_id(email_id).await?;
                continue;
            }

            let sender = &user_messages.get(1).as_ref().unwrap().sender;

            let challenge_data = self
                .db
                .select_challenge_data(sender, &AccountType::Email)
                .await?;

            if challenge_data.is_empty() {
                warn!(
                    "Received message from an unknown email address: {}. Ignoring.",
                    sender.as_str()
                );

                self.db.track_email_id(email_id).await?;
                continue;
            }

            let mut verifier = Verifier2::new(&challenge_data);

            for message in &user_messages {
                verifier.verify(&message.body);
            }

            for network_address in verifier.valid_verifications() {
                debug!(
                    "Valid verification for address: {}",
                    network_address.address().as_str()
                );

                self.comms
                    .notify_status_change(network_address.address().clone());

                self.db
                    .set_challenge_status(
                        network_address.address(),
                        &AccountType::Twitter,
                        ChallengeStatus::Accepted,
                    )
                    .await?;
            }

            for network_address in verifier.invalid_verifications() {
                debug!(
                    "Invalid verification for address: {}",
                    network_address.address().as_str()
                );

                self.db
                    .set_challenge_status(
                        network_address.address(),
                        &AccountType::Twitter,
                        ChallengeStatus::Rejected,
                    )
                    .await?;
            }

            self.send_message(sender, verifier.response_message_builder())
                .await?;
        }

        Ok(())
    }
}

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

        let mut client =
            ClientBuilder::new(Database2::new(&db_path()).unwrap(), CommsVerifier::new())
                .issuer(config.google_issuer)
                .scope(config.google_scope)
                .subject(config.google_email)
                .private_key(config.google_private_key)
                .email_server(config.email_server)
                .email_user(config.email_user)
                .email_password(config.email_password)
                .token_url("https://oauth2.googleapis.com/token".to_string())
                .build()
                .unwrap();

        client.token_request().await.unwrap();

        client
            .send_message(
                &Account::from("fabio.lama@pm.me"),
                "Hello there".to_string(),
            )
            .await
            .unwrap();
    });
}

use crate::comms::{CommsMessage, CommsVerifier};
use crate::db::Database2;
use crate::primitives::{unix_time, Account, Challenge, NetAccount, Result};
use reqwest::header::{self, HeaderValue};
use reqwest::{Client, Request};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Value, ValueRef};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::convert::{TryFrom, TryInto};
use std::result::Result as StdResult;
use tokio::time::{self, Duration};

#[derive(Debug, Clone, Deserialize)]
pub struct TwitterId(u64);

impl TwitterId {
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for TwitterId {
    fn from(val: u64) -> Self {
        TwitterId(val)
    }
}

impl TryFrom<String> for TwitterId {
    type Error = TwitterError;

    fn try_from(val: String) -> StdResult<Self, Self::Error> {
        Ok(TwitterId(
            val.parse::<u64>()
                .map_err(|err| TwitterError::UnrecognizedData)?,
        ))
    }
}

impl ToSql for TwitterId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Owned(Value::Integer(self.0 as i64)))
    }
}

impl FromSql for TwitterId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Integer(val) => Ok(TwitterId(val as u64)),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

#[derive(Debug, Fail)]
pub enum TwitterError {
    #[fail(display = "The builder was not used correctly")]
    IncompleteBuilder,
    #[fail(display = "Unrecognized data returned from the Twitter API")]
    UnrecognizedData,
    #[fail(display = "Error from Twitter API: {:?}", 0)]
    ApiCode(TwitterApiError),
    #[fail(display = "HTTP error: {}", 0)]
    Http(failure::Error),
    #[fail(display = "Failed to (de-)serialize JSON data: {}", 0)]
    Serde(failure::Error),
    #[fail(display = "Failed to build request: {}", 0)]
    RequestBuilder(failure::Error),
}

#[derive(Debug, Clone, Deserialize)]
pub struct TwitterApiError {
    errors: Vec<ApiErrorObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorObject {
    code: i64,
    message: String,
}

pub struct TwitterBuilder {
    consumer_key: Option<String>,
    consumer_secret: Option<String>,
    sig_method: Option<String>,
    token: Option<String>,
    token_secret: Option<String>,
    version: Option<f64>,
    db: Database2,
    comms: CommsVerifier,
}

impl TwitterBuilder {
    pub fn new(db: Database2, comms: CommsVerifier) -> Self {
        TwitterBuilder {
            consumer_key: None,
            consumer_secret: None,
            sig_method: None,
            token: None,
            token_secret: None,
            version: None,
            db: db,
            comms: comms,
        }
    }
    pub fn consumer_key(mut self, key: String) -> Self {
        self.consumer_key = Some(key);
        self
    }
    pub fn consumer_secret(mut self, key: String) -> Self {
        self.consumer_secret = Some(key);
        self
    }
    pub fn sig_method(mut self, method: String) -> Self {
        self.sig_method = Some(method);
        self
    }
    pub fn token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }
    pub fn token_secret(mut self, secret: String) -> Self {
        self.token_secret = Some(secret);
        self
    }
    pub fn version(mut self, version: f64) -> Self {
        self.version = Some(version);
        self
    }
    pub fn build(self) -> Result<Twitter> {
        Ok(Twitter {
            client: Client::new(),
            consumer_key: self.consumer_key.ok_or(TwitterError::IncompleteBuilder)?,
            consumer_secret: self
                .consumer_secret
                .ok_or(TwitterError::IncompleteBuilder)?,
            sig_method: self.sig_method.ok_or(TwitterError::IncompleteBuilder)?,
            token: self.token.ok_or(TwitterError::IncompleteBuilder)?,
            token_secret: self.token_secret.ok_or(TwitterError::IncompleteBuilder)?,
            version: self.version.ok_or(TwitterError::IncompleteBuilder)?,
            db: self.db,
            comms: self.comms,
        })
    }
}

pub struct Twitter {
    client: Client,
    consumer_key: String,
    consumer_secret: String,
    sig_method: String,
    token: String,
    token_secret: String,
    version: f64,
    db: Database2,
    comms: CommsVerifier,
}

use hmac::{Hmac, Mac, NewMac};
use sha1::Sha1;

enum HttpMethod {
    POST,
    GET,
}

impl HttpMethod {
    fn as_str(&self) -> &'static str {
        use HttpMethod::*;

        match self {
            POST => "POST",
            GET => "GET",
        }
    }
}

impl Twitter {
    /// Creates a signature as documented here:
    /// https://developer.twitter.com/en/docs/authentication/oauth-1-0a/creating-a-signature
    fn authenticate_request(
        &self,
        method: &HttpMethod,
        url: &str,
        request: &mut Request,
        params: Option<&[(&str, &str)]>,
    ) -> Result<()> {
        use urlencoding::encode;

        let challenge = Challenge::gen_random();
        let timestamp = unix_time().to_string();
        let version = format!("{:.1}", self.version);

        let mut fields = vec![
            ("oauth_consumer_key", self.consumer_key.as_str()),
            ("oauth_nonce", challenge.as_str()),
            ("oauth_signature_method", self.sig_method.as_str()),
            ("oauth_timestamp", &timestamp),
            ("oauth_token", self.token.as_str()),
            ("oauth_version", &version),
        ];

        if let Some(params) = params {
            fields.append(&mut params.to_vec());
        }

        fields.sort_by(|(a, _), (b, _)| a.cmp(b));

        let mut params = String::new();
        for (name, val) in &fields {
            params.push_str(&format!("{}={}&", encode(name), encode(val)));
        }

        // Remove the trailing `&`.
        params.pop();

        let base = format!("{}&{}&{}", method.as_str(), encode(url), encode(&params));

        // Sign the base string.
        let sign_key = format!(
            "{}&{}",
            encode(&self.consumer_secret),
            encode(&self.token_secret)
        );

        let mut mac: Hmac<Sha1> = Hmac::new_varkey(sign_key.as_bytes()).unwrap();
        mac.update(base.as_bytes());

        // Create the resulting hash.
        let sig = base64::encode(mac.finalize().into_bytes());

        // Insert the signature;
        fields.push(("oauth_signature", &sig));
        fields.sort_by(|(a, _), (b, _)| a.cmp(b));

        let mut oauth_header = String::new();
        oauth_header.push_str("OAuth ");

        for (name, val) in &fields {
            oauth_header.push_str(&format!("{}={}, ", encode(name), encode(val)))
        }

        // Remove the trailing `, `.
        oauth_header.pop();
        oauth_header.pop();

        // Inserth the authentication header into the request.
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, HeaderValue::from_str(&oauth_header)?);

        Ok(())
    }
    pub async fn get_request<T: DeserializeOwned>(
        &self,
        url: &str,
        params: Option<&[(&str, &str)]>,
    ) -> StdResult<T, TwitterError> {
        let mut full_url = String::from(url);

        if let Some(params) = params {
            full_url.push('?');
            for (key, val) in params {
                full_url.push_str(&format!("{}={}&", key, val));
            }

            // Remove trailing `&` or `?` in case "params" is empty.
            full_url.pop();
        }

        let mut request = self
            .client
            .get(&full_url)
            .build()
            .map_err(|err| TwitterError::RequestBuilder(err.into()))?;

        self.authenticate_request(&HttpMethod::GET, url, &mut request, params)
            .map_err(|err| TwitterError::RequestBuilder(err.into()))?;

        let resp = self
            .client
            .execute(request)
            .await
            .map_err(|err| TwitterError::Http(err.into()))?;

        let txt = resp
            .text()
            .await
            .map_err(|_| TwitterError::UnrecognizedData)?;

        println!("RESP>> {}", txt);

        serde_json::from_str::<T>(&txt).map_err(|err| {
            if let Ok(api_err) = serde_json::from_str::<TwitterApiError>(&txt) {
                TwitterError::ApiCode(api_err)
            } else {
                TwitterError::Serde(err.into())
            }
        })
    }
    pub async fn post_request<T: DeserializeOwned, B: Serialize>(
        &self,
        url: &str,
        body: B,
    ) -> StdResult<T, TwitterError> {
        let mut request = self
            .client
            .post(url)
            .body(
                serde_json::to_string(&body)
                    .map(|s| {
                        println!("TO SEND: {}", s);
                        s
                    })
                    .map_err(|err| TwitterError::Serde(err.into()))?,
            )
            .build()
            .map_err(|err| TwitterError::RequestBuilder(err.into()))?;

        self.authenticate_request(&HttpMethod::POST, url, &mut request, None)
            .map_err(|err| TwitterError::RequestBuilder(err.into()))?;

        let resp = self
            .client
            .execute(request)
            .await
            .map_err(|err| TwitterError::Http(err.into()))?;

        let txt = resp
            .text()
            .await
            .map_err(|err| TwitterError::UnrecognizedData)?;

        println!("RESP>> {}", txt);

        serde_json::from_str::<T>(&txt).map_err(|err| {
            if let Ok(api_err) = serde_json::from_str::<TwitterApiError>(&txt) {
                TwitterError::ApiCode(api_err)
            } else {
                TwitterError::Serde(err.into())
            }
        })
    }
    async fn request_messages(
        &self,
        exclude_me: &TwitterId,
    ) -> Result<Vec<ReceivedMessageContext>> {
        self.get_request::<ApiMessageEvent>(
            "https://api.twitter.com/1.1/direct_messages/events/list.json",
            None,
        )
        .await?
        .get_messages(exclude_me)
    }
    async fn lookup_twitter_id(
        &self,
        twitter_ids: Option<&[&TwitterId]>,
        accounts: Option<&[&Account]>,
    ) -> Result<Vec<(Account, TwitterId)>> {
        let mut params = vec![];

        // Lookups for UserIds
        let mut lookup = String::new();
        if let Some(twitter_ids) = twitter_ids {
            for twitter_id in twitter_ids {
                lookup.push_str(&twitter_id.as_u64().to_string());
                lookup.push(',');
            }

            // Remove trailing `,`.
            lookup.pop();

            params.push(("user_id", lookup.as_str()))
        }

        // Lookups for Accounts
        let mut lookup = String::new();
        if let Some(accounts) = accounts {
            for account in accounts {
                lookup.push_str(&account.as_str().replace("@", ""));
                lookup.push(',');
            }

            // Remove trailing `,`.
            lookup.pop();

            params.push(("screen_name", lookup.as_str()))
        }

        #[derive(Deserialize)]
        // Only `screen_name` required.
        struct UserObject {
            id: TwitterId,
            screen_name: String,
        }

        let user_objects = self
            .get_request::<Vec<UserObject>>(
                "https://api.twitter.com/1.1/users/lookup.json",
                Some(&params),
            )
            .await?;

        if user_objects.is_empty() {
            return Err(TwitterError::UnrecognizedData.into());
        }

        Ok(user_objects
            .into_iter()
            .map(|obj| (Account::from(format!("@{}", obj.screen_name)), obj.id))
            .collect())
    }
    pub async fn send_message(&self, id: &TwitterId, msg: String) -> StdResult<(), TwitterError> {
        self.post_request::<(), _>(
            "https://api.twitter.com/1.1/direct_messages/events/new.json",
            ApiMessageEvent::new(id, msg),
        )
        .await
        .map(|_| ())
    }
    pub async fn start(self) {
        let mut interval = time::interval(Duration::from_secs(3));
        loop {
            interval.tick().await;

            let _ = self.local().await.map_err(|err| {
                error!("{}", err);
            });
        }
    }
    pub async fn local(&self) -> Result<()> {
        use CommsMessage::*;

        match self.comms.recv().await {
            AccountToVerify {
                net_account,
                account,
            } => {
                self.handle_account_verification(net_account, account)
                    .await?
            }
            _ => panic!(),
        }

        Ok(())
    }
    pub async fn handle_account_verification(
        &self,
        net_account: NetAccount,
        account: Account,
    ) -> Result<()> {
        let twitter_id = if let Some(twitter_id) = self.db.select_twitter_id(&account).await? {
            twitter_id
        } else {
            self.lookup_twitter_id(None, Some(&[&account]))
                .await?
                .remove(0)
                .1
        };

        Ok(())
    }
    pub async fn handle_incoming_messages(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
struct ApiMessageEvent {
    event: Option<ApiEvent>,
    events: Option<Vec<ApiEvent>>,
}

#[derive(Deserialize, Serialize)]
struct ApiEvent {
    #[serde(rename = "type")]
    t_type: String,
    message_create: ApiMessageCreate,
}

#[derive(Deserialize, Serialize)]
struct ApiMessageCreate {
    target: ApiTarget,
    sender_id: Option<String>,
    message_data: ApiMessageData,
}

#[derive(Deserialize, Serialize)]
struct ApiTarget {
    recipient_id: String,
}

#[derive(Deserialize, Serialize)]
struct ApiMessageData {
    text: String,
}

struct ReceivedMessageContext {
    recipient: TwitterId,
    msg: String,
}

impl ApiMessageEvent {
    fn new(recipient: &TwitterId, msg: String) -> Self {
        ApiMessageEvent {
            event: Some(ApiEvent {
                t_type: "message_create".to_string(),
                message_create: ApiMessageCreate {
                    target: ApiTarget {
                        recipient_id: recipient.as_u64().to_string(),
                    },
                    sender_id: None,
                    message_data: ApiMessageData { text: msg },
                },
            }),
            events: None,
        }
    }
    fn get_messages(self, exclude_me: &TwitterId) -> Result<Vec<ReceivedMessageContext>> {
        let mut msgs = vec![];

        if let Some(events) = self.events {
            for event in events {
                msgs.push(ReceivedMessageContext {
                    recipient: event
                        .message_create
                        .sender_id
                        .ok_or(TwitterError::UnrecognizedData)?
                        .try_into()?,
                    msg: event.message_create.message_data.text,
                });
            }
        }

        Ok(msgs)
    }
}

#[test]
fn test_twitter() {
    use crate::primitives::{Challenge, NetAccount};
    use crate::Database2;
    use tokio::runtime::Runtime;

    // Generate a random db path
    fn db_path() -> String {
        format!("/tmp/sqlite_{}", Challenge::gen_random().as_str())
    }

    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let config = crate::open_config().unwrap();

        let client = TwitterBuilder::new(Database2::new(&db_path()).unwrap(), CommsVerifier::new())
            .consumer_key(config.twitter_api_key)
            .consumer_secret(config.twitter_api_secret)
            .sig_method("HMAC-SHA1".to_string())
            .token(config.twitter_token)
            .token_secret(config.twitter_token_secret)
            .version(1.0)
            .build()
            .unwrap();

        #[derive(Debug, Serialize, Deserialize)]
        struct Root {
            next_cursor: Option<String>,
            events: Vec<ApiEvent>,
        }

        #[derive(Debug, Serialize, Deserialize)]
        struct ApiEvent {
            id: Option<String>,
            created_timestamp: Option<String>,
            message_create: ApiMessageCreate,
        }

        #[derive(Debug, Serialize, Deserialize)]
        struct ApiMessageCreate {
            sender_id: String,
            message_data: ApiMessageData,
        }

        #[derive(Debug, Serialize, Deserialize)]
        struct ApiMessageData {
            text: String,
        }

        let res = client.request_messages().await.unwrap();

        /*
        let res = client
            .lookup_twitter_id(
                Some(&[&TwitterId::from(102128843)]),
                Some(&[&Account::from("@JohnDop88908274")]),
            )
            .await
            .unwrap();

        println!("ACCOUNTS: {:?}", res);

        let res = client
            .send_message(
                &TwitterId::from(1309954318712426496),
                String::from("Hello there, this is a test"),
            )
            .await
            .unwrap();

        client
            .get_request::<Root>(
                "https://api.twitter.com/1.1/direct_messages/events/list.json",
                None,
            )
            .await
            .unwrap();
            */
    });
}

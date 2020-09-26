use crate::primitives::{unix_time, Challenge, Result, Account};
use reqwest::header::{self, HeaderValue};
use reqwest::{Client, Request};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Value, ValueRef};
use serde::de::DeserializeOwned;
use tokio::time::{self, Duration};

#[derive(Debug, Clone)]
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
    #[fail(display = "the builder was not used correctly")]
    IncompleteBuilder,
}

pub struct TwitterBuilder {
    consumer_key: Option<String>,
    consumer_secret: Option<String>,
    sig_method: Option<String>,
    token: Option<String>,
    token_secret: Option<String>,
    version: Option<f64>,
}

impl TwitterBuilder {
    pub fn new() -> Self {
        TwitterBuilder {
            consumer_key: None,
            consumer_secret: None,
            sig_method: None,
            token: None,
            token_secret: None,
            version: None,
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
}

use hmac::{Hmac, Mac, NewMac};
use sha1::Sha1;

enum HttpMethod {
    //POST,
    GET,
}

impl HttpMethod {
    fn as_str(&self) -> &'static str {
        use HttpMethod::*;

        match self {
            //POST => "POST",
            GET => "GET",
        }
    }
}

impl Twitter {
    fn create_request(&self, _method: HttpMethod, url: &str, params: Option<&[(&str, &str)]>) -> Result<Request> {
        let mut full_url = String::from(url);

        if let Some(params) = params {
            full_url.push('?');

            for (key, val) in params {
                full_url.push_str(&format!("{}={}&", key, val));
            }

            // Remove trailing `&` or `?` in case "params" is empty.
            full_url.pop();
        }

        Ok(self.client.get(&full_url).build()?)
    }
    /// Creates a signature as documented here:
    /// https://developer.twitter.com/en/docs/authentication/oauth-1-0a/creating-a-signature
    fn authenticate_request(
        &self,
        method: HttpMethod,
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
    pub async fn get_request<T: DeserializeOwned>(&self, url: &str, params: Option<&[(&str, &str)]>) -> Result<T> {
        let mut request = self.create_request(HttpMethod::GET, url, params)?;
        self.authenticate_request(HttpMethod::GET, url, &mut request, params)?;
        let resp = self.client.execute(request).await?;
        let txt = resp.text().await?;

        println!("RESP>> {}", txt);

        Ok(serde_json::from_str(&txt)?)
    }
    pub async fn lookup_users(&self, twitter_id: TwitterId) -> Result<Account> {
        unimplemented!()
    }
    pub async fn send_message(&self, account: &Account, msg: &str) -> Result<()> {
        Ok(())
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
        Ok(())
    }
}

#[test]
fn test_twitter() {
    use tokio::runtime::Runtime;

    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let config = crate::open_config().unwrap();

        let client = TwitterBuilder::new()
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
            events: Vec<Event>,
        }

        #[derive(Debug, Serialize, Deserialize)]
        struct Event {
            id: Option<String>,
            created_timestamp: Option<String>,
            message_create: MessageCreate,
        }

        #[derive(Debug, Serialize, Deserialize)]
        struct MessageCreate {
            message_data: MessageData,
        }

        #[derive(Debug, Serialize, Deserialize)]
        struct MessageData {
            text: String,
        }

        client
            .get_request::<Root>("https://api.twitter.com/1.1/direct_messages/events/list.json", None)
            .await
            .unwrap();
    });
}

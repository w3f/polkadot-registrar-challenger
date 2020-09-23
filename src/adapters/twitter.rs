use crate::primitives::{unix_time, Challenge, Result};
use reqwest::header::{self, HeaderName, HeaderValue};
use reqwest::{Client, Request};
use serde::de::DeserializeOwned;
use tokio::time::{self, Duration};

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
    fn create_request(&self, _method: HttpMethod, url: &str) -> Result<Request> {
        Ok(self.client.get(url).build()?)
    }
    /// Creates a signature as documented here:
    /// https://developer.twitter.com/en/docs/authentication/oauth-1-0a/creating-a-signature
    fn create_sig(
        &self,
        method: HttpMethod,
        url: &str,
        request: &mut Request,
        body: Option<&[(&str, &str)]>,
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

        if let Some(body) = body {
            fields.append(&mut body.to_vec());
        }

        fields.sort_by(|(a, _), (b, _)| a.cmp(b));

        let mut params = String::new();
        for (name, val) in &fields {
            params.push_str(&format!("{}={}&", encode(name), encode(val)));
        }

        // Remove the trailing `&`.
        params.pop();

        let base = format!("{}&{}&{}", method.as_str(), encode(url), encode(&params));
        println!("BASE: {}", base);

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

        request.headers_mut().insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&oauth_header)?,
        );

        Ok(())
    }
    pub async fn get_request<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let mut request = self.create_request(HttpMethod::GET, url)?;
        self.create_sig(HttpMethod::GET, url, &mut request, None)?;

        println!("REQUEST: {:?}", request);
        let s = self.client.execute(request).await?.text().await.unwrap();
        println!("RESP: {}", s);

        panic!()
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

        client
            .get_request::<u32>(
                "https://api.twitter.com/1.1/direct_messages/welcome_messages/list.json",
            )
            .await
            .unwrap();
    });
}

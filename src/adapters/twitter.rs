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
        Ok(self
            .client
            .get(url)
            .header(header::AUTHORIZATION, HeaderValue::from_str("OAuth")?)
            .header(self::header::CONTENT_LENGTH, HeaderValue::from_static("0"))
            .header(
                HeaderName::from_static("oauth_consumer_key"),
                HeaderValue::from_str(&self.consumer_key)?,
            )
            .header(
                HeaderName::from_static("oauth_nonce"),
                // The nonce can be anything random, so just re-use existing
                // functionality here.
                HeaderValue::from_str(Challenge::gen_random().as_str())?,
            )
            .header(
                HeaderName::from_static("oauth_signature_method"),
                HeaderValue::from_str(&self.sig_method)?,
            )
            .header(
                HeaderName::from_static("oauth_timestamp"),
                HeaderValue::from_str(&unix_time().to_string())?,
            )
            .header(
                HeaderName::from_static("oauth_token"),
                HeaderValue::from_str(&self.token)?,
            )
            .header(
                HeaderName::from_static("oauth_version"),
                HeaderValue::from_str(&format!("{:.1}", self.version))?,
            )
            .build()?)
    }
    /// Creates a signature as documented here:
    /// https://developer.twitter.com/en/docs/authentication/oauth-1-0a/creating-a-signature
    fn create_sig(
        &self,
        method: HttpMethod,
        url: &str,
        request: &Request,
        body: Option<&[(&str, &str)]>,
    ) -> Result<String> {
        use urlencoding::encode;

        let mut fields: Vec<(&str, &str)> = request
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
            .filter(|(k, _)| k.starts_with("oauth_"))
            .chain(body.unwrap_or(&[]).iter().map(|&s| s))
            .collect();

        fields.sort_by(|(a, _), (b, _)| a.cmp(b));

        for f in &fields {
            println!("{} => {}", f.0, f.1);
        }

        let mut params = String::new();
        for (name, val) in fields {
            params.push_str(&format!("{}={}&", encode(name), encode(val)));
        }

        // Remove the trailing '&'.
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
        Ok(sig)
    }
    pub async fn get_request<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let mut request = self.create_request(HttpMethod::GET, url)?;
        let sig = self.create_sig(HttpMethod::GET, url, &request, None)?;

        request.headers_mut().insert(
            HeaderName::from_static("auth_sig"),
            HeaderValue::from_str(&sig)?,
        );

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

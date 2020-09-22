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
    sig_method: Option<String>,
    token: Option<String>,
    version: Option<f64>,
}

impl TwitterBuilder {
    pub fn new() -> Self {
        TwitterBuilder {
            consumer_key: None,
            sig_method: None,
            token: None,
            version: None,
        }
    }
    pub fn consumer_key(mut self, key: String) -> Self {
        self.consumer_key = Some(key);
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
    pub fn version(mut self, version: f64) -> Self {
        self.version = Some(version);
        self
    }
    pub fn build(self) -> Result<Twitter> {
        Ok(Twitter {
            client: Client::new(),
            consumer_key: self.consumer_key.ok_or(TwitterError::IncompleteBuilder)?,
            sig_method: self.sig_method.ok_or(TwitterError::IncompleteBuilder)?,
            token: self.token.ok_or(TwitterError::IncompleteBuilder)?,
            version: self.version.ok_or(TwitterError::IncompleteBuilder)?,
        })
    }
}

pub struct Twitter {
    client: Client,
    consumer_key: String,
    sig_method: String,
    token: String,
    version: f64,
}

fn sign(mut request: Request) {
    let headers = request.headers_mut();
    let mut h: Vec<(&HeaderName, &HeaderValue)> = headers.iter().collect();
    h.sort_by(|(&a, _), (&b, _)| a.partial_cmp(b).unwrap());
}

impl Twitter {
    pub async fn request<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let request = self
            .client
            .get(url)
            .header(header::AUTHORIZATION, HeaderValue::from_str("OAuth")?)
            .header(
                self::header::CONTENT_TYPE,
                HeaderValue::from_static("application/x-www-form-urlencoded"),
            )
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
                HeaderName::from_static("oauth_signature"),
                HeaderValue::from_str("")?,
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
                HeaderValue::from_str(&self.version.to_string())?,
            )
            .build()?;

        Ok(self.client.execute(request).await?.json::<T>().await?)
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

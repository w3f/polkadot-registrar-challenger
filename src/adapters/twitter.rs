use crate::primitives::Result;
use reqwest::Client;
use serde::de::DeserializeOwned;
use tokio::time::{self, Duration};

pub struct Twitter {
    client: Client,
    username: String,
    password: String,
}

impl Twitter {
    pub fn new(username: String, password: String) -> Self {
        Twitter {
            client: Client::new(),
            username: username,
            password: password,
        }
    }
    pub async fn request<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        Ok(self
            .client
            .get(url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?
            .json::<T>()
            .await?)
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

use crate::primitives::Result;
use reqwest::Client;
use serde::de::DeserializeOwned;

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
        loop {
            let _ = self.local().await.map_err(|err| {
                error!("{}", err);
            });
        }
    }
    pub async fn local(&self) -> Result<()> {
        Ok(())
    }
}

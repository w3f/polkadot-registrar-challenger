use crate::primitives::Result;
use jwt::algorithm::openssl::PKeyWithDigest;
use jwt::algorithm::AlgorithmType;
use jwt::header::{Header, HeaderType};
use jwt::{SignWithKey, Token};
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;

#[derive(Debug, Fail)]
enum ClientError {
    #[fail(display = "the builder was not used correctly")]
    IncompleteBuilder,
    #[fail(display = "the access token was not requested for the client")]
    MissingAccessToken,
}

pub struct ClientBuilder {
    jwt: Option<JWT>,
    token_url: Option<String>,
}

impl ClientBuilder {
    pub fn new() -> Self {
        ClientBuilder {
            jwt: None,
            token_url: None,
        }
    }
    pub fn jwt(self, jwt: JWT) -> Self {
        ClientBuilder {
            jwt: Some(jwt),
            ..self
        }
    }
    pub fn token_url(self, url: &str) -> Self {
        ClientBuilder {
            token_url: Some(url.to_string()),
            ..self
        }
    }
    pub fn build(self) -> Result<Client> {
        Ok(Client {
            client: ReqClient::new(),
            jwt: self.jwt.ok_or(ClientError::IncompleteBuilder)?,
            token_url: self.token_url.ok_or(ClientError::IncompleteBuilder)?,
            token_id: None,
        })
    }
}

pub struct Client {
    client: ReqClient,
    jwt: JWT,
    token_url: String,
    token_id: Option<String>,
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
    pub async fn get_request(&self, url: &str) -> Result<String> {
        let request = self
            .client
            .get(&url.replace(
                "{userId}",
                //"w3f-registrar-bot@w3f-registrar-bot.iam.gserviceaccount.com",
                "fabio@web3.foundation",
            ))
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

        Ok(self.client.execute(request).await?.text().await?)
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
        self.claims.insert(
            "sub",
            "w3f-registrar-bot@w3f-registrar-bot.iam.gserviceaccount.com",
        );
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
    use std::env;
    use tokio::runtime::Runtime;

    let mut rt = Runtime::new().unwrap();
    rt.block_on(async move {
        let private_key = env::var("TEST_GOOG_SEC_KEY").unwrap();
        let iss = env::var("TEST_GOOG_ISSUER").unwrap();

        let jwt = JWTBuilder::new()
            .issuer(&iss)
            .scope("https://mail.google.com https://www.googleapis.com/auth/gmail.compose https://www.googleapis.com/auth/gmail.modify https://www.googleapis.com/auth/gmail.readonly")
            .audience("https://oauth2.googleapis.com/token")
            .expiration(&(unix_time() + 3_000).to_string()) // + 50 min
            .issued_at(&unix_time().to_string())
            .sign(&private_key).unwrap();

        let mut client = ClientBuilder::new()
            .jwt(jwt)
            .token_url("https://oauth2.googleapis.com/token")
            .build()
            .unwrap();

        client.token_request().await.unwrap();

        let res = client
            .get_request("https://gmail.googleapis.com/gmail/v1/users/{userId}/messages")
            .await
            .unwrap();

        println!("RES: {}", res);
    });
}

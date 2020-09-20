use jwt::algorithm::openssl::PKeyWithDigest;
use jwt::algorithm::AlgorithmType;
use jwt::header::{Header, HeaderType};
use jwt::{SignWithKey, Token};
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use sha2::Sha256;
use std::result::Result as StdResult;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Fail)]
enum ClientError {
    #[fail(display = "the builder was not used correctly")]
    IncompleteBuilder,
}

pub struct ClientBuilder {
    client_id: Option<String>,
    jwt: Option<JWT>,
    token_url: Option<String>,
}

impl ClientBuilder {
    pub fn new() -> Self {
        ClientBuilder {
            client_id: None,
            jwt: None,
            token_url: None,
        }
    }
    pub fn client_id(self, client_id: &str) -> Self {
        ClientBuilder {
            client_id: Some(client_id.to_string()),
            ..self
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
    pub fn build(self) -> Result<Client, ClientError> {
        Ok(Client {
            client_id: self.client_id.ok_or(ClientError::IncompleteBuilder)?,
            jwt: self.jwt.ok_or(ClientError::IncompleteBuilder)?,
            token_url: self.token_url.ok_or(ClientError::IncompleteBuilder)?,
        })
    }
}

pub struct Client {
    client_id: String,
    jwt: JWT,
    token_url: String,
}

impl Client {}

use jwt::claims::RegisteredClaims;
use std::collections::BTreeMap;

pub struct JWTBuilder<'a> {
    claims: BTreeMap<&'a str, &'a str>,
}

struct JWT(String);

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
    pub fn sign(self, secret: &str) -> JWT {
        let rsa = Rsa::private_key_from_pem(secret.replace("\\n", "\n").as_bytes()).unwrap();

        let pkey = PKeyWithDigest {
            digest: MessageDigest::sha256(),
            key: PKey::from_rsa(rsa).unwrap(),
        };

        let header = Header {
            algorithm: AlgorithmType::Rs256,
            key_id: None,
            type_: Some(HeaderType::JsonWebToken),
            content_type: None,
        };

        JWT(Token::new(header, self.claims)
            .sign_with_key(&pkey)
            .unwrap()
            .as_str()
            .to_string())
    }
}

fn unix_time() -> u64 {
    let start = SystemTime::now();
    start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

#[test]
fn test_email_client() {
    use std::env;

    let client_id = env::var("TEST_GOOG_CLIENT_ID").unwrap();
    let private_key = env::var("TEST_GOOG_SEC_KEY").unwrap();
    let iss = env::var("TEST_GOOG_ISSUER").unwrap();

    let jwt = JWTBuilder::new()
        .issuer(&iss)
        .scope("https://www.googleapis.com/auth/gmail.modifye")
        .audience("https://oauth2.googleapis.com/token")
        .expiration(&(unix_time() + 3_000).to_string()) // + 50 min
        .issued_at(&unix_time().to_string())
        .sign(&private_key);

    println!(">> {}", jwt.0);

    let client = ClientBuilder::new()
        .client_id(&client_id)
        .jwt(jwt)
        .token_url("https://oauth2.googleapis.com/token")
        .build()
        .unwrap();
}

#[macro_use]
extern crate serde;

#[derive(Debug, Deserialize)]
pub struct Config {
    accounts: AccountsConfig,
}

#[derive(Debug, Deserialize)]
pub struct AccountsConfig {
    matrix: MatrixConfig,
    twitter: TwitterConfig,
    email: EmailConfig,
}

#[derive(Debug, Deserialize)]
pub struct MatrixConfig {
    pub enabled: bool,
    pub homeserver: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct TwitterConfig {
    pub enabled: bool,
    pub screen_name: String,
    pub api_key: String,
    pub api_secret: String,
    pub token: String,
    pub token_secret: String,
}

#[derive(Debug, Deserialize)]
pub struct EmailConfig {
    pub enabled: bool,
    pub stmp_server: String,
    pub imap_server: String,
    pub inbox: String,
    pub user: String,
    pub password: String,
}

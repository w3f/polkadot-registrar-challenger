#[macro_use]
extern crate serde;

#[derive(Debug, Deserialize)]
pub struct OLDConfig {
    pub registrar_db_path: String,
    pub matrix_db_path: String,
    pub log_level: log::LevelFilter,
    pub watcher_url: String,
    pub enable_watcher: bool,
    pub enable_accounts: bool,
    pub enable_health_check: bool,
    //
    pub matrix_homeserver: String,
    pub matrix_username: String,
    pub matrix_password: String,
    //
    pub twitter_screen_name: String,
    pub twitter_api_key: String,
    pub twitter_api_secret: String,
    pub twitter_token: String,
    pub twitter_token_secret: String,
    //
    pub email_server: String,
    pub imap_server: String,
    pub email_inbox: String,
    pub email_user: String,
    pub email_password: String,
}

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

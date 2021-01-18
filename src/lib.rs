#[macro_use]
extern crate serde;
#[macro_use]
extern crate actix_web;

use actix_web::dev::RequestHead;
use actix_web::guard::Guard;
use actix_web::{web, App, HttpResponse, HttpServer, Responder, Scope};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;

pub type Result<T> = std::result::Result<T, failure::Error>;

mod state_projection;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub accounts: AccountsConfig,
    pub log_level: log::LevelFilter,
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

fn open_config() -> Result<Config> {
    // Open config file.
    let content = fs::read_to_string("config.json")
        .or_else(|_| fs::read_to_string("/etc/registrar/config.json"))
        .map_err(|_| {
            eprintln!("Failed to open config at 'config.json' or '/etc/registrar/config.json'.");
            std::process::exit(1);
        })
        .unwrap();

    // Parse config file as JSON.
    let config = serde_yaml::from_str::<Config>(&content)
        .map_err(|err| {
            eprintln!("Failed to parse config: {}", err);
            std::process::exit(1);
        })
        .unwrap();

    Ok(config)
}

pub fn init_env() -> Result<Config> {
    let config = open_config()?;

    // Env variables for log level overwrites config.
    if let Ok(_) = env::var("RUST_LOG") {
        println!("Env variable 'RUST_LOG' found, overwriting logging level from config.");
        env_logger::init();
    } else {
        println!("Setting log level to '{}' from config.", config.log_level);
        env_logger::builder()
            .filter_module("registrar", config.log_level)
            .init();
    }

    println!("Logger initiated");

    Ok(config)
}

#[get("/healthcheck")]
async fn healthcheck() -> impl Responder {
    HttpResponse::Ok().body("Ok")
}

#[get("/judge_identity")]
async fn judge_identity() -> impl Responder {
    HttpResponse::Ok().body("")
}

#[get("/status")]
async fn status() -> impl Responder {
    HttpResponse::Ok().body("")
}

fn admin_scope() -> Scope {
    web::scope("/admin")
        .guard(VerifyAdmin::default())
        .service(judge_identity)
}

fn public_scope() -> Scope {
    web::scope("/public").service(status)
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct UserId;

pub struct UserPermissions {
    permissions: HashSet<Permission>,
}

impl UserPermissions {
    fn root_profile() -> Self {
        UserPermissions {
            permissions: vec![
                Permission::AddUser,
                Permission::RemoveUser,
                Permission::JudgeIdentity,
            ]
            .into_iter()
            .collect(),
        }
    }
    fn judge_profile() -> Self {
        UserPermissions {
            permissions: vec![Permission::JudgeIdentity].into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum Permission {
    AddUser,
    RemoveUser,
    JudgeIdentity,
}

#[derive(Default)]
struct VerifyAdmin {
    users: HashMap<UserId, UserPermissions>,
    check_for: Vec<Permission>,
}

impl VerifyAdmin {
    fn check_for(check_for: Vec<Permission>) -> Self {
        VerifyAdmin {
            users: HashMap::default(),
            check_for: check_for,
        }
    }
}

impl Guard for VerifyAdmin {
    fn check(&self, _request: &RequestHead) -> bool {
        // TODO: pull UUID from request.
        let user_id = UserId;

        if let Some(permissions) = self.users.get(&user_id) {
            for permission in &self.check_for {
                if !permissions.permissions.contains(permission) {
                    return false;
                }
            }

            true
        } else {
            false
        }
    }
}

#[rustfmt::skip]
pub async fn run(_config: Config) -> Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(healthcheck)
            .service(admin_scope())
            .service(public_scope())
        })
        .bind("127.0.0.1:8080")?
        .run()
        .await?;

    Ok(())
}

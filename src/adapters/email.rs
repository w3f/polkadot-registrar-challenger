use crate::Result;
use lettre::smtp::authentication::Credentials;
use lettre::smtp::SmtpClient;
use lettre::Transport;
use lettre_email::EmailBuilder;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use std::result::Result as StdResult;
use tokio::time::{self, Duration};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct EmailId(u64);

impl From<u32> for EmailId {
    fn from(val: u32) -> Self {
        EmailId(val as u64)
    }
}

impl From<u64> for EmailId {
    fn from(val: u64) -> Self {
        EmailId(val)
    }
}

trait ConvertEmailInto<T> {
    type Error;

    fn convert_into(self) -> std::result::Result<T, Self::Error>;
}

impl ConvertEmailInto<String> for String {
    type Error = anyhow::Error;

    fn convert_into(self) -> Result<String> {
        if self.contains("<") {
            let parts = self.split("<");
            if let Some(email) = parts.into_iter().nth(1) {
                Ok(email.replace(">", ""))
            } else {
                Err(anyhow!("unrecognized data"))
            }
        } else {
            Ok(self)
        }
    }
}

pub struct SmtpImapClientBuilder {
    server: Option<String>,
    imap_server: Option<String>,
    inbox: Option<String>,
    user: Option<String>,
    password: Option<String>,
}

impl SmtpImapClientBuilder {
    pub fn new() -> Self {
        SmtpImapClientBuilder {
            server: None,
            imap_server: None,
            inbox: None,
            user: None,
            password: None,
        }
    }
    pub fn email_server(mut self, server: String) -> Self {
        self.server = Some(server);
        self
    }
    pub fn imap_server(mut self, imap_server: String) -> Self {
        self.imap_server = Some(imap_server);
        self
    }
    pub fn email_inbox(mut self, inbox: String) -> Self {
        self.inbox = Some(inbox);
        self
    }
    pub fn email_user(mut self, user: String) -> Self {
        self.user = Some(user);
        self
    }
    pub fn email_password(mut self, password: String) -> Self {
        self.password = Some(password);
        self
    }
    pub fn build(self) -> Result<SmtpImapClient> {
        Ok(SmtpImapClient {
            smtp_server: self.server.ok_or(anyhow!("SMTP server not specified"))?,
            imap_server: self
                .imap_server
                .ok_or(anyhow!("SMTP server not specified"))?,
            inbox: self.inbox.ok_or(anyhow!("SMTP server not specified"))?,
            user: self.user.ok_or(anyhow!("SMTP server not specified"))?,
            password: self.password.ok_or(anyhow!("SMTP server not specified"))?,
        })
    }
}

#[derive(Clone)]
pub struct SmtpImapClient {
    smtp_server: String,
    imap_server: String,
    inbox: String,
    user: String,
    password: String,
}

impl SmtpImapClient {
    async fn request_messages(&self) -> Result<Vec<()>> {
        let tls = native_tls::TlsConnector::builder().build()?;
        let client = imap::connect((self.imap_server.as_str(), 993), &self.imap_server, &tls)?;

        let mut imap = client
            .login(&self.user, &self.password)
            .map_err(|(err, _)| err)?;

        imap.select(&self.inbox)?;

        // Fetch the messages of the last day. The database keeps track of which messages
        // have been processed.
        //
        // Gmail has a custom search syntax and does not support the IMAP
        // standardized queries.
        let recent_seq = imap.search("X-GM-RAW \"newer_than:2d\"")?;

        if recent_seq.is_empty() {
            return Ok(vec![]);
        }

        let min = recent_seq.iter().min().unwrap();
        let max = recent_seq.iter().max().unwrap();

        let query = if min == max {
            min.to_string()
        } else {
            format!("{}:{}", min, max)
        };

        let messages = imap.fetch(query, "(RFC822 UID)")?;

        let mut parsed_messages = vec![];
        for message in &messages {
            let email_id = EmailId::from(message.uid.ok_or(anyhow!("unrecognized data"))?);
            if let Some(body) = message.body() {
                let mail = mailparse::parse_mail(body)?;

                let sender = mail
                    .headers
                    .iter()
                    .find(|header| header.get_key_ref() == "From")
                    .ok_or(anyhow!("unrecognized data"))?
                    .get_value()
                    .convert_into()?;

                if let Ok(body) = mail.get_body() {
                    //parsed_messages.push(create_message_context(email_id, sender.clone(), body));
                } else {
                    warn!("No body found in message from {}", sender);
                }

                for subpart in mail.subparts {
                    if let Ok(body) = subpart.get_body() {
                        /*
                            parsed_messages.push(create_message_context(
                                email_id,
                                sender.clone(),
                                body,
                            ));
                        */
                    } else {
                        warn!("No body found in subpart message from {}", sender);
                    }
                }
            } else {
                warn!("No body");
            }
        }

        Ok(parsed_messages)
    }
    /*
    async fn send_message(&self, account: &Account, message: VerifierMessage) -> Result<()> {
        // SMTP transport
        let mut smtp = SmtpClient::new_simple(&self.smtp_server)?
            .credentials(Credentials::new(
                self.user.to_string(),
                self.password.to_string(),
            ))
            .transport();

        let email = EmailBuilder::new()
            // Addresses can be specified by the tuple (email, alias)
            .to(account.as_str())
            .from(self.user.as_str())
            .subject("W3F Registrar Verification Service")
            .text(message.as_str())
            .build()
            .unwrap();

        let _ = smtp.send(email.into())?;

        Ok(())
    }
    */
}

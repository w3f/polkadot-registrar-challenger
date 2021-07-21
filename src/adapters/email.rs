use std::collections::HashSet;

use crate::adapters::Adapter;
use crate::primitives::{
    ExpectedMessage, ExternalMessage, ExternalMessageType, MessageId, Timestamp,
};
use crate::Result;
use lettre::smtp::authentication::Credentials;
use lettre::smtp::SmtpClient;
use lettre::Transport;
use lettre_email::EmailBuilder;

trait ExtractSender<T> {
    type Error;

    fn extract_sender(self) -> std::result::Result<T, Self::Error>;
}

impl ExtractSender<String> for String {
    type Error = anyhow::Error;

    fn extract_sender(self) -> Result<String> {
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
    pub fn smtp_server(mut self, server: String) -> Self {
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
                .ok_or(anyhow!("IMAP server not specified"))?,
            inbox: self.inbox.ok_or(anyhow!("inbox server not specified"))?,
            user: self.user.ok_or(anyhow!("user server not specified"))?,
            password: self
                .password
                .ok_or(anyhow!("password server not specified"))?,
            cache: HashSet::new(),
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
    // Keep track of messages.
    cache: HashSet<MessageId>,
}

impl SmtpImapClient {
    fn request_messages(&mut self) -> Result<Vec<ExternalMessage>> {
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
        let recent_seq = imap.search("X-GM-RAW \"newer_than:1d\"")?;

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
            if let Some(body) = message.body() {
                let mail = mailparse::parse_mail(body)?;

                let sender = mail
                    .headers
                    .iter()
                    .find(|header| header.get_key_ref() == "From")
                    .ok_or(anyhow!("unrecognized data"))?
                    .get_value()
                    .extract_sender()?;

                let id = message
                    .uid
                    .ok_or(anyhow!("missing UID for email message"))?
                    .into();

                // Skip message if it was already processed.
                if self.cache.contains(&id) {
                    continue;
                }

                debug!("Received message from {}", sender);

                // Prepare parsed message
                let mut parsed_message = ExternalMessage {
                    origin: ExternalMessageType::Email(sender),
                    id: message
                        .uid
                        .ok_or(anyhow!("missing UID for email message"))?
                        .into(),
                    timestamp: Timestamp::now(),
                    values: vec![],
                };

                // Add body content.
                if let Ok(body) = mail.get_body() {
                    parsed_message.values.push(body.into());
                } else {
                    warn!("No body found in message");
                }

                // An email message can contain multiple "subparts". Add each of
                // those into the prepared message.
                for subpart in mail.subparts {
                    if let Ok(body) = subpart.get_body() {
                        parsed_message.values.push(body.into());
                    } else {
                        debug!("No body found in subpart message");
                    }
                }

                self.cache.insert(parsed_message.id);
                parsed_messages.push(parsed_message);
            } else {
                warn!("No body found for message");
            }
        }

        Ok(parsed_messages)
    }
    async fn send_message(&self, to: &str, message: &str) -> Result<()> {
        // SMTP transport
        let mut smtp = SmtpClient::new_simple(&self.smtp_server)?
            .credentials(Credentials::new(
                self.user.to_string(),
                self.password.to_string(),
            ))
            .transport();

        let email = EmailBuilder::new()
            // Addresses can be specified by the tuple (email, alias)
            .to(to)
            .from(self.user.as_str())
            .subject("W3F Registrar Verification Service")
            .text(format!(
                "Insert the following challenge into the web interface: {:?}",
                message
            ))
            .build()?;

        let _ = smtp.send(email.into())?;

        Ok(())
    }
}

#[async_trait]
impl Adapter for SmtpImapClient {
    type MessageType = ExpectedMessage;

    fn name(&self) -> &'static str {
        "email"
    }
    async fn fetch_messages(&mut self) -> Result<Vec<ExternalMessage>> {
        self.request_messages()
    }
    async fn send_message(&mut self, to: &str, content: Self::MessageType) -> Result<()> {
        Self::send_message(&self, to, content.value.as_str()).await
    }
}

use crate::comms::{CommsMessage, CommsVerifier};
use crate::db::Database;
use crate::manager::AccountStatus;
use crate::primitives::{Account, AccountType, NetAccount, Result};
use crate::verifier::{invalid_accounts_message, verification_handler, Verifier, VerifierMessage};
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

impl ToSql for EmailId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::Borrowed(ValueRef::Integer(self.0 as i64)))
    }
}

impl FromSql for EmailId {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Integer(val) => Ok(EmailId(val as u64)),
            _ => Err(FromSqlError::InvalidType),
        }
    }
}

trait ConvertEmailInto<T> {
    type Error;

    fn convert_into(self) -> StdResult<T, Self::Error>;
}

impl ConvertEmailInto<Account> for String {
    type Error = ClientError;

    fn convert_into(self) -> StdResult<Account, Self::Error> {
        if self.contains("<") {
            let parts = self.split("<");
            if let Some(email) = parts.into_iter().nth(1) {
                Ok(Account::from(email.replace(">", "")))
            } else {
                Err(ClientError::UnrecognizedData)
            }
        } else {
            Ok(Account::from(self))
        }
    }
}

impl ConvertEmailInto<Account> for &str {
    type Error = ClientError;

    fn convert_into(self) -> StdResult<Account, Self::Error> {
        <String as ConvertEmailInto<Account>>::convert_into(self.to_string())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
// TODO: Only expose fields to tests.
pub struct ReceivedMessageContext {
    pub id: EmailId,
    pub sender: Account,
    pub body: String,
}

#[derive(Debug, Fail)]
pub enum ClientError {
    #[fail(display = "the builder was not used correctly")]
    IncompleteBuilder,
    #[fail(display = "Unrecognized data returned from the Gmail API")]
    UnrecognizedData,
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
            smtp_server: self.server.ok_or(ClientError::IncompleteBuilder)?,
            imap_server: self.imap_server.ok_or(ClientError::IncompleteBuilder)?,
            inbox: self.inbox.ok_or(ClientError::IncompleteBuilder)?,
            user: self.user.ok_or(ClientError::IncompleteBuilder)?,
            password: self.password.ok_or(ClientError::IncompleteBuilder)?,
        })
    }
}

#[async_trait]
pub trait EmailTransport: 'static + Send + Sync {
    async fn request_messages(&self) -> Result<Vec<ReceivedMessageContext>>;
    async fn send_message(&self, account: &Account, msg: VerifierMessage) -> Result<()>;
}

#[derive(Clone)]
pub struct SmtpImapClient {
    smtp_server: String,
    imap_server: String,
    inbox: String,
    user: String,
    password: String,
}

#[async_trait]
impl EmailTransport for SmtpImapClient {
    async fn request_messages(&self) -> Result<Vec<ReceivedMessageContext>> {
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

        fn create_message_context(
            email_id: EmailId,
            sender: Account,
            body: String,
        ) -> ReceivedMessageContext {
            // The very first line must be the signature. If the message cannot
            // be parsed correctly, then just use empty strings which will
            // automatically invalidate the signature without aborting the whole
            // process.
            ReceivedMessageContext {
                id: email_id,
                sender: sender,
                body: format!(
                    "{:?}",
                    body.lines()
                        .nth(0)
                        .unwrap_or("")
                        .split(" ")
                        .collect::<Vec<&str>>()
                        .iter()
                        .nth(0)
                        .unwrap_or(&"")
                        .trim()
                )
                .replace("\"", ""),
            }
        }

        let mut parsed_messages = vec![];
        for message in &messages {
            let email_id = EmailId::from(message.uid.ok_or(ClientError::UnrecognizedData)?);
            if let Some(body) = message.body() {
                let mail = mailparse::parse_mail(body)?;

                let sender = mail
                    .headers
                    .iter()
                    .find(|header| header.get_key_ref() == "From")
                    .ok_or(ClientError::UnrecognizedData)?
                    .get_value()
                    .convert_into()?;

                if let Ok(body) = mail.get_body() {
                    parsed_messages.push(create_message_context(email_id, sender.clone(), body));
                } else {
                    warn!("No body found in message from {}", sender);
                }

                for subpart in mail.subparts {
                    if let Ok(body) = subpart.get_body() {
                        parsed_messages.push(create_message_context(
                            email_id,
                            sender.clone(),
                            body,
                        ));
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
}

#[derive(Clone)]
pub struct EmailHandler {
    db: Database,
    comms: CommsVerifier,
}

impl EmailHandler {
    pub fn new(db: Database, comms: CommsVerifier) -> Self {
        EmailHandler {
            db: db,
            comms: comms,
        }
    }
}

impl EmailHandler {
    pub async fn start<T: Clone + EmailTransport>(self, transport: T) {
        // Start incoming messages handler.
        let l_self = self.clone();
        let l_transport = transport.clone();
        tokio::spawn(async move {
            loop {
                let _ = l_self
                    .handle_incoming_messages(&l_transport)
                    .await
                    .map_err(|err| {
                        error!("{}", err);
                        err
                    });

                time::delay_for(Duration::from_secs(3)).await;
            }
        });

        loop {
            let _ = self.local(&transport).await.map_err(|err| {
                error!("{}", err);
                err
            });
        }
    }
    async fn local<T: EmailTransport>(&self, transport: &T) -> Result<()> {
        use CommsMessage::*;

        match self.comms.recv().await {
            AccountToVerify {
                net_account: _,
                account,
            } => {
                self.handle_account_verification(transport, account).await?;
            }
            NotifyInvalidAccount {
                net_account,
                account,
                accounts,
            } => {
                self.handle_invalid_account_notification(net_account, account, accounts, transport)
                    .await?
            }
            _ => warn!("Received unrecognized message type"),
        }

        Ok(())
    }
    async fn handle_account_verification<T: EmailTransport>(
        &self,
        transport: &T,
        account: Account,
    ) -> Result<()> {
        let challenge_data = self
            .db
            .select_challenge_data(&account, &AccountType::Email)
            .await?;

        debug!("Sending initial message to {}", account.as_str());

        // Only require the verifier to send the initial message
        let verifier = Verifier::new(&challenge_data);
        transport
            .send_message(&account, verifier.init_message_builder(true))
            .await?;

        Ok(())
    }
    async fn handle_incoming_messages<T: EmailTransport>(&self, transport: &T) -> Result<()> {
        let messages = transport.request_messages().await?;

        if messages.is_empty() {
            trace!("No new messages found");
            return Ok(());
        }

        // Check database on which of the new messages were not processed yet.
        let mut email_ids = messages.iter().map(|msg| msg.id).collect::<Vec<EmailId>>();
        email_ids.sort();
        email_ids.dedup();

        let unknown_email_ids = self.db.find_untracked_email_ids(&email_ids).await?;

        for email_id in unknown_email_ids {
            // Filter messages based on EmailId.
            let user_messages = messages
                .iter()
                .filter(|msg| &msg.id == email_id)
                .collect::<Vec<&ReceivedMessageContext>>();

            let sender = &user_messages.first().unwrap().sender;
            debug!("New message from {}", sender.as_str());

            debug!("Fetching challenge data");
            let challenge_data = self
                .db
                .select_challenge_data(sender, &AccountType::Email)
                .await?;

            if challenge_data.is_empty() {
                warn!("No challenge data found for {}. Ignoring", sender.as_str());

                self.db.track_email_id(email_id).await?;
                continue;
            }

            // Set email as valid.
            self.db
                .set_account_status(
                    challenge_data.get(0).unwrap().0.address(),
                    &AccountType::Email,
                    &AccountStatus::Valid,
                )
                .await?;

            let mut verifier = Verifier::new(&challenge_data);

            for message in &user_messages {
                debug!("Verifying message: {}", message.body);
                verifier.verify(&message.body);
            }

            // Update challenge statuses and notify manager
            verification_handler(&verifier, &self.db, &self.comms, &AccountType::Email).await?;

            // Inform user about the current state of the verification
            transport
                .send_message(sender, verifier.response_message_builder())
                .await?;

            self.db.track_email_id(email_id).await?;
        }

        Ok(())
    }
    async fn handle_invalid_account_notification<T: EmailTransport>(
        &self,
        net_account: NetAccount,
        account: Account,
        accounts: Vec<(AccountType, Account, AccountStatus)>,
        transport: &T,
    ) -> Result<()> {
        // Check for any display name violations (optional).
        let violations = self.db.select_display_name_violations(&net_account).await?;

        transport
            .send_message(&account, invalid_accounts_message(&accounts, violations))
            .await?;

        Ok(())
    }
}

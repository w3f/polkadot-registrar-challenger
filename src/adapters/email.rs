use crate::comms::{CommsMessage, CommsVerifier};
use crate::db::Database2;
use crate::primitives::{Account, AccountType, NetAccount, Result};
use crate::verifier::{Verifier2, verification_handler};
use lettre::smtp::authentication::Credentials;
use lettre::smtp::SmtpClient;
use lettre::Transport;
use lettre_email::EmailBuilder;
use reqwest::Client as ReqClient;
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

#[derive(Debug, Clone)]
struct ReceivedMessageContext {
    id: EmailId,
    sender: Account,
    body: String,
}

#[derive(Debug, Fail)]
pub enum ClientError {
    #[fail(display = "the builder was not used correctly")]
    IncompleteBuilder,
    #[fail(display = "Unrecognized data returned from the Gmail API")]
    UnrecognizedData,
}

pub struct ClientBuilder {
    db: Database2,
    comms: CommsVerifier,
    server: Option<String>,
    imap_server: Option<String>,
    inbox: Option<String>,
    user: Option<String>,
    password: Option<String>,
}

impl ClientBuilder {
    pub fn new(db: Database2, comms: CommsVerifier) -> Self {
        ClientBuilder {
            db: db,
            comms: comms,
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
    pub fn build(self) -> Result<Client> {
        Ok(Client {
            client: ReqClient::new(),
            db: self.db,
            comms: self.comms,
            server: self.server.ok_or(ClientError::IncompleteBuilder)?,
            imap_server: self.imap_server.ok_or(ClientError::IncompleteBuilder)?,
            inbox: self.inbox.ok_or(ClientError::IncompleteBuilder)?,
            user: self.user.ok_or(ClientError::IncompleteBuilder)?,
            password: self.password.ok_or(ClientError::IncompleteBuilder)?,
        })
    }
}

#[derive(Clone)]
pub struct Client {
    client: ReqClient,
    db: Database2,
    comms: CommsVerifier,
    server: String,
    imap_server: String,
    inbox: String,
    user: String,
    password: String,
}

impl Client {
    async fn request_message(&self) -> Result<Vec<ReceivedMessageContext>> {
        let tls = native_tls::TlsConnector::builder().build()?;
        let client = imap::connect((self.imap_server.as_str(), 993), &self.imap_server, &tls)?;

        let mut transport = client
            .login(&self.user, &self.password)
            .map_err(|(err, _)| err)?;

        transport.select(&self.inbox)?;

        // Find the message sequence/index of unread messages and fetch that
        // range, plus some extra. The database keeps track of which messages
        // have been processed.
        //
        // Gmail has a custom search syntax and does not support the IMAP
        // standardized queries.
        let recent_seq = transport.search("X-GM-RAW \"is:unread\"")?;

        if recent_seq.is_empty() {
            return Ok(vec![]);
        }

        let min = recent_seq.iter().min().unwrap();
        let max = recent_seq.iter().max().unwrap();

        let query = if min == max {
            min.to_string()
        } else {
            format!("{}:{}", min.saturating_sub(5).max(1), max)
        };

        let messages = transport.fetch(query, "(RFC822 UID)")?;

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
    async fn send_message(&self, account: &Account, msg: String) -> Result<()> {
        let email = EmailBuilder::new()
            // Addresses can be specified by the tuple (email, alias)
            .to(account.as_str())
            .from(self.user.as_str())
            .subject("W3F Registrar Verification Service")
            .text(msg)
            .build()
            .unwrap();

        // TODO: Can cloning/to_string be avoided here?
        let mut transport = SmtpClient::new_simple(&self.server)?
            .credentials(Credentials::new(
                self.user.to_string(),
                self.password.to_string(),
            ))
            .transport();

        let _x = transport.send(email.into())?;

        Ok(())
    }
    pub async fn start(self) {
        self.start_responder().await;

        loop {
            let _ = self.local().await.map_err(|err| {
                error!("{}", err);
                err
            });
        }
    }
    async fn local(&self) -> Result<()> {
        use CommsMessage::*;

        match self.comms.recv().await {
            AccountToVerify {
                net_account,
                account,
            } => {
                self.handle_account_verification(net_account, account)
                    .await?
            }
            _ => {}
        }

        Ok(())
    }
    async fn start_responder(&self) {
        let c_self = self.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(3));

            loop {
                interval.tick().await;

                let _ = c_self.handle_incoming_messages().await.map_err(|err| {
                    error!("{}", err);
                    err
                });
            }
        });
    }
    async fn handle_account_verification(
        &self,
        _net_account: NetAccount,
        account: Account,
    ) -> Result<()> {
        let challenge_data = self
            .db
            .select_challenge_data(&account, &AccountType::Email)
            .await?;

        debug!("Sending initial message to {}", account.as_str());

        // Only require the verifier to send the initial message
        let verifier = Verifier2::new(&challenge_data);
        self.send_message(&account, verifier.init_message_builder(true))
            .await?;

        Ok(())
    }
    async fn handle_incoming_messages(&self) -> Result<()> {
        let messages = self.request_message().await?;

        if messages.is_empty() {
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
                warn!("No challenge data found for {}", sender.as_str());

                self.db.track_email_id(email_id).await?;
                continue;
            }

            let mut verifier = Verifier2::new(&challenge_data);

            for message in &user_messages {
                debug!("Verifying message: {}", message.body);
                verifier.verify(&message.body);
            }

            // Update challenge statuses and notify manager
            verification_handler(&verifier, &self.db, &self.comms).await?;

            // Inform user about the current state of the verification
            self.send_message(sender, verifier.response_message_builder())
                .await?;
            self.db.track_email_id(email_id).await?;
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiMessage {
    id: String,
    thread_id: String,
    payload: ApiPayload,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiPayload {
    headers: Vec<ApiHeader>,
    body: Option<ApiBody>,
    parts: Option<Vec<ApiPart>>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiHeader {
    name: String,
    value: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiPart {
    part_id: String,
    mime_type: String,
    filename: String,
    body: ApiBody,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiBody {
    size: i64,
    data: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comms::CommsVerifier;
    use crate::open_config;
    use crate::primitives::Challenge;
    use crate::Database2;
    use tokio::runtime::Runtime;

    // Generate a random db path
    fn db_path() -> String {
        format!("/tmp/sqlite_{}", Challenge::gen_random().as_str())
    }

    #[test]
    fn request_message() {
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {
            let config = open_config().unwrap();
            let db = Database2::new(&db_path()).unwrap();

            let email = ClientBuilder::new(db, CommsVerifier::new())
                .email_server(config.email_server)
                .imap_server(config.imap_server)
                .email_inbox(config.email_inbox)
                .email_user(config.email_user)
                .email_password(config.email_password)
                .build()
                .unwrap();

            let res = email.request_message().await.unwrap();
            for msg in res {
                println!(">> SENDER: {}", msg.sender.as_str());
                println!(">> BODY: {}", msg.body);
                println!("\n\n")
            }
        });
    }
}

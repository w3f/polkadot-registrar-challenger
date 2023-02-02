use crate::adapters::Adapter;
use crate::primitives::{ExternalMessage, ExternalMessageType, MessageId, Timestamp};
use crate::Result;
use base64::Engine;
use hmac::{Hmac, Mac};
use rand::{thread_rng, Rng};
use reqwest::header::{self, HeaderValue};
use reqwest::{Client, Request};
use serde::de::DeserializeOwned;
use serde::Serialize;
use sha1::Sha1;
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{cmp::Ordering, hash::Hash};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReceivedMessageContext {
    sender: TwitterId,
    id: u64,
    message: String,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TwitterId(u64);

impl TwitterId {
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Ord for TwitterId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for TwitterId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<u64> for TwitterId {
    fn from(val: u64) -> Self {
        TwitterId(val)
    }
}

impl TryFrom<String> for TwitterId {
    type Error = anyhow::Error;

    fn try_from(val: String) -> Result<Self> {
        Ok(TwitterId(val.parse::<u64>()?))
    }
}

pub struct TwitterBuilder {
    consumer_key: Option<String>,
    consumer_secret: Option<String>,
    token: Option<String>,
    token_secret: Option<String>,
}

impl TwitterBuilder {
    pub fn new() -> Self {
        TwitterBuilder {
            consumer_key: None,
            consumer_secret: None,
            token: None,
            token_secret: None,
        }
    }
    pub fn consumer_key(mut self, key: String) -> Self {
        self.consumer_key = Some(key);
        self
    }
    pub fn consumer_secret(mut self, key: String) -> Self {
        self.consumer_secret = Some(key);
        self
    }
    pub fn token(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }
    pub fn token_secret(mut self, secret: String) -> Self {
        self.token_secret = Some(secret);
        self
    }
    pub fn build(self) -> Result<TwitterClient> {
        Ok(TwitterClient {
            client: Client::new(),
            consumer_key: self
                .consumer_key
                .ok_or_else(|| anyhow!("consumer key name not specified"))?,
            consumer_secret: self
                .consumer_secret
                .ok_or_else(|| anyhow!("consumer secret name not specified"))?,
            token: self.token.ok_or_else(|| anyhow!("token not specified"))?,
            token_secret: self
                .token_secret
                .ok_or_else(|| anyhow!("token secret not specified"))?,
            twitter_ids: HashMap::new(),
            cache: HashSet::new(),
        })
    }
}

fn gen_nonce() -> String {
    let random: [u8; 16] = thread_rng().gen();
    hex::encode(random)
}

fn gen_timestamp() -> u64 {
    let start = SystemTime::now();
    start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
}

#[derive(Clone)]
pub struct TwitterClient {
    client: Client,
    consumer_key: String,
    consumer_secret: String,
    token: String,
    token_secret: String,
    twitter_ids: HashMap<TwitterId, String>,
    // Keep track of messages.
    cache: HashSet<MessageId>,
}

impl TwitterClient {
    async fn request_messages(&mut self) -> Result<Vec<ExternalMessage>> {
        debug!("Requesting Twitter messages");
        // Request message on parse those into a simpler type.
        let mut messages = self
            .get_request::<ApiMessageRequest>(
                "https://api.twitter.com/1.1/direct_messages/events/list.json",
                None,
            )
            .await?
            .parse()?;

        // Skip message if it was already processed.
        messages.retain(|message| !self.cache.contains(&message.id.into()));

        if messages.is_empty() {
            debug!("No new Twitter messages found");
            return Ok(vec![]);
        } else {
            debug!("Fetched {} message(-s)", messages.len());
        }

        // Collect all the Twitter Ids that need to be looked-up.
        #[rustfmt::skip]
        let mut to_lookup: Vec<&TwitterId> = messages
            .iter()
            .filter(|message| {
                // Only lookup Ids that aren't cached.
                !self.twitter_ids.contains_key(&message.sender)
            })
            .map(|message| &message.sender)
            .collect();

        // Remove duplicates.
        to_lookup.sort();
        to_lookup.dedup();

        // Lookup Twitter Ids and insert those into the cache.
        debug!("Looking up Twitter Ids");
        if !to_lookup.is_empty() {
            let lookup_results = self.lookup_twitter_id(Some(&to_lookup), None).await?;
            self.twitter_ids.extend(lookup_results);
        }

        // Parse all messages into `TwitterMessage`.
        let mut parsed_messages = vec![];
        for message in messages {
            let sender = self
                .twitter_ids
                .get(&message.sender)
                .ok_or_else(|| anyhow!("Failed to find Twitter handle based on Id"))?
                .clone();

            let id = message.id.into();

            parsed_messages.push(ExternalMessage {
                origin: ExternalMessageType::Twitter(sender),
                id,
                timestamp: Timestamp::now(),
                values: vec![message.message.into()],
            });

            self.cache.insert(id);
        }

        Ok(parsed_messages)
    }
    /// Creates a signature as documented here:
    /// https://developer.twitter.com/en/docs/authentication/oauth-1-0a/creating-a-signature
    fn authenticate_request(
        &self,
        url: &str,
        request: &mut Request,
        params: Option<&[(&str, &str)]>,
    ) -> Result<()> {
        use urlencoding::encode;

        // Prepare  required data.
        let nonce = gen_nonce();
        let timestamp = gen_timestamp().to_string();

        // Create  OAuth 1.0 fields.
        let mut fields = vec![
            ("oauth_consumer_key", self.consumer_key.as_str()),
            ("oauth_nonce", nonce.as_str()),
            ("oauth_signature_method", "HMAC-SHA1"),
            ("oauth_timestamp", &timestamp),
            ("oauth_token", self.token.as_str()),
            ("oauth_version", "1.0"),
        ];

        if let Some(params) = params {
            fields.append(&mut params.to_vec());
        }

        fields.sort_by(|(a, _), (b, _)| a.cmp(b));

        let mut params = String::new();
        for (name, val) in &fields {
            params.push_str(&format!("{}={}&", encode(name), encode(val)));
        }

        // Remove the trailing `&`.
        params.pop();

        let base = format!("GET&{}&{}", encode(url), encode(&params));

        // Sign the base string.
        let sign_key = format!(
            "{}&{}",
            encode(&self.consumer_secret),
            encode(&self.token_secret)
        );

        let mut mac: Hmac<Sha1> = Hmac::new_from_slice(sign_key.as_bytes()).unwrap();
        mac.update(base.as_bytes());

        // Create the resulting hash.
        let sig = base64::engine::general_purpose::STANDARD_NO_PAD.encode(mac.finalize().into_bytes());

        // Insert the signature;
        fields.push(("oauth_signature", &sig));
        fields.sort_by(|(a, _), (b, _)| a.cmp(b));

        // Merge all fields into the OAuth header.
        let mut oauth_header = String::new();
        oauth_header.push_str("OAuth ");

        for (name, val) in &fields {
            oauth_header.push_str(&format!("{}={}, ", encode(name), encode(val)))
        }

        // Remove the trailing `, `.
        oauth_header.pop();
        oauth_header.pop();

        // Inserth the authentication header into the request.
        request
            .headers_mut()
            .insert(header::AUTHORIZATION, HeaderValue::from_str(&oauth_header)?);

        Ok(())
    }
    async fn get_request<T: DeserializeOwned>(
        &self,
        url: &str,
        params: Option<&[(&str, &str)]>,
    ) -> Result<T> {
        let mut full_url = String::from(url);

        if let Some(params) = params {
            full_url.push('?');
            for (key, val) in params {
                full_url.push_str(&format!("{}={}&", key, val));
            }

            // Remove trailing `&` or `?` in case "params" is empty.
            full_url.pop();
        }

        let mut request = self.client.get(&full_url).build()?;
        self.authenticate_request(url, &mut request, params)?;
        let resp = self.client.execute(request).await?;
        let txt = resp.text().await?;

        debug!("Twitter response: {:?}", txt);

        serde_json::from_str::<T>(&txt).map_err(|err| err.into())
    }
    async fn lookup_twitter_id(
        &self,
        twitter_ids: Option<&[&TwitterId]>,
        accounts: Option<&[&String]>,
    ) -> Result<HashMap<TwitterId, String>> {
        let mut params = vec![];

        // Lookups for UserIds
        let mut lookup = String::new();
        if let Some(twitter_ids) = twitter_ids {
            for twitter_id in twitter_ids {
                lookup.push_str(&twitter_id.as_u64().to_string());
                lookup.push(',');
            }

            // Remove trailing `,`.
            lookup.pop();

            params.push(("user_id", lookup.as_str()))
        }

        // Lookups for Accounts
        let mut lookup = String::new();
        if let Some(accounts) = accounts {
            for account in accounts {
                lookup.push_str(&account.as_str().replace('@', ""));
                lookup.push(',');
            }

            // Remove trailing `,`.
            lookup.pop();

            params.push(("screen_name", lookup.as_str()))
        }

        #[derive(Deserialize)]
        // Only `screen_name` required.
        struct UserObject {
            id: TwitterId,
            screen_name: String,
        }

        debug!("Params: {:?}", params);

        let user_objects = self
            .get_request::<Vec<UserObject>>(
                "https://api.twitter.com/1.1/users/lookup.json",
                Some(&params),
            )
            .await?;

        if user_objects.is_empty() {
            return Err(anyhow!("unrecognized data"));
        }

        Ok(user_objects
            .into_iter()
            .map(|obj| (obj.id, format!("@{}", obj.screen_name.to_lowercase())))
            .collect())
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ApiMessageRequest {
    events: Vec<ApiEvent>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApiEvent {
    #[serde(rename = "type")]
    t_type: String,
    id: String,
    created_timestamp: Option<String>,
    message_create: ApiMessageCreate,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApiMessageCreate {
    target: ApiTarget,
    sender_id: Option<String>,
    message_data: ApiMessageData,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApiTarget {
    recipient_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ApiMessageData {
    text: String,
}

impl ApiMessageRequest {
    fn parse(self) -> Result<Vec<ReceivedMessageContext>> {
        let mut messages = vec![];

        for event in self.events {
            let message = ReceivedMessageContext {
                sender: event
                    .message_create
                    .sender_id
                    .ok_or_else(|| anyhow!("unrecognized data"))?
                    .try_into()?,
                message: event.message_create.message_data.text,
                id: event.id.parse().map_err(|_| anyhow!("unrecognized data"))?,
            };

            messages.push(message);
        }

        Ok(messages)
    }
}

#[async_trait]
impl Adapter for TwitterClient {
    type MessageType = ();

    fn name(&self) -> &'static str {
        "Twitter"
    }
    async fn fetch_messages(&mut self) -> Result<Vec<ExternalMessage>> {
        self.request_messages().await
    }
    async fn send_message(&mut self, _to: &str, _content: Self::MessageType) -> Result<()> {
        unimplemented!()
    }
}

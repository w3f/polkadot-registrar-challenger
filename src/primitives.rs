use actix::Message;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityContext {
    pub address: ChainAddress,
    pub chain: ChainName,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChainAddress(String);

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainName {
    Polkadot,
    Kusama,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityField {
    pub value: IdentityFieldValue,
    pub expected_challenge: ExpectedChallenge,
    #[serde(skip)]
    pub second_expected_challenge: Option<ExpectedChallenge>,
    pub failed_attempts: isize,
}

impl IdentityField {
    pub fn new(val: IdentityFieldValue, second_challenge: bool) -> Self {
        IdentityField {
            value: val,
            expected_challenge: ExpectedChallenge::random(),
            second_expected_challenge: {
                if second_challenge {
                    Some(ExpectedChallenge::random())
                } else {
                    None
                }
            },
            failed_attempts: 0,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExpectedChallenge {
    pub value: String,
    pub is_verified: bool,
}

impl ExpectedChallenge {
    pub fn random() -> Self {
        use rand::{thread_rng, Rng};

        let random: [u8; 16] = thread_rng().gen();
        ExpectedChallenge {
            value: hex::encode(random),
            is_verified: false,
        }
    }
    pub fn verify_message(&mut self, message: &ExternalMessage) -> bool {
        for value in &message.values {
            if value.0.contains(&self.value) {
                self.is_verified = true;
                return true;
            }
        }

        false
    }
    #[cfg(test)]
    pub fn set_verified(&mut self) {
        self.is_verified = true;
    }
    #[cfg(test)]
    pub fn into_message_parts(&self) -> Vec<MessagePart> {
        vec![self.value.clone().into()]
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum IdentityFieldValue {
    LegalName(String),
    DisplayName(String),
    Email(String),
    Web(String),
    Twitter(String),
    Matrix(String),
    PGPFingerprint(()),
    Image(()),
    Additional(()),
}

impl IdentityFieldValue {
    pub fn matches(&self, message: &ExternalMessage) -> bool {
        match self {
            IdentityFieldValue::Email(n1) => match &message.origin {
                ExternalMessageType::Email(n2) => n1 == n2,
                _ => false,
            },
            IdentityFieldValue::Twitter(n1) => match &message.origin {
                ExternalMessageType::Twitter(n2) => n1 == n2,
                _ => false,
            },
            IdentityFieldValue::Matrix(n1) => match &message.origin {
                ExternalMessageType::Matrix(n2) => n1 == n2,
                _ => false,
            },
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JudgementState {
    pub context: IdentityContext,
    pub is_fully_verified: bool,
    pub completion_timestamp: Option<Timestamp>,
    pub fields: Vec<IdentityField>,
}

impl JudgementState {
    pub fn is_fully_verified(&self) -> bool {
        self.fields.iter().all(|field| {
            field.expected_challenge.is_verified && {
                match field.second_expected_challenge {
                    Some(ref challenge) => challenge.is_verified,
                    None => true,
                }
            }
        })
    }
    #[cfg(test)]
    pub fn get_field<'a>(&'a self, ty: &IdentityFieldValue) -> &'a IdentityField {
        self.fields.iter().find(|field| &field.value == ty).unwrap()
    }
    #[cfg(test)]
    pub fn get_field_mut<'a>(&'a mut self, ty: &IdentityFieldValue) -> &'a mut IdentityField {
        self.fields
            .iter_mut()
            .find(|field| &field.value == ty)
            .unwrap()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Message)]
#[serde(rename_all = "snake_case")]
#[rtype(result = "()")]
pub struct ExternalMessage {
    pub origin: ExternalMessageType,
    pub id: MessageId,
    pub timestamp: Timestamp,
    pub values: Vec<MessagePart>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum ExternalMessageType {
    Email(String),
    Twitter(String),
    Matrix(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageId(u64);

impl From<u64> for MessageId {
    fn from(val: u64) -> Self {
        MessageId(val)
    }
}

impl From<u32> for MessageId {
    fn from(val: u32) -> Self {
        MessageId::from(val as u64)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
// Uses signed integer to make it MongoDb (driver) friendly.
pub struct Timestamp(i64);

impl Timestamp {
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};

        let start = SystemTime::now();
        let time = start
            .duration_since(UNIX_EPOCH)
            .expect("Failed to calculate UNIX time")
            .as_secs();

        Timestamp(time as i64)
    }
    pub fn raw(&self) -> i64 {
        self.0
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MessagePart(String);

impl From<String> for MessagePart {
    fn from(val: String) -> Self {
        MessagePart(val)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Notification {
    #[serde(rename = "type")]
    ty: NotificationType,
    message: NotificationMessage,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Event {
    pub timestamp: Timestamp,
    pub id: i64,
    pub event: NotificationMessage,
}

impl Event {
    pub fn new(event: NotificationMessage, id: i64) -> Self {
        Event {
            timestamp: Timestamp::now(),
            id: id,
            event: event,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Message)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
#[rtype(result = "()")]
// TODO: Rename
// TODO: Migrate to enum-structs.
pub enum NotificationMessage {
    FieldVerified(IdentityContext, IdentityFieldValue),
    FieldVerificationFailed(IdentityContext, IdentityFieldValue),
    IdentityFullyVerified(IdentityContext),
    JudgementProvided(IdentityContext),
    // TODO: Make use of this
    NotSupported(IdentityContext, IdentityFieldValue),
}

impl NotificationMessage {
    pub fn context(&self) -> &IdentityContext {
        use NotificationMessage::*;

        match self {
            FieldVerified(ctx, _) => ctx,
            FieldVerificationFailed(ctx, _) => ctx,
            IdentityFullyVerified(ctx) => ctx,
            JudgementProvided(ctx) => ctx,
            NotSupported(ctx, _) => ctx,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityJudged {
    context: IdentityContext,
    timestamp: Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web_actors::ws::Message;
    use serde::Serialize;

    impl IdentityContext {
        pub fn alice() -> Self {
            IdentityContext {
                address: ChainAddress(
                    "1a2YiGNu1UUhJtihq8961c7FZtWGQuWDVMWTNBKJdmpGhZP".to_string(),
                ),
                chain: ChainName::Polkadot,
            }
        }
        pub fn bob() -> Self {
            IdentityContext {
                address: ChainAddress(
                    "1b3NhsSEqWSQwS6nPGKgCrSjv9Kp13CnhraLV5Coyd8ooXB".to_string(),
                ),
                chain: ChainName::Polkadot,
            }
        }
        pub fn eve() -> Self {
            IdentityContext {
                address: ChainAddress(
                    "1cNyFSmLW4ofr7xh38za6JxLFxcu548LPcfc1E6L9r57SE3".to_string(),
                ),
                chain: ChainName::Polkadot,
            }
        }
    }

    impl JudgementState {
        pub fn alice() -> Self {
            JudgementState {
                context: IdentityContext::alice(),
                is_fully_verified: false,
                completion_timestamp: None,
                fields: vec![
                    IdentityField::new(IdentityFieldValue::DisplayName("Alice".to_string()), false),
                    IdentityField::new(
                        IdentityFieldValue::Email("alice@email.com".to_string()),
                        true,
                    ),
                    IdentityField::new(IdentityFieldValue::Twitter("@alice".to_string()), false),
                    IdentityField::new(
                        IdentityFieldValue::Matrix("@alice:matrix.org".to_string()),
                        true,
                    ),
                ],
            }
        }
        pub fn bob() -> Self {
            JudgementState {
                context: IdentityContext::bob(),
                is_fully_verified: false,
                completion_timestamp: None,
                fields: vec![
                    IdentityField::new(IdentityFieldValue::DisplayName("Bob".to_string()), false),
                    IdentityField::new(
                        IdentityFieldValue::Email("bob@email.com".to_string()),
                        true,
                    ),
                    IdentityField::new(IdentityFieldValue::Twitter("@bob".to_string()), false),
                    IdentityField::new(
                        IdentityFieldValue::Matrix("@bob:matrix.org".to_string()),
                        true,
                    ),
                ],
            }
        }
        pub fn eve() -> Self {
            JudgementState {
                context: IdentityContext::eve(),
                is_fully_verified: false,
                completion_timestamp: None,
                fields: vec![
                    IdentityField::new(IdentityFieldValue::DisplayName("Eve".to_string()), false),
                    IdentityField::new(
                        IdentityFieldValue::Email("eve@email.com".to_string()),
                        true,
                    ),
                    IdentityField::new(IdentityFieldValue::Twitter("@eve".to_string()), false),
                    IdentityField::new(
                        IdentityFieldValue::Matrix("@eve:matrix.org".to_string()),
                        true,
                    ),
                ],
            }
        }
        pub fn blank_second_challenge(&mut self) {
            self.fields.iter_mut().for_each(|field| {
                field.second_expected_challenge = None;
            })
        }
    }

    impl From<ExternalMessageType> for IdentityFieldValue {
        fn from(val: ExternalMessageType) -> Self {
            match val {
                ExternalMessageType::Email(n) => IdentityFieldValue::Email(n),
                ExternalMessageType::Twitter(n) => IdentityFieldValue::Twitter(n),
                ExternalMessageType::Matrix(n) => IdentityFieldValue::Matrix(n),
            }
        }
    }
}

use actix::Message;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityContext {
    pub chain_address: ChainAddress,
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
    pub second_expected_challenge: Option<SecondExpectedChallenge>,
    pub is_verified: bool,
    pub failed_attempts: isize,
}

impl IdentityField {
    pub fn new(val: IdentityFieldValue, second_challenge: bool) -> Self {
        IdentityField {
            value: val,
            expected_challenge: ExpectedChallenge::random(),
            second_expected_challenge: {
                if second_challenge {
                    Some(SecondExpectedChallenge::random())
                } else {
                    None
                }
            },
            is_verified: false,
            failed_attempts: 0,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExpectedChallenge(String);

impl ExpectedChallenge {
    pub fn random() -> Self {
        use rand::{thread_rng, Rng};

        let random: [u8; 16] = thread_rng().gen();
        ExpectedChallenge(hex::encode(random))
    }
    #[cfg(test)]
    pub fn into_message_parts(&self) -> Vec<MessagePart> {
        vec![self.0.clone().into()]
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecondExpectedChallenge(String);

impl SecondExpectedChallenge {
    pub fn random() -> Self {
        SecondExpectedChallenge(ExpectedChallenge::random().0)
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
    pub fields: Vec<IdentityField>,
}

impl JudgementState {
    pub fn check_field_verifications(&self) -> bool {
        self.fields.iter().all(|field| field.is_verified)
    }
    #[cfg(test)]
    pub fn get_field<'a>(&'a self, ty: &IdentityFieldValue) -> &'a IdentityField {
        self.fields.iter().find(|field| &field.value == ty).unwrap()
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

impl ExternalMessage {
    pub fn contains_challenge(&self, challenge: &ExpectedChallenge) -> bool {
        for value in &self.values {
            if value.0.contains(&challenge.0) {
                return true;
            }
        }

        false
    }
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
pub struct Timestamp(u64);

impl Timestamp {
    pub fn now() -> Self {
        Timestamp(0)
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
    timestamp: Timestamp,
    event: NotificationMessage,
}

impl From<NotificationMessage> for Event {
    fn from(val: NotificationMessage) -> Self {
        Event {
            // TODO:
            timestamp: Timestamp(0),
            event: val,
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
    FieldVerificationFailed(IdentityFieldValue),
    JudgementProvided(IdentityContext),
    // TODO: Make use of this
    NotSupported(IdentityFieldValue),
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
                chain_address: ChainAddress(
                    "1a2YiGNu1UUhJtihq8961c7FZtWGQuWDVMWTNBKJdmpGhZP".to_string(),
                ),
                chain: ChainName::Polkadot,
            }
        }
        pub fn bob() -> Self {
            IdentityContext {
                chain_address: ChainAddress(
                    "1b3NhsSEqWSQwS6nPGKgCrSjv9Kp13CnhraLV5Coyd8ooXB".to_string(),
                ),
                chain: ChainName::Polkadot,
            }
        }
        pub fn eve() -> Self {
            IdentityContext {
                chain_address: ChainAddress(
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
}

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
    value: IdentityFieldValue,
    challenge: ChallengeType,
    // TODO: Change this to usize.
    failed_attempts: isize,
}

impl IdentityField {
    pub fn value(&self) -> &IdentityFieldValue {
        &self.value
    }
    pub fn challenge(&self) -> &ChallengeType {
        &self.challenge
    }
    pub fn challenge_mut(&mut self) -> &mut ChallengeType {
        &mut self.challenge
    }
    // TODO: Move to tests module.
    #[cfg(test)]
    pub fn expected_message(&self) -> &ExpectedMessage {
        match &self.challenge {
            ChallengeType::ExpectedMessage {
                expected,
                second: _,
            } => expected,
            _ => panic!(),
        }
    }
    // TODO: Move to tests module.
    #[cfg(test)]
    pub fn expected_message_mut(&mut self) -> &mut ExpectedMessage {
        match &mut self.challenge {
            ChallengeType::ExpectedMessage {
                ref mut expected,
                second,
            } => expected,
            _ => panic!(),
        }
    }
    #[cfg(test)]
    pub fn expected_second(&self) -> &Option<ExpectedMessage> {
        match &self.challenge {
            ChallengeType::ExpectedMessage { expected, second } => second,
            _ => panic!(),
        }
    }
    #[cfg(test)]
    pub fn expected_second_mut(&mut self) -> &mut Option<ExpectedMessage> {
        match &mut self.challenge {
            ChallengeType::ExpectedMessage {
                expected,
                ref mut second,
            } => second,
            _ => panic!(),
        }
    }
    #[cfg(test)]
    pub fn failed_attempts_mut(&mut self) -> &mut isize {
        &mut self.failed_attempts
    }
}

impl IdentityField {
    pub fn new(val: IdentityFieldValue) -> Self {
        use IdentityFieldValue::*;

        let challenge = {
            match val {
                LegalName(_) => ChallengeType::Unsupported,
                Web(_) => ChallengeType::Unsupported,
                PGPFingerprint(_) => ChallengeType::Unsupported,
                Image(_) => ChallengeType::Unsupported,
                Additional(_) => ChallengeType::Unsupported,
                DisplayName(_) => ChallengeType::BackgroundCheck { passed: false },
                Email(_) => ChallengeType::ExpectedMessage {
                    expected: ExpectedMessage::random(),
                    second: Some(ExpectedMessage::random()),
                },
                Twitter(_) => ChallengeType::ExpectedMessage {
                    expected: ExpectedMessage::random(),
                    second: None,
                },
                Matrix(_) => ChallengeType::ExpectedMessage {
                    expected: ExpectedMessage::random(),
                    second: None,
                },
            }
        };

        IdentityField {
            value: val,
            challenge: challenge,
            failed_attempts: 0,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "challenge_type", content = "content")]
pub enum ChallengeType {
    ExpectedMessage {
        expected: ExpectedMessage,
        second: Option<ExpectedMessage>,
    },
    BackgroundCheck {
        passed: bool,
    },
    Unsupported,
}

impl ChallengeType {
    pub fn is_verified(&self) -> bool {
        match self {
            ChallengeType::ExpectedMessage { expected, second } => {
                if let Some(second) = second {
                    expected.is_verified && second.is_verified
                } else {
                    expected.is_verified
                }
            }
            ChallengeType::BackgroundCheck { passed } => *passed,
            ChallengeType::Unsupported => false,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExpectedMessage {
    pub value: String,
    pub is_verified: bool,
}

impl ExpectedMessage {
    pub fn random() -> Self {
        use rand::{thread_rng, Rng};

        let random: [u8; 16] = thread_rng().gen();
        ExpectedMessage {
            value: hex::encode(random),
            is_verified: false,
        }
    }
    pub fn verify_message(&mut self, message: &ExternalMessage) -> bool {
        for value in &message.values {
            if value.0.contains(&self.value) {
                self.set_verified();
                return true;
            }
        }

        false
    }
    pub fn set_verified(&mut self) {
        self.is_verified = true;
    }
    #[cfg(test)]
    pub fn to_message_parts(&self) -> Vec<MessagePart> {
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
    // TODO: Rename
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

// TODO: Should those fields be public?
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JudgementStateBlanked {
    pub context: IdentityContext,
    pub is_fully_verified: bool,
    pub inserted_timestamp: Timestamp,
    pub completion_timestamp: Option<Timestamp>,
    pub fields: Vec<IdentityFieldBlanked>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityFieldBlanked {
    value: IdentityFieldValue,
    challenge: ChallengeTypeBlanked,
    // TODO: Change this to usize.
    failed_attempts: isize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "challenge_type", content = "content")]
pub enum ChallengeTypeBlanked {
    ExpectedMessage {
        expected: ExpectedMessage,
        second: Option<ExpectedMessageBlanked>,
    },
    BackgroundCheck {
        passed: bool,
    },
    Unsupported,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExpectedMessageBlanked {
    // IMPORTANT: This value is blanked.
    // pub value: String,
    pub is_verified: bool,
}

impl From<JudgementState> for JudgementStateBlanked {
    fn from(s: JudgementState) -> Self {
        JudgementStateBlanked {
            context: s.context,
            is_fully_verified: s.is_fully_verified,
            inserted_timestamp: s.inserted_timestamp,
            completion_timestamp: s.completion_timestamp,
            fields: s
                .fields
                .into_iter()
                .map(|f| IdentityFieldBlanked {
                    value: f.value,
                    challenge: {
                        match f.challenge {
                            ChallengeType::ExpectedMessage { expected, second } => {
                                ChallengeTypeBlanked::ExpectedMessage {
                                    expected: expected,
                                    second: second.map(|s| ExpectedMessageBlanked {
                                        is_verified: s.is_verified,
                                    }),
                                }
                            }
                            ChallengeType::BackgroundCheck { passed } => {
                                ChallengeTypeBlanked::BackgroundCheck { passed: passed }
                            }
                            ChallengeType::Unsupported => ChallengeTypeBlanked::Unsupported,
                        }
                    },
                    failed_attempts: f.failed_attempts,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JudgementState {
    pub context: IdentityContext,
    pub is_fully_verified: bool,
    pub inserted_timestamp: Timestamp,
    pub completion_timestamp: Option<Timestamp>,
    pub fields: Vec<IdentityField>,
}

impl JudgementState {
    pub fn is_fully_verified(&self) -> bool {
        self.fields
            .iter()
            .all(|field| field.challenge.is_verified())
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
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
        use std::time::{SystemTime, UNIX_EPOCH};

        let start = SystemTime::now();
        let time = start
            .duration_since(UNIX_EPOCH)
            .expect("Failed to calculate UNIX time")
            .as_secs();

        Timestamp(time)
    }
    pub fn raw(&self) -> u64 {
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
    pub id: u64,
    pub event: NotificationMessage,
}

impl Event {
    pub fn new(event: NotificationMessage, id: u64) -> Self {
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
    SecondFieldVerified(IdentityContext, IdentityFieldValue),
    SecondFieldVerificationFailed(IdentityContext, IdentityFieldValue),
    AwaitingSecondChallenge(IdentityContext, IdentityFieldValue),
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
            SecondFieldVerified(ctx, _) => ctx,
            SecondFieldVerificationFailed(ctx, _) => ctx,
            AwaitingSecondChallenge(ctx, _) => ctx,
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
                inserted_timestamp: Timestamp::now(),
                completion_timestamp: None,
                fields: vec![
                    IdentityField::new(IdentityFieldValue::DisplayName("Alice".to_string())),
                    IdentityField::new(IdentityFieldValue::Email("alice@email.com".to_string())),
                    IdentityField::new(IdentityFieldValue::Twitter("@alice".to_string())),
                    IdentityField::new(IdentityFieldValue::Matrix("@alice:matrix.org".to_string())),
                ],
            }
        }
        pub fn alice_add_unsupported(&mut self) {
            // Prevent duplicates.
            self.remove_unsupported();

            self.fields.append(&mut vec![
                IdentityField::new(IdentityFieldValue::LegalName("Alice Cooper".to_string())),
                IdentityField::new(IdentityFieldValue::Web("alice.com".to_string())),
            ])
        }
        pub fn remove_unsupported(&mut self) {
            self.fields.retain(|field| match field.value {
                IdentityFieldValue::LegalName(_) => false,
                IdentityFieldValue::Web(_) => false,
                _ => true,
            })
        }
        pub fn bob() -> Self {
            JudgementState {
                context: IdentityContext::bob(),
                is_fully_verified: false,
                inserted_timestamp: Timestamp::now(),
                completion_timestamp: None,
                fields: vec![
                    IdentityField::new(IdentityFieldValue::DisplayName("Bob".to_string())),
                    IdentityField::new(IdentityFieldValue::Email("bob@email.com".to_string())),
                    IdentityField::new(IdentityFieldValue::Twitter("@bob".to_string())),
                    IdentityField::new(IdentityFieldValue::Matrix("@bob:matrix.org".to_string())),
                ],
            }
        }
        pub fn eve() -> Self {
            JudgementState {
                context: IdentityContext::eve(),
                is_fully_verified: false,
                inserted_timestamp: Timestamp::now(),
                completion_timestamp: None,
                fields: vec![
                    IdentityField::new(IdentityFieldValue::DisplayName("Eve".to_string())),
                    IdentityField::new(IdentityFieldValue::Email("eve@email.com".to_string())),
                    IdentityField::new(IdentityFieldValue::Twitter("@eve".to_string())),
                    IdentityField::new(IdentityFieldValue::Matrix("@eve:matrix.org".to_string())),
                ],
            }
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

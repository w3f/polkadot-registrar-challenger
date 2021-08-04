use actix::Message;

use crate::actors::connector::DisplayNameEntry;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityContext {
    pub address: ChainAddress,
    pub chain: ChainName,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChainAddress(String);

impl ChainAddress {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainName {
    Polkadot,
    Kusama,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityField {
    pub value: IdentityFieldValue,
    pub challenge: ChallengeType,
    // TODO: Change this to usize.
    pub failed_attempts: isize,
}

impl IdentityField {
    // TODO: deprecate this.
    pub fn value(&self) -> &IdentityFieldValue {
        &self.value
    }
    // TODO: deprecate this.
    pub fn challenge(&self) -> &ChallengeType {
        &self.challenge
    }
    // TODO: deprecate this.
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
    #[cfg(test)]
    pub fn expected_second(&self) -> &ExpectedMessage {
        match &self.challenge {
            ChallengeType::ExpectedMessage {
                expected: _,
                second,
            } => second.as_ref().unwrap(),
            _ => panic!(),
        }
    }
    // TODO: Move to tests module.
    #[cfg(test)]
    pub fn expected_message_mut(&mut self) -> &mut ExpectedMessage {
        match &mut self.challenge {
            ChallengeType::ExpectedMessage {
                ref mut expected,
                second: _,
            } => expected,
            _ => panic!(),
        }
    }
    // TODO: Move to tests module.
    #[cfg(test)]
    pub fn expected_second_mut(&mut self) -> &mut ExpectedMessage {
        match &mut self.challenge {
            ChallengeType::ExpectedMessage {
                expected: _,
                ref mut second,
            } => second.as_mut().unwrap(),
            _ => panic!(),
        }
    }
    #[cfg(test)]
    // TODO: deprecate this.
    pub fn failed_attempts_mut(&mut self) -> &mut isize {
        &mut self.failed_attempts
    }
    #[cfg(test)]
    pub fn expected_display_name_check_mut(&mut self) -> (&mut bool, &mut Vec<DisplayNameEntry>) {
        match &mut self.challenge {
            ChallengeType::DisplayNameCheck { passed, violations } => (passed, violations),
            _ => panic!(),
        }
    }
}

// TODO: Should be `From`?
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
                DisplayName(_) => ChallengeType::DisplayNameCheck {
                    passed: false,
                    violations: vec![],
                },
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
#[serde(rename_all = "snake_case", tag = "type", content = "content")]
pub enum ChallengeType {
    ExpectedMessage {
        expected: ExpectedMessage,
        second: Option<ExpectedMessage>,
    },
    DisplayNameCheck {
        passed: bool,
        violations: Vec<DisplayNameEntry>,
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
            ChallengeType::DisplayNameCheck {
                passed,
                violations: _,
            } => *passed,
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JudgementStateBlanked {
    pub context: IdentityContext,
    pub is_fully_verified: bool,
    pub inserted_timestamp: Timestamp,
    pub completion_timestamp: Option<Timestamp>,
    pub judgement_submitted: bool,
    pub fields: Vec<IdentityFieldBlanked>,
}

impl JudgementStateBlanked {
    #[cfg(test)]
    pub fn get_field<'a>(&'a self, ty: &IdentityFieldValue) -> &'a IdentityFieldBlanked {
        self.fields.iter().find(|field| &field.value == ty).unwrap()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityFieldBlanked {
    value: IdentityFieldValue,
    challenge: ChallengeTypeBlanked,
    // TODO: Change this to usize.
    failed_attempts: isize,
}

impl IdentityFieldBlanked {
    #[cfg(test)]
    pub fn expected_message(&self) -> &ExpectedMessage {
        match &self.challenge {
            ChallengeTypeBlanked::ExpectedMessage {
                expected,
                second: _,
            } => expected,
            _ => panic!(),
        }
    }
    #[cfg(test)]
    pub fn expected_second(&self) -> &ExpectedMessageBlanked {
        match &self.challenge {
            ChallengeTypeBlanked::ExpectedMessage {
                expected: _,
                second,
            } => second.as_ref().unwrap(),
            _ => panic!(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "content")]
pub enum ChallengeTypeBlanked {
    ExpectedMessage {
        expected: ExpectedMessage,
        second: Option<ExpectedMessageBlanked>,
    },
    DisplayNameCheck {
        passed: bool,
        violations: Vec<DisplayNameEntry>,
    },
    Unsupported,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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
            judgement_submitted: s.judgement_submitted,
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
                            ChallengeType::DisplayNameCheck { passed, violations } => {
                                ChallengeTypeBlanked::DisplayNameCheck {
                                    passed: passed,
                                    violations: violations,
                                }
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
    pub judgement_submitted: bool,
    pub issue_judgement_at: Option<Timestamp>,
    pub fields: Vec<IdentityField>,
}

impl JudgementState {
    pub fn new(context: IdentityContext, fields: Vec<IdentityFieldValue>) -> Self {
        JudgementState {
            context: context,
            is_fully_verified: false,
            inserted_timestamp: Timestamp::now(),
            completion_timestamp: None,
            judgement_submitted: false,
            issue_judgement_at: None,
            fields: fields
                .into_iter()
                .map(|val| IdentityField::new(val))
                .collect(),
        }
    }
    pub fn is_fully_verified(&self) -> bool {
        self.fields
            .iter()
            .all(|field| field.challenge.is_verified())
    }
    pub fn display_name(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| match field.value() {
                IdentityFieldValue::DisplayName(_) => true,
                _ => false,
            })
            .map(|field| match field.value() {
                IdentityFieldValue::DisplayName(name) => name.as_str(),
                _ => panic!("Failed to get display name. This is a bug."),
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
    pub fn with_offset(offset: u64) -> Self {
        let now = Self::now();
        Timestamp(now.0 + offset)
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
pub struct Event {
    pub timestamp: Timestamp,
    pub event: NotificationMessage,
}

impl Event {
    pub fn new(event: NotificationMessage) -> Self {
        Event {
            timestamp: Timestamp::now(),
            event: event,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Message)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
#[rtype(result = "()")]
pub enum NotificationMessage {
    IdentityInserted {
        context: IdentityContext,
    },
    IdentityUpdated {
        context: IdentityContext,
    },
    FieldVerified {
        context: IdentityContext,
        field: IdentityFieldValue,
    },
    FieldVerificationFailed {
        context: IdentityContext,
        field: IdentityFieldValue,
    },
    SecondFieldVerified {
        context: IdentityContext,
        field: IdentityFieldValue,
    },
    SecondFieldVerificationFailed {
        context: IdentityContext,
        field: IdentityFieldValue,
    },
    AwaitingSecondChallenge {
        context: IdentityContext,
        field: IdentityFieldValue,
    },
    IdentityFullyVerified {
        context: IdentityContext,
    },
    JudgementProvided {
        context: IdentityContext,
    },
    // TODO: Make use of this
    NotSupported {
        context: IdentityContext,
        field: IdentityFieldValue,
    },
}

impl NotificationMessage {
    pub fn context(&self) -> &IdentityContext {
        use NotificationMessage::*;

        match self {
            IdentityInserted { context } => context,
            IdentityUpdated { context } => context,
            FieldVerified { context, field: _ } => context,
            FieldVerificationFailed { context, field: _ } => context,
            SecondFieldVerified { context, field: _ } => context,
            SecondFieldVerificationFailed { context, field: _ } => context,
            AwaitingSecondChallenge { context, field: _ } => context,
            IdentityFullyVerified { context } => context,
            JudgementProvided { context } => context,
            NotSupported { context, field: _ } => context,
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
    }

    // TODO: Use JudgementState::new().
    impl JudgementState {
        pub fn alice() -> Self {
            JudgementState {
                context: IdentityContext::alice(),
                is_fully_verified: false,
                inserted_timestamp: Timestamp::now(),
                completion_timestamp: None,
                judgement_submitted: false,
                issue_judgement_at: None,
                fields: vec![
                    IdentityField::new(IdentityFieldValue::DisplayName("Alice".to_string())),
                    IdentityField::new(IdentityFieldValue::Email("alice@email.com".to_string())),
                    IdentityField::new(IdentityFieldValue::Twitter("@alice".to_string())),
                    IdentityField::new(IdentityFieldValue::Matrix("@alice:matrix.org".to_string())),
                ],
            }
        }
        pub fn bob() -> Self {
            JudgementState {
                context: IdentityContext::bob(),
                is_fully_verified: false,
                inserted_timestamp: Timestamp::now(),
                completion_timestamp: None,
                judgement_submitted: false,
                issue_judgement_at: None,
                fields: vec![
                    IdentityField::new(IdentityFieldValue::DisplayName("Bob".to_string())),
                    IdentityField::new(IdentityFieldValue::Email("bob@email.com".to_string())),
                    IdentityField::new(IdentityFieldValue::Twitter("@bob".to_string())),
                    IdentityField::new(IdentityFieldValue::Matrix("@bob:matrix.org".to_string())),
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

    impl IdentityFieldValue {
        pub fn alice_email() -> Self {
            IdentityFieldValue::Email("alice@email.com".to_string())
        }
        pub fn alice_matrix() -> Self {
            IdentityFieldValue::Matrix("@alice:matrix.org".to_string())
        }
        pub fn alice_twitter() -> Self {
            IdentityFieldValue::Twitter("@alice".to_string())
        }
        pub fn bob_email() -> Self {
            IdentityFieldValue::Email("bob@email.com".to_string())
        }
        pub fn bob_matrix() -> Self {
            IdentityFieldValue::Matrix("@bob:matrix.org".to_string())
        }
        pub fn bob_twitter() -> Self {
            IdentityFieldValue::Twitter("@bob".to_string())
        }
    }

    impl From<&str> for ChainAddress {
        fn from(val: &str) -> Self {
            ChainAddress(val.to_string())
        }
    }
}

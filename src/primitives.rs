use crate::adapters::admin::RawFieldName;
use crate::connector::{AccountType, DisplayNameEntry, VerifiedEntry};
use actix::Message;
use std::collections::HashMap;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityContext {
    pub address: ChainAddress,
    pub chain: ChainName,
}

impl IdentityContext {
    pub fn new(addr: ChainAddress, network: ChainName) -> Self {
        IdentityContext {
            address: addr,
            chain: network,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChainAddress(String);

impl ChainAddress {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for ChainAddress {
    fn from(v: String) -> Self {
        ChainAddress(v)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainName {
    Polkadot,
    Kusama,
}

impl ChainName {
    pub fn as_str(&self) -> &str {
        match self {
            ChainName::Polkadot => "polkadot",
            ChainName::Kusama => "kusama",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityField {
    pub value: IdentityFieldValue,
    pub challenge: ChallengeType,
    pub failed_attempts: usize,
}

impl IdentityField {
    pub fn new(val: IdentityFieldValue) -> Self {
        use IdentityFieldValue::*;

        let challenge = {
            match val {
                LegalName(_) => ChallengeType::Unsupported { is_verified: None },
                Web(_) => ChallengeType::Unsupported { is_verified: None },
                PGPFingerprint(_) => ChallengeType::Unsupported { is_verified: None },
                Image(_) => ChallengeType::Unsupported { is_verified: None },
                Additional(_) => ChallengeType::Unsupported { is_verified: None },
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
            challenge,
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
    Unsupported {
        // For manual judgements via the admin interface.
        is_verified: Option<bool>,
    },
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
            ChallengeType::Unsupported { is_verified } => is_verified.unwrap_or(false),
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
    pub fn is_message_valid(&self, message: &ExternalMessage) -> bool {
        for value in &message.values {
            if value.0.contains(&self.value) {
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
    pub fn as_account_type(&self) -> (AccountType, String) {
        match self {
            IdentityFieldValue::LegalName(val) => (AccountType::LegalName, val.to_string()),
            IdentityFieldValue::DisplayName(val) => (AccountType::DisplayName, val.to_string()),
            IdentityFieldValue::Email(val) => (AccountType::Email, val.to_string()),
            IdentityFieldValue::Web(val) => (AccountType::Web, val.to_string()),
            IdentityFieldValue::Twitter(val) => (AccountType::Twitter, val.to_string()),
            IdentityFieldValue::Matrix(val) => (AccountType::Matrix, val.to_string()),
            IdentityFieldValue::PGPFingerprint(_) => (AccountType::PGPFingerprint, String::new()),
            IdentityFieldValue::Image(_) => (AccountType::Image, String::new()),
            IdentityFieldValue::Additional(_) => (AccountType::Additional, String::new()),
        }
    }
    pub fn matches_type(&self, ty: &AccountType, value: &str) -> bool {
        match (self, ty) {
            (IdentityFieldValue::LegalName(val), AccountType::LegalName) => val == value,
            (IdentityFieldValue::DisplayName(val), AccountType::DisplayName) => val == value,
            (IdentityFieldValue::Email(val), AccountType::Email) => val == value,
            (IdentityFieldValue::Web(val), AccountType::Web) => val == value,
            (IdentityFieldValue::Twitter(val), AccountType::Twitter) => val == value,
            (IdentityFieldValue::Matrix(val), AccountType::Matrix) => val == value,
            (IdentityFieldValue::PGPFingerprint(_), AccountType::PGPFingerprint) => true,
            (IdentityFieldValue::Image(_), AccountType::Image) => true,
            (IdentityFieldValue::Additional(_), AccountType::Additional) => true,
            _ => false,
        }
    }
    pub fn matches_origin(&self, message: &ExternalMessage) -> bool {
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

// The blanked judgement state sent to the frontend UI. Does not include the
// secondary challenge. NOTE: `JudgementState` could be converted to take a
// generic and `JudgementStateBlanked` could just be a type alias.
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct IdentityFieldBlanked {
    pub value: IdentityFieldValue,
    pub challenge: ChallengeTypeBlanked,
    failed_attempts: usize,
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
    Unsupported {
        // For manual judgements via the admin interface.
        is_verified: Option<bool>,
    },
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
                                    expected,
                                    second: second.map(|s| ExpectedMessageBlanked {
                                        is_verified: s.is_verified,
                                    }),
                                }
                            }
                            ChallengeType::DisplayNameCheck { passed, violations } => {
                                ChallengeTypeBlanked::DisplayNameCheck { passed, violations }
                            }
                            ChallengeType::Unsupported { is_verified } => {
                                ChallengeTypeBlanked::Unsupported { is_verified }
                            }
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
            context,
            is_fully_verified: false,
            inserted_timestamp: Timestamp::now(),
            completion_timestamp: None,
            judgement_submitted: false,
            issue_judgement_at: None,
            fields: fields.into_iter().map(IdentityField::new).collect(),
        }
    }
    pub fn check_full_verification(&self) -> bool {
        self.fields
            .iter()
            .all(|field| field.challenge.is_verified())
    }
    pub fn display_name(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| matches!(field.value, IdentityFieldValue::DisplayName(_)))
            .map(|field| match &field.value {
                IdentityFieldValue::DisplayName(name) => name.as_str(),
                _ => panic!("Failed to get display name. This is a bug."),
            })
    }
    pub fn has_same_fields_as(&self, other: &HashMap<AccountType, String>) -> bool {
        if other.len() != self.fields.len() {
            return false;
        }

        for (account, value) in other {
            let matches = self
                .fields
                .iter()
                .any(|field| field.value.matches_type(account, value));
            if !matches {
                return false;
            }
        }

        true
    }
    pub fn as_verified_entries(&self) -> Vec<VerifiedEntry> {
        let mut list = vec![];

        for field in &self.fields {
            let (account_ty, value) = field.value.as_account_type();
            list.push(VerifiedEntry { account_ty, value });
        }

        list
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
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
    pub fn max(self, other: Timestamp) -> Self {
        if self.0 >= other.0 {
            self
        } else {
            other
        }
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
    pub message: NotificationMessage,
}

impl Event {
    pub fn new(message: NotificationMessage) -> Self {
        Event {
            timestamp: Timestamp::now(),
            message,
        }
    }
}

impl From<NotificationMessage> for Event {
    fn from(val: NotificationMessage) -> Self {
        Event::new(val)
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
    ManuallyVerified {
        context: IdentityContext,
        field: RawFieldName,
    },
    FullManualVerification {
        context: IdentityContext,
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
            ManuallyVerified { context, field: _ } => context,
            FullManualVerification { context } => context,
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
                    IdentityField::new(IdentityFieldValue::ALICE_DISPLAY_NAME()),
                    IdentityField::new(IdentityFieldValue::ALICE_EMAIL()),
                    IdentityField::new(IdentityFieldValue::ALICE_TWITTER()),
                    IdentityField::new(IdentityFieldValue::ALICE_MATRIX()),
                ],
            }
        }
        pub fn get_field<'a>(&'a self, ty: &IdentityFieldValue) -> &'a IdentityField {
            self.fields.iter().find(|field| &field.value == ty).unwrap()
        }
        pub fn get_field_mut<'a>(&'a mut self, ty: &IdentityFieldValue) -> &'a mut IdentityField {
            self.fields
                .iter_mut()
                .find(|field| &field.value == ty)
                .unwrap()
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
        #[allow(non_snake_case)]
        pub fn ALICE_DISPLAY_NAME() -> Self {
            IdentityFieldValue::DisplayName("Alice".to_string())
        }
        #[allow(non_snake_case)]
        pub fn ALICE_EMAIL() -> Self {
            IdentityFieldValue::Email("alice@email.com".to_string())
        }
        #[allow(non_snake_case)]
        pub fn ALICE_MATRIX() -> Self {
            IdentityFieldValue::Matrix("@alice:matrix.org".to_string())
        }
        #[allow(non_snake_case)]
        pub fn ALICE_TWITTER() -> Self {
            IdentityFieldValue::Twitter("@alice".to_string())
        }
    }

    impl From<&str> for ChainAddress {
        fn from(val: &str) -> Self {
            ChainAddress(val.to_string())
        }
    }

    impl IdentityField {
        pub fn expected_message(&self) -> &ExpectedMessage {
            match &self.challenge {
                ChallengeType::ExpectedMessage {
                    expected,
                    second: _,
                } => expected,
                _ => panic!(),
            }
        }
        pub fn expected_message_mut(&mut self) -> &mut ExpectedMessage {
            match &mut self.challenge {
                ChallengeType::ExpectedMessage {
                    ref mut expected,
                    second: _,
                } => expected,
                _ => panic!(),
            }
        }
        pub fn expected_second(&self) -> &ExpectedMessage {
            match &self.challenge {
                ChallengeType::ExpectedMessage {
                    expected: _,
                    second,
                } => second.as_ref().unwrap(),
                _ => panic!(),
            }
        }
        pub fn expected_second_mut(&mut self) -> &mut ExpectedMessage {
            match &mut self.challenge {
                ChallengeType::ExpectedMessage {
                    expected: _,
                    ref mut second,
                } => second.as_mut().unwrap(),
                _ => panic!(),
            }
        }
        pub fn failed_attempts_mut(&mut self) -> &mut usize {
            &mut self.failed_attempts
        }
        // rename, without "expected"
        pub fn expected_display_name_check_mut(
            &mut self,
        ) -> (&mut bool, &mut Vec<DisplayNameEntry>) {
            match &mut self.challenge {
                ChallengeType::DisplayNameCheck { passed, violations } => (passed, violations),
                _ => panic!(),
            }
        }
        pub fn expected_unsupported_mut(&mut self) -> &mut Option<bool> {
            match &mut self.challenge {
                ChallengeType::Unsupported { is_verified } => is_verified,
                _ => panic!(),
            }
        }
    }

    #[test]
    fn has_same_fields_as() {
        let id = IdentityContext::alice();
        let accounts: HashMap<AccountType, String> = [
            (AccountType::LegalName, "Alice".to_string()),
            (AccountType::DisplayName, "alice".to_string()),
            (AccountType::Email, "alice@gmail.com".to_string()),
            (AccountType::Twitter, "@alice".to_string()),
        ]
        .into();

        let state =
            JudgementState::new(id, accounts.clone().into_iter().map(|a| a.into()).collect());

        assert!(state.has_same_fields_as(&accounts));

        let accounts_new: HashMap<AccountType, String> = [
            (AccountType::LegalName, "Alice".to_string()),
            (AccountType::DisplayName, "alice".to_string()),
            // Email changed
            (AccountType::Email, "alice2@gmail.com".to_string()),
            (AccountType::Twitter, "@alice".to_string()),
        ]
        .into();

        assert!(!state.has_same_fields_as(&accounts_new));
        assert!(state.has_same_fields_as(&accounts));

        let accounts_trimmed: HashMap<AccountType, String> = [
            (AccountType::LegalName, "Alice".to_string()),
            // Does not contain display name
            (AccountType::Email, "alice@gmail.com".to_string()),
            (AccountType::Twitter, "@alice".to_string()),
        ]
        .into();

        assert!(!state.has_same_fields_as(&accounts_trimmed));
        assert!(state.has_same_fields_as(&accounts));
    }
}

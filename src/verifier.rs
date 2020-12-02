use crate::adapters::VIOLATIONS_CAP;
use crate::comms::CommsVerifier;
use crate::manager::AccountStatus;
use crate::primitives::{
    Account, AccountType, Challenge, ChallengeStatus, NetworkAddress, Result, Signature,
};
use crate::Database;
use schnorrkel::sign::Signature as SchnorrkelSignature;
use std::fmt;

const INTRODUCTION_STR: &'static str = "\
    [!!] NEVER EXPOSE YOUR PRIVATE KEYS TO ANYONE [!!]\n\n\
    This contact address was discovered in the Polkadot on-chain naming system and \
    the issuer has requested the Web3 Registrar service to judge this account. \
    If you did not issue this request then just ignore this message.\n\n";

#[derive(Debug, Fail)]
pub enum VerifierError {
    #[fail(display = "This is not a valid signature output.")]
    InvalidSignature,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum VerifierMessage {
    InitMessage(String),
    InitMessageWithContext(String),
    ResponseValid(String),
    ResponseInvalid(String),
    NotifyViolation(String),
    InvalidFormat(String),
    Goodbye(String),
}

impl VerifierMessage {
    pub fn as_str(&self) -> &str {
        // Is there a nicer way to do this?
        use VerifierMessage::*;

        match self {
            InitMessage(msg) => &msg,
            InitMessageWithContext(msg) => &msg,
            ResponseValid(msg) => &msg,
            ResponseInvalid(msg) => &msg,
            NotifyViolation(msg) => &msg,
            InvalidFormat(msg) => &msg,
            Goodbye(msg) => &msg,
        }
    }
}

impl fmt::Display for VerifierMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Is there a nicer way to do this?
        use VerifierMessage::*;

        match self {
            InitMessage(msg) => write!(f, "{}", msg),
            InitMessageWithContext(msg) => write!(f, "{}", msg),
            ResponseValid(msg) => write!(f, "{}", msg),
            ResponseInvalid(msg) => write!(f, "{}", msg),
            NotifyViolation(msg) => write!(f, "{}", msg),
            InvalidFormat(msg) => write!(f, "{}", msg),
            Goodbye(msg) => write!(f, "{}", msg),
        }
    }
}

pub struct Verifier<'a> {
    challenges: &'a [(NetworkAddress, Challenge)],
    valid: Vec<(&'a NetworkAddress, &'a Challenge)>,
    invalid: Vec<(&'a NetworkAddress, &'a Challenge)>,
}

impl<'a> Verifier<'a> {
    pub fn new(challenges: &'a [(NetworkAddress, Challenge)]) -> Self {
        Verifier {
            challenges: challenges,
            valid: vec![],
            invalid: vec![],
        }
    }
    fn create_signature(&self, input: &str) -> Result<Signature> {
        Ok(Signature::from(
            SchnorrkelSignature::from_bytes(
                &hex::decode(input.replace("0x", ""))
                    .map_err(|_| VerifierError::InvalidSignature)?,
            )
            .map_err(|_| VerifierError::InvalidSignature)?,
        ))
    }
    pub fn verify(&mut self, response: &str) {
        let sig = if let Ok(sig) = self.create_signature(response) {
            sig
        } else {
            return;
        };

        for (network_address, challenge) in self.challenges {
            if challenge.verify_challenge(network_address.pub_key(), &sig) {
                self.valid.push((network_address, challenge));
            } else {
                self.invalid.push((network_address, challenge));
            }
        }
    }
    fn valid_verifications(&self) -> Vec<&'a NetworkAddress> {
        self.valid
            .iter()
            .map(|(account_address, _)| *account_address)
            .collect()
    }
    fn invalid_verifications(&self) -> Vec<&'a NetworkAddress> {
        self.invalid
            .iter()
            .map(|(account_address, _)| *account_address)
            .collect()
    }
    pub fn init_message_builder(&self, send_intro: bool) -> VerifierMessage {
        let mut message = String::new();

        if send_intro {
            message.push_str(INTRODUCTION_STR);
        }

        if self.challenges.len() > 1 {
            message.push_str(
                "Please sign each challenge with the corresponding address and send \
            back the signature in order to verify this account:\n",
            );
        } else {
            message.push_str(
                "Please sign the challenge with the corresponding address and send \
            back the signature in order to verify this account:\n",
            );
        }

        for (network_address, challenge) in self.challenges {
            message.push_str("\nADDRESS:\n");
            message.push_str(&format!("> {}", network_address.address().as_str()));
            message.push_str("\nCHALLENGE:\n");
            message.push_str(&format!("> {}", challenge.as_str()));
        }

        message.push_str(
            "\n\nPlease note that each account you specified (Matrix, Email, etc.) \
        has it's own challenge which you need to sign and send back via the corresponding channel. \
        Refer to the Polkadot Wiki guide: https://wiki.polkadot.network/docs/en/learn-registrar",
        );

        if send_intro {
            VerifierMessage::InitMessageWithContext(message)
        } else {
            VerifierMessage::InitMessage(message)
        }
    }
    pub fn response_message_builder(&self) -> VerifierMessage {
        let mut message = String::new();

        if self.valid.is_empty() {
            message.push_str("The signature is invalid. Refer to the Polkadot Wiki guide.");
            return VerifierMessage::ResponseInvalid(message);
        } else if self.valid.len() == 1 {
            message.push_str("The following address has been verified:\n")
        } else {
            message.push_str("The following addresses have been verified:\n")
        }

        for (network_address, _) in &self.valid {
            message.push_str("\nADDRESS:\n");
            message.push_str(&format!("> {}", network_address.address().as_str()));
        }

        if !self.invalid.is_empty() {
            message.push_str("\n\nPending/Unconfirmed address(-es) for this account:\n");

            for (network_address, challenge) in &self.invalid {
                message.push_str("\n- Address:\n");
                message.push_str(network_address.address().as_str());
                message.push_str("\n- Challenge:\n");
                message.push_str(challenge.as_str());
            }
        }

        VerifierMessage::ResponseValid(message)
    }
}

pub async fn verification_handler<'a>(
    verifier: &Verifier<'a>,
    db: &Database,
    comms: &CommsVerifier,
    account_ty: &AccountType,
) -> Result<()> {
    for network_address in verifier.valid_verifications() {
        debug!(
            "Valid verification for address: {}",
            network_address.address().as_str()
        );

        db.set_challenge_status(
            network_address.address(),
            account_ty,
            &ChallengeStatus::Accepted,
        )
        .await?;

        comms.notify_status_change(network_address.address().clone());
    }

    for network_address in verifier.invalid_verifications() {
        debug!(
            "Invalid verification for address: {}",
            network_address.address().as_str()
        );

        db.set_challenge_status(
            network_address.address(),
            account_ty,
            &ChallengeStatus::Rejected,
        )
        .await?;

        comms.notify_status_change(network_address.address().clone());
    }

    Ok(())
}

pub fn invalid_accounts_message(
    accounts: &[(AccountType, Account, AccountStatus)],
    violations: Option<Vec<Account>>,
    send_intro: bool,
) -> VerifierMessage {
    let mut message = if send_intro {
        String::from(INTRODUCTION_STR)
    } else {
        String::new()
    };

    message.push_str("Please note that the following information is invalid:\n\n");

    for (account_ty, account, status) in accounts {
        if account_ty == &AccountType::DisplayName {
            if let Some(violations) = violations.as_ref() {
                message.push_str(&format!(
                    "* \"{}\" (Display Name) is too similar to {}existing display {}:\n",
                    account.as_str(),
                    {
                        if violations.len() == 1 {
                            "an "
                        } else {
                            ""
                        }
                    },
                    {
                        if violations.len() == 1 {
                            "name"
                        } else {
                            "names"
                        }
                    }
                ));

                for violation in violations {
                    message.push_str(&format!("  * \"{}\"\n", violation.as_str()));
                }

                if violations.len() == VIOLATIONS_CAP {
                    message.push_str("  * etc.\n");
                }

                continue;
            }
        } else if status == &AccountStatus::Unsupported {
            message.push_str(&format!(
                "* {} judgements (\"{}\") are not supported by the registrar.\n",
                account_ty.to_string(),
                account.as_str(),
            ));
        } else {
            message.push_str(&format!(
                "* \"{}\" ({}) could not be reached.\n",
                account.as_str(),
                account_ty.to_string()
            ));
        }
    }

    message.push_str(
        "\nPlease update the on-chain identity data. Note that you DO NOT \
        have to issue a new `requestJudgement` extrinsic after the update.\n\n\
        Refer to the Polkadot Wiki guide: https://wiki.polkadot.network/docs/en/learn-registrar",
    );

    VerifierMessage::NotifyViolation(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_accounts_message_status_invalid() {
        let accounts = [
            (
                AccountType::Matrix,
                Account::from("@alice:matrix.org"),
                AccountStatus::Invalid,
            ),
            (
                AccountType::Email,
                Account::from("alice@example.com"),
                AccountStatus::Invalid,
            ),
        ];

        let res = invalid_accounts_message(&accounts, None, true);
        let txt = match res {
            VerifierMessage::NotifyViolation(txt) => txt,
            _ => panic!(),
        };

        assert_eq!(txt, "\
            [!!] NEVER EXPOSE YOUR PRIVATE KEYS TO ANYONE [!!]\n\n\
            This contact address was discovered in the Polkadot on-chain naming system and \
            the issuer has requested the Web3 Registrar service to judge this account. \
            If you did not issue this request then just ignore this message.\n\
            \n\
            Please note that the following information is invalid:\n\
            \n\
            * \"@alice:matrix.org\" (Matrix) could not be reached.\n\
            * \"alice@example.com\" (Email) could not be reached.\n\
            \n\
            Please update the on-chain identity data. Note that you DO NOT have to issue a new `requestJudgement` extrinsic after the update.\n\
            \n\
            Refer to the Polkadot Wiki guide: https://wiki.polkadot.network/docs/en/learn-registrar\
        ");
    }

    #[test]
    fn invalid_accounts_message_status_unsupported() {
        let accounts = [
            (
                AccountType::LegalName,
                Account::from("Alice Doe"),
                AccountStatus::Unsupported,
            ),
            (
                AccountType::Web,
                Account::from("alice.com"),
                AccountStatus::Unsupported,
            ),
        ];

        let res = invalid_accounts_message(&accounts, None, false);
        let txt = match res {
            VerifierMessage::NotifyViolation(txt) => txt,
            _ => panic!(),
        };

        println!("{}", txt);
        assert_eq!(txt, "\
            Please note that the following information is invalid:\n\
            \n\
            * Legal Name judgements (\"Alice Doe\") are not supported by the registrar.\n\
            * Web judgements (\"alice.com\") are not supported by the registrar.\n\
            \n\
            Please update the on-chain identity data. Note that you DO NOT have to issue a new `requestJudgement` extrinsic after the update.\n\
            \n\
            Refer to the Polkadot Wiki guide: https://wiki.polkadot.network/docs/en/learn-registrar\
        ");
    }

    #[test]
    fn invalid_accounts_message_status_both() {
        let accounts = [
            (
                AccountType::LegalName,
                Account::from("Alice Doe"),
                AccountStatus::Unsupported,
            ),
            (
                AccountType::Email,
                Account::from("alice@example.com"),
                AccountStatus::Invalid,
            ),
        ];

        let res = invalid_accounts_message(&accounts, None, false);
        let txt = match res {
            VerifierMessage::NotifyViolation(txt) => txt,
            _ => panic!(),
        };

        println!("{}", txt);
        assert_eq!(txt, "\
            Please note that the following information is invalid:\n\
            \n\
            * Legal Name judgements (\"Alice Doe\") are not supported by the registrar.\n\
            * \"alice@example.com\" (Email) could not be reached.\n\
            \n\
            Please update the on-chain identity data. Note that you DO NOT have to issue a new `requestJudgement` extrinsic after the update.\n\
            \n\
            Refer to the Polkadot Wiki guide: https://wiki.polkadot.network/docs/en/learn-registrar\
        ");
    }
}

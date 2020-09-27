use crate::primitives::{Challenge, NetworkAddress, PubKey, Result, Signature};
use schnorrkel::sign::Signature as SchnorrkelSignature;
use std::result::Result as StdResult;

#[derive(Debug, Fail)]
pub enum VerifierError {
    #[fail(display = "This is not a valid signature output.")]
    InvalidSignature,
    #[fail(
        display = "The signature is INVALID. Please sign the challenge with the key which belongs to the on-chain identity address."
    )]
    SignatureNok,
}

pub struct Verifier {
    pub_key: PubKey,
    challenge: Challenge,
}

impl Verifier {
    pub fn new(pub_key: PubKey, challenge: Challenge) -> Self {
        Verifier {
            pub_key: pub_key,
            challenge: challenge,
        }
    }
    pub fn verify(&self, response: &str) -> StdResult<String, VerifierError> {
        let sig = Signature::from(
            SchnorrkelSignature::from_bytes(
                &hex::decode(response.replace("0x", ""))
                    .map_err(|_| VerifierError::InvalidSignature)?,
            )
            .map_err(|_| VerifierError::InvalidSignature)?,
        );

        if self.challenge.verify_challenge(&self.pub_key, &sig) {
            Ok("The signature is VALID. This account is confirmed.".to_string())
        } else {
            Err(VerifierError::SignatureNok)
        }
    }
}

pub struct Verifier2<'a> {
    challenges: &'a [(NetworkAddress, Challenge)],
    valid: Vec<&'a NetworkAddress>,
}

impl<'a> Verifier2<'a> {
    pub fn new(challenges: &'a [(NetworkAddress, Challenge)]) -> Self {
        Verifier2 {
            challenges: challenges,
            valid: vec![],
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
                self.valid.push(network_address);
            }
        }
    }
    pub fn valid_verifications(&self) -> &Vec<&'a NetworkAddress> {
        &self.valid
    }
    pub fn invalid_verifications(&self) -> Vec<&'a NetworkAddress> {
        self.challenges
            .iter()
            .map(|(network_address, _)| network_address)
            .filter(|network_address| !self.valid.contains(network_address))
            .collect()
    }
    pub fn init_message_builder(&self) -> String {
        let message = String::new();
        message
    }
    pub fn response_message_builder(&self) -> String {
        let mut message = String::new();

        if self.valid.is_empty() {
            message.push_str("The signature is invalid for every pending address.");
        } else {
            message.push_str("The following address(-es) has/have been verified:\n")
        }

        for network_address in &self.valid {
            message.push_str(&format!("- {}", network_address.address().as_str()));
        }

        if !self.invalid_verifications().is_empty() {
            message.push_str("\n");
            message.push_str("Pending/Unconfirmed address(-es) for this account:\n");
        }

        for network_address in self.invalid_verifications() {
            message.push_str(&format!("- {}", network_address.address().as_str()));
        }

        message
    }
}

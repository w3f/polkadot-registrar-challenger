use crate::primitives::{Challenge, NetworkAddress, Result, Signature};
use schnorrkel::sign::Signature as SchnorrkelSignature;

#[derive(Debug, Fail)]
pub enum VerifierError {
    #[fail(display = "This is not a valid signature output.")]
    InvalidSignature,
}

pub struct Verifier2<'a> {
    challenges: &'a [(NetworkAddress, Challenge)],
    valid: Vec<(&'a NetworkAddress, &'a Challenge)>,
    invalid: Vec<(&'a NetworkAddress, &'a Challenge)>,
}

impl<'a> Verifier2<'a> {
    pub fn new(challenges: &'a [(NetworkAddress, Challenge)]) -> Self {
        Verifier2 {
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
    pub fn valid_verifications(&self) -> Vec<&'a NetworkAddress> {
        self.valid
            .iter()
            .map(|(account_address, _)| *account_address)
            .collect()
    }
    pub fn invalid_verifications(&self) -> Vec<&'a NetworkAddress> {
        self.invalid
            .iter()
            .map(|(account_address, _)| *account_address)
            .collect()
    }
    pub fn init_message_builder(&self, send_context: bool) -> String {
        let mut message = String::new();

        if send_context {
            message.push_str(
                "\
                [!!] NEVER EXPOSE YOUR PRIVATE KEYS TO ANYONE [!!]\n\n\
                This contact address was discovered in the Polkadot on-chain naming system and\
                the issuer has requested the Web3 Registrar service to judge this account.\
                If you did not issue this request then just ignore this message.\n\n\
            ",
            );
        }

        if self.challenges.len() > 1 {
            message.push_str("Please sign each challenge with the corresponding address:\n");
        } else {
            message.push_str("Please sign the challenge with the corresponding address:\n");
        }

        for (network_address, challenge) in self.challenges {
            message.push_str("\n- Address:\n");
            message.push_str(network_address.address().as_str());
            message.push_str("\n- Challenge:\n");
            message.push_str(challenge.as_str());
        }

        message.push_str("\n\nRefer to the Polkadot Wiki guide https://wiki.polkadot.network/");

        message
    }
    pub fn response_message_builder(&self) -> String {
        let mut message = String::new();

        if self.valid.is_empty() {
            message.push_str("The signature is invalid.");
        } else if self.valid.len() == 1 {
            message.push_str("The following address has been verified:\n")
        } else {
            message.push_str("The following addresses have been verified:\n")
        }

        for (network_address, challenge) in &self.valid {
            message.push_str("\n- Address:\n");
            message.push_str(network_address.address().as_str());
            message.push_str("\n- Challenge:\n");
            message.push_str(challenge.as_str());
        }

        if !self.invalid.is_empty() {
            message.push_str("\n\nPending/Unconfirmed address(-es) for this account:\n");

            for (network_address, challenge) in &self.invalid {
                message.push_str("\n- Address:\n");
                message.push_str(network_address.address().as_str());
                message.push_str("\n- Challenge:\n");
                message.push_str(challenge.as_str());
            }

            message.push_str("\n\nRefer to the Polkadot Wiki guide");
        }

        message
    }
}

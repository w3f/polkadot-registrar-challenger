use crate::primitives::{Challenge, PubKey, Signature};
use schnorrkel::sign::Signature as SchnorrkelSignature;

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
    pub fn verify(&self, response: &str) -> Result<String, VerifierError> {
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

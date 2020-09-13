#[macro_use]
extern crate futures;
#[macro_use]
extern crate async_trait;
#[macro_use]
extern crate serde;

use rand::{thread_rng, Rng};
use schnorrkel::keys::PublicKey as SchnorrkelPubKey;
use schnorrkel::sign::Signature as SchnorrkelSignature;
use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

mod adapters;
mod identity;

#[derive(Eq, PartialEq)]
struct PubKey(SchnorrkelPubKey);
struct Signature(SchnorrkelSignature);
#[derive(Eq, PartialEq, Serialize, Deserialize)]
struct Address(String);

#[derive(Eq, PartialEq, Serialize, Deserialize)]
enum AddressType {
    Email(Address),
    Web(Address),
    Twitter(Address),
    Riot(Address),
}

impl AddressType {
    fn raw(&self) -> &Address {
        use AddressType::*;

        match self {
            Email(a) | Web(a) | Twitter(a) | Riot(a) => a,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Challenge(String);

impl Challenge {
    fn gen_random() -> Challenge {
        let random: [u8; 16] = thread_rng().gen();
        Challenge(hex::encode(random))
    }
}

impl Serialize for PubKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&hex::encode(self.0.to_bytes()))
    }
}

impl<'de> Deserialize<'de> for PubKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_str = <String as Deserialize>::deserialize(deserializer)?;
        Ok(PubKey(
            SchnorrkelPubKey::from_bytes(
                &hex::decode(hex_str)
                    .map_err(|_| SerdeError::custom("failed to decode public key from hex"))?,
            )
            .map_err(|_| SerdeError::custom("failed creating public key from bytes"))?,
        ))
    }
}

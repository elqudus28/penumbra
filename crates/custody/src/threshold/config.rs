use anyhow::Result;
use ark_ff::UniformRand;
use decaf377::Fq;
use decaf377_frost as frost;
use ed25519_consensus::{SigningKey, VerificationKey};
use penumbra_keys::{keys::NullifierKey, FullViewingKey};
use rand_core::CryptoRngCore;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct Config {
    threshold: u16,
    fvk: FullViewingKey,
    spend_key_share: frost::keys::SigningShare,
    signing_key: SigningKey,
    verifying_shares: HashMap<VerificationKey, frost::keys::VerifyingShare>,
}

impl Config {
    pub fn deal(mut rng: &mut impl CryptoRngCore, t: u16, n: u16) -> Result<Vec<Self>> {
        let signing_keys = (0..n)
            .map(|_| {
                let sk = SigningKey::new(&mut rng);
                let pk = sk.verification_key();
                (pk, sk)
            })
            .collect::<HashMap<_, _>>();
        let identifiers = signing_keys
            .keys()
            .cloned()
            .map(|pk| Ok((pk, frost::Identifier::derive(pk.as_bytes().as_slice())?)))
            .collect::<Result<HashMap<_, _>, frost::Error>>()?;

        let (share_map, public_key_package) = frost::keys::generate_with_dealer(
            n,
            t,
            frost::keys::IdentifierList::Custom(
                identifiers.values().cloned().collect::<Vec<_>>().as_slice(),
            ),
            &mut rng,
        )?;
        let verifying_shares = signing_keys
            .keys()
            .map(|pk| {
                let identifier = frost::Identifier::derive(pk.to_bytes().as_slice())
                    .expect("should be able to derive identifier");
                (pk.clone(), public_key_package.signer_pubkeys()[&identifier])
            })
            .collect::<HashMap<_, _>>();
        // Okay, this conversion is a bit of a hack, but it should work...
        // It's a hack cause we're going via the serialization, but, you know, that should be fine.
        let fvk = FullViewingKey::from_components(
            public_key_package
                .group_public()
                .serialize()
                .as_slice()
                .try_into()
                .expect("conversion of a group element to a VerifyingKey should not fail"),
            NullifierKey(Fq::rand(rng)),
        );

        Ok(signing_keys
            .into_iter()
            .map(|(verification_key, signing_key)| {
                let identifier = identifiers[&verification_key];
                let signing_share = share_map[&identifier].value().clone();
                Self {
                    threshold: t,
                    signing_key,
                    fvk: fvk.clone(),
                    spend_key_share: signing_share,
                    verifying_shares: verifying_shares.clone(),
                }
            })
            .collect())
    }

    pub fn threshold(&self) -> u16 {
        self.threshold
    }

    fn group_public(&self) -> frost::keys::VerifyingKey {
        frost::keys::VerifyingKey::deserialize(
            self.fvk.spend_verification_key().to_bytes().to_vec(),
        )
        .expect("should be able to parse out VerifyingKey from FullViewingKey")
    }

    pub fn key_package(&self) -> frost::keys::KeyPackage {
        let identifier =
            frost::Identifier::derive(&self.signing_key.verification_key().as_bytes().as_slice())
                .expect("deriving our identifier should not fail");

        frost::keys::KeyPackage::new(
            identifier,
            self.spend_key_share,
            self.spend_key_share.into(),
            self.group_public(),
            self.threshold,
        )
    }

    pub fn public_key_package(&self) -> frost::keys::PublicKeyPackage {
        let signer_pubkeys = self
            .verifying_shares
            .iter()
            .map(|(vk, share)| {
                (
                    frost::Identifier::derive(vk.to_bytes().as_slice())
                        .expect("deriving an identifier should not fail"),
                    share.clone(),
                )
            })
            .collect();
        frost::keys::PublicKeyPackage::new(signer_pubkeys, self.group_public())
    }

    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    pub fn fvk(&self) -> &FullViewingKey {
        &self.fvk
    }

    pub fn verification_keys(&self) -> HashSet<VerificationKey> {
        self.verifying_shares.keys().cloned().collect()
    }
}

use crate::{
    asset,
    keys::NullifierKey,
    note::{self, NOTECOMMIT_DOMAIN_SEP},
    nullifier::NULLIFIER_DOMAIN_SEP,
    Address, Amount, Note, Nullifier, Value,
};
use decaf377_rdsa::{SpendAuth, VerificationKey};
use once_cell::sync::Lazy;
use penumbra_tct as tct;

use ark_ff::PrimeField;
use ark_r1cs_std::prelude::*;
use ark_relations::r1cs::{ConstraintSystemRef, SynthesisError};
use decaf377::{
    r1cs::{ElementVar, FqVar},
    Element, FieldExt, Fq, Fr,
};

pub(crate) static SPENDAUTH_BASEPOINT: Lazy<Element> = Lazy::new(decaf377::basepoint);

/// Check the element is not identity.
pub(crate) fn element_not_identity(
    cs: ConstraintSystemRef<Fq>,
    enforce: &Boolean<Fq>,
    // Witness
    element: ElementVar,
) -> Result<(), SynthesisError> {
    let identity = ElementVar::new_constant(cs, decaf377::Element::default())?;
    identity.conditional_enforce_not_equal(&element, enforce)?;
    Ok(())
}

pub struct AmountVar {
    cs: ConstraintSystemRef<Fq>,
    amount: FqVar,
}

impl AllocVar<Amount, Fq> for AmountVar {
    fn new_variable<T: std::borrow::Borrow<Amount>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        let amount1 = f()?;
        let amount: Amount = *amount1.borrow();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => unimplemented!(),
            AllocationMode::Witness => {
                let inner_amount_var = FqVar::new_witness(cs.clone(), || Ok(Fq::from(amount)))?;
                Ok(Self {
                    cs,
                    amount: inner_amount_var,
                })
            }
        }
    }
}

pub struct AssetIdVar {
    cs: ConstraintSystemRef<Fq>,
    asset_id: FqVar,
}

impl AllocVar<asset::Id, Fq> for AssetIdVar {
    fn new_variable<T: std::borrow::Borrow<asset::Id>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        let asset_id1 = f()?;
        let asset_id: asset::Id = *asset_id1.borrow();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => unimplemented!(),
            AllocationMode::Witness => {
                let inner_asset_id_var = FqVar::new_witness(cs.clone(), || Ok(asset_id.0))?;
                Ok(Self {
                    cs,
                    asset_id: inner_asset_id_var,
                })
            }
        }
    }
}

pub struct ValueVar {
    cs: ConstraintSystemRef<Fq>,
    amount: AmountVar,
    asset_id: AssetIdVar,
}

impl AllocVar<Value, Fq> for ValueVar {
    fn new_variable<T: std::borrow::Borrow<Value>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        let value1 = f()?;
        let value: Value = *value1.borrow();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => unimplemented!(),
            AllocationMode::Witness => {
                let amount_var = AmountVar::new_witness(cs.clone(), || Ok(value.amount))?;
                let asset_id_var = AssetIdVar::new_witness(cs.clone(), || Ok(value.asset_id))?;
                Ok(Self {
                    cs,
                    amount: amount_var,
                    asset_id: asset_id_var,
                })
            }
        }
    }
}

impl ValueVar {
    pub fn amount(&self) -> FqVar {
        self.amount.amount.clone()
    }

    pub fn asset_id(&self) -> FqVar {
        self.asset_id.asset_id.clone()
    }
}

struct AddressVar {
    cs: ConstraintSystemRef<Fq>,
    // TODO: in some places, we'll want the diversified generator (and
    // transmission key) as a validated
    // curve point, in others we'll want it as the encoding.  which should we
    // pick as the "default" internal representation? for now, use both, and
    // over-constrain, then we can optimize later we could, e.g. have an enum {
    // Encoding, Element, EncodingAndElement } that does lazy
    // eval of constraints internal mutability on enum, and then have the
    // accessors take &mut self, and then either fetch the already-allocated
    // variable, or allocate it and mutate the internal state to do constraint
    // on demand ?
    diversified_generator: ElementVar,
    transmission_key: ElementVar,
    transmission_key_s: FqVar,
    // Output proof needs: diversified generator as element and does the elligator
    // map to get an Fq, transmission key as Fq
    // Spend proof needs: diversified generator as element and does the elligator
    // map to get an Fq, transmission key as Fq and element
    clue_key: FqVar,
}

impl AddressVar {
    pub fn diversified_generator(&self) -> ElementVar {
        self.diversified_generator.clone()
    }

    pub fn transmission_key(&self) -> ElementVar {
        self.transmission_key.clone()
    }

    pub fn transmission_key_s(&self) -> FqVar {
        self.transmission_key_s.clone()
    }

    pub fn clue_key(&self) -> FqVar {
        self.clue_key.clone()
    }
}

impl AllocVar<Address, Fq> for AddressVar {
    fn new_variable<T: std::borrow::Borrow<Address>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        let value1 = f()?;
        let address: Address = *value1.borrow();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => unimplemented!(),
            AllocationMode::Witness => {
                let diversified_generator: ElementVar =
                    AllocVar::<Element, Fq>::new_witness(cs.clone(), || {
                        Ok(address.diversified_generator().clone())
                    })?;
                let transmission_key_s =
                    FqVar::new_witness(cs.clone(), || Ok(address.transmission_key_s().clone()))?;
                // dbg!(decaf377::Encoding(address.transmission_key().0).vartime_decompress());
                let element_transmission_key = decaf377::Encoding(address.transmission_key().0)
                    .vartime_decompress()
                    .map_err(|_| SynthesisError::AssignmentMissing)?;
                let transmission_key: ElementVar =
                    AllocVar::<Element, Fq>::new_witness(cs.clone(), || {
                        Ok(element_transmission_key)
                    })?;
                let clue_key = FqVar::new_witness(cs.clone(), || {
                    Ok(Fq::from_le_bytes_mod_order(&address.clue_key().0[..]))
                })?;

                Ok(Self {
                    cs,
                    diversified_generator,
                    transmission_key_s,
                    transmission_key,
                    clue_key,
                })
            }
        }
    }
}

pub struct NoteVar {
    cs: ConstraintSystemRef<Fq>,
    value: ValueVar,
    note_blinding: FqVar,
    address: AddressVar,
}

impl NoteVar {
    pub fn amount(&self) -> FqVar {
        self.value.amount()
    }

    pub fn asset_id(&self) -> FqVar {
        self.value.asset_id()
    }

    pub fn note_blinding(&self) -> FqVar {
        self.note_blinding.clone()
    }

    pub fn diversified_generator(&self) -> ElementVar {
        self.address.diversified_generator.clone()
    }

    pub fn transmission_key(&self) -> ElementVar {
        self.address.transmission_key.clone()
    }

    pub fn transmission_key_s(&self) -> FqVar {
        self.address.transmission_key_s.clone()
    }

    pub fn clue_key(&self) -> FqVar {
        self.address.clue_key.clone()
    }
}

impl AllocVar<Note, Fq> for NoteVar {
    fn new_variable<T: std::borrow::Borrow<Note>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        // TODO: figure out how to use namespaces
        let ns = cs.into();
        let cs = ns.cs();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => unimplemented!(),
            AllocationMode::Witness => {
                let note1 = f()?;
                let note = note1.borrow();

                let note_blinding =
                    FqVar::new_witness(cs.clone(), || Ok(note.note_blinding().clone()))?;
                let value = ValueVar::new_witness(cs.clone(), || Ok(note.value().clone()))?;
                let address = AddressVar::new_witness(cs.clone(), || Ok(note.address().clone()))?;

                Ok(Self {
                    cs,
                    note_blinding,
                    value,
                    address,
                })
            }
        }
    }
}

pub struct NoteCommitmentVar {
    cs: ConstraintSystemRef<Fq>,
    inner: FqVar,
}

impl NoteCommitmentVar {
    pub fn inner(&self) -> FqVar {
        self.inner.clone()
    }
}

impl AllocVar<note::Commitment, Fq> for NoteCommitmentVar {
    fn new_variable<T: std::borrow::Borrow<note::Commitment>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => {
                let note_commitment1 = f()?;
                let note_commitment: note::Commitment = *note_commitment1.borrow();
                let inner = FqVar::new_input(cs.clone(), || Ok(note_commitment.0))?;

                Ok(Self { cs, inner })
            }
            AllocationMode::Witness => {
                let note_commitment1 = f()?;
                let note_commitment: note::Commitment = *note_commitment1.borrow();
                let inner = FqVar::new_witness(cs.clone(), || Ok(note_commitment.0))?;

                Ok(Self { cs, inner })
            }
        }
    }
}

impl NoteVar {
    pub fn commit(&self) -> Result<NoteCommitmentVar, SynthesisError> {
        let domain_sep = FqVar::new_constant(self.cs.clone(), *NOTECOMMIT_DOMAIN_SEP)?;
        let compressed_g_d = self.address.diversified_generator().compress_to_field()?;

        let commitment = poseidon377::r1cs::hash_6(
            self.cs.clone(),
            &domain_sep,
            (
                self.note_blinding.clone(),
                self.value.amount(),
                self.value.asset_id(),
                compressed_g_d,
                self.address.transmission_key_s(),
                self.address.clue_key(),
            ),
        )?;

        Ok(NoteCommitmentVar {
            cs: self.cs.clone(),
            inner: commitment,
        })
    }
}

impl EqGadget<Fq> for NoteCommitmentVar {
    fn is_eq(&self, other: &Self) -> Result<Boolean<Fq>, SynthesisError> {
        self.inner.is_eq(&other.inner)
    }
}

pub struct PositionVar {
    cs: ConstraintSystemRef<Fq>,
    pub inner: FqVar,
}

impl AllocVar<tct::Position, Fq> for PositionVar {
    fn new_variable<T: std::borrow::Borrow<tct::Position>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        let inner1 = f()?;
        let inner: tct::Position = *inner1.borrow();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => unimplemented!(),
            AllocationMode::Witness => Ok(Self {
                cs: cs.clone(),
                inner: FqVar::new_witness(cs.clone(), || Ok(Fq::from(u64::from(inner))))?,
            }),
        }
    }
}

pub struct NullifierKeyVar {
    cs: ConstraintSystemRef<Fq>,
    pub inner: FqVar,
}

impl AllocVar<NullifierKey, Fq> for NullifierKeyVar {
    fn new_variable<T: std::borrow::Borrow<NullifierKey>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        let inner1 = f()?;
        let inner: NullifierKey = *inner1.borrow();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => unimplemented!(),
            AllocationMode::Witness => Ok(Self {
                cs: cs.clone(),
                inner: FqVar::new_witness(cs, || Ok(inner.0))?,
            }),
        }
    }
}

impl NullifierKeyVar {
    pub fn derive_nullifier(
        &self,
        position: &PositionVar,
        note_commitment: &NoteCommitmentVar,
    ) -> Result<NullifierVar, SynthesisError> {
        let domain_sep = FqVar::new_constant(self.cs.clone(), *NULLIFIER_DOMAIN_SEP)?;
        let nullifier = poseidon377::r1cs::hash_3(
            self.cs.clone(),
            &domain_sep,
            (
                self.inner.clone(),
                note_commitment.inner.clone(),
                position.inner.clone(),
            ),
        )?;

        Ok(NullifierVar {
            cs: self.cs.clone(),
            inner: nullifier,
        })
    }
}

pub struct NullifierVar {
    cs: ConstraintSystemRef<Fq>,
    inner: FqVar,
}

impl AllocVar<Nullifier, Fq> for NullifierVar {
    fn new_variable<T: std::borrow::Borrow<Nullifier>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        let nullifier1 = f()?;
        let nullifier: Nullifier = *nullifier1.borrow();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => Ok(Self {
                cs: cs.clone(),
                inner: FqVar::new_input(cs.clone(), || Ok(nullifier.0))?,
            }),
            AllocationMode::Witness => unimplemented!(),
        }
    }
}

impl EqGadget<Fq> for NullifierVar {
    fn is_eq(&self, other: &Self) -> Result<Boolean<Fq>, SynthesisError> {
        self.inner.is_eq(&other.inner)
    }
}

pub struct RandomizedVerificationKey {
    cs: ConstraintSystemRef<Fq>,
    pub inner: ElementVar,
}

impl AllocVar<VerificationKey<SpendAuth>, Fq> for RandomizedVerificationKey {
    fn new_variable<T: std::borrow::Borrow<VerificationKey<SpendAuth>>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        let inner1 = f()?;
        let inner: VerificationKey<SpendAuth> = *inner1.borrow();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => {
                let point = decaf377::Encoding(*inner.as_ref())
                    .vartime_decompress()
                    .unwrap();
                let element_var: ElementVar =
                    AllocVar::<Element, Fq>::new_witness(cs.clone(), || Ok(point))?;
                Ok(Self {
                    cs: cs.clone(),
                    inner: element_var,
                })
            }
            AllocationMode::Witness => unimplemented!(),
        }
    }
}

impl RandomizedVerificationKey {
    pub fn compress_to_field(&self) -> Result<FqVar, SynthesisError> {
        self.inner.compress_to_field()
    }
}

impl EqGadget<Fq> for RandomizedVerificationKey {
    fn is_eq(&self, other: &Self) -> Result<Boolean<Fq>, SynthesisError> {
        let self_fq = self.inner.compress_to_field()?;
        let other_fq = self.inner.compress_to_field()?;
        self_fq.is_eq(&other_fq)
    }
}

pub struct AuthorizationKeyVar {
    cs: ConstraintSystemRef<Fq>,
    pub inner: ElementVar,
}

impl AllocVar<VerificationKey<SpendAuth>, Fq> for AuthorizationKeyVar {
    fn new_variable<T: std::borrow::Borrow<VerificationKey<SpendAuth>>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        let inner1 = f()?;
        let inner: VerificationKey<SpendAuth> = *inner1.borrow();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => unimplemented!(),
            AllocationMode::Witness => {
                let ak_point = decaf377::Encoding(*inner.as_ref())
                    .vartime_decompress()
                    .unwrap();
                let ak_element_var: ElementVar =
                    AllocVar::<Element, Fq>::new_witness(cs.clone(), || Ok(ak_point))?;
                Ok(Self {
                    cs: cs.clone(),
                    inner: ak_element_var,
                })
            }
        }
    }
}

impl AuthorizationKeyVar {
    pub fn compress_to_field(&self) -> Result<FqVar, SynthesisError> {
        self.inner.compress_to_field()
    }

    pub fn randomize(
        &self,
        spend_auth_randomizer: &SpendAuthRandomizerVar,
    ) -> Result<RandomizedVerificationKey, SynthesisError> {
        let spend_auth_basepoint_var =
            ElementVar::new_constant(self.cs.clone(), *SPENDAUTH_BASEPOINT)?;
        let point = self.inner.clone()
            + spend_auth_basepoint_var
                .scalar_mul_le(spend_auth_randomizer.inner.to_bits_le()?.iter())?;
        Ok(RandomizedVerificationKey {
            cs: self.cs.clone(),
            inner: point,
        })
    }
}

pub struct SpendAuthRandomizerVar {
    cs: ConstraintSystemRef<Fq>,
    inner: Vec<UInt8<Fq>>,
}

impl AllocVar<Fr, Fq> for SpendAuthRandomizerVar {
    fn new_variable<T: std::borrow::Borrow<Fr>>(
        cs: impl Into<ark_relations::r1cs::Namespace<Fq>>,
        f: impl FnOnce() -> Result<T, SynthesisError>,
        mode: ark_r1cs_std::prelude::AllocationMode,
    ) -> Result<Self, SynthesisError> {
        let ns = cs.into();
        let cs = ns.cs();
        let inner1 = f()?;
        let inner: Fr = *inner1.borrow();
        match mode {
            AllocationMode::Constant => unimplemented!(),
            AllocationMode::Input => unimplemented!(),
            AllocationMode::Witness => {
                let spend_auth_randomizer_arr: [u8; 32] = inner.to_bytes();
                Ok(Self {
                    cs: cs.clone(),
                    inner: UInt8::new_witness_vec(cs.clone(), &spend_auth_randomizer_arr)?,
                })
            }
        }
    }
}

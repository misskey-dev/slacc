use aws_lc_rs::signature;
use aws_lc_rs::unstable::signature::{PqdsaKeyPair, ML_DSA_44, ML_DSA_44_SIGNING};
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi_derive::napi;
use std::sync::Arc;

use crate::THREAD_POOL;

#[napi(string_enum)]
#[derive(Clone, Copy)]
pub enum SignatureAlgorithmIdentifier {
  Mldsa44,
  Eddsa,
  Rsa2048_8192,
}

enum KeyPair {
  Mldsa44(PqdsaKeyPair),
  Ed25519(signature::Ed25519KeyPair),
  Rsa(signature::RsaKeyPair),
}

enum PublicKey {
  Mldsa44(Vec<u8>),
  Ed25519(Vec<u8>),
  Rsa(Vec<u8>),
}

enum Payload {
  Parts(Vec<Buffer>),
  Raw(Buffer),
}

impl From<Payload> for Vec<u8> {
  fn from(val: Payload) -> Self {
    match val {
      Payload::Parts(parts) => {
        let mut combined = Vec::with_capacity(parts.len() * aws_lc_rs::digest::SHA256.output_len());
        for part in parts.iter() {
          combined.extend_from_slice(
            aws_lc_rs::digest::digest(&aws_lc_rs::digest::SHA256, part.as_ref()).as_ref(),
          );
        }
        combined
      }
      Payload::Raw(buf) => buf.to_vec(),
    }
  }
}

#[napi(js_name = "Signer")]
pub struct JsSigner {
  inner: Arc<KeyPair>,
}

#[napi]
impl JsSigner {
  #[napi(factory)]
  pub fn from_pkcs8_der(suite: SignatureAlgorithmIdentifier, der: Buffer) -> Result<Self> {
    Self::from_pkcs8(suite, der.to_vec())
  }

  #[napi(factory)]
  pub fn from_pkcs8_pem(suite: SignatureAlgorithmIdentifier, pem: String) -> Result<Self> {
    Self::from_pkcs8(
      suite,
      pkcs8::der::pem::decode_vec(pem.as_bytes())
        .map_err(|e| Error::new(Status::InvalidArg, e.to_string()))?
        .1,
    )
  }

  fn from_pkcs8(suite: SignatureAlgorithmIdentifier, pkcs8_der: Vec<u8>) -> Result<Self> {
    Ok(Self {
      inner: Arc::new(match suite {
        SignatureAlgorithmIdentifier::Mldsa44 => KeyPair::Mldsa44(
          PqdsaKeyPair::from_pkcs8(&ML_DSA_44_SIGNING, &pkcs8_der)
            .map_err(|e| Error::new(Status::InvalidArg, e.to_string()))?,
        ),
        SignatureAlgorithmIdentifier::Eddsa => KeyPair::Ed25519(
          signature::Ed25519KeyPair::from_pkcs8(&pkcs8_der)
            .map_err(|e| Error::new(Status::InvalidArg, e.to_string()))?,
        ),
        SignatureAlgorithmIdentifier::Rsa2048_8192 => KeyPair::Rsa(
          signature::RsaKeyPair::from_pkcs8(&pkcs8_der)
            .map_err(|e| Error::new(Status::InvalidArg, e.to_string()))?,
        ),
      }),
    })
  }

  #[napi(getter)]
  pub fn public_key(&self) -> Buffer {
    match &*self.inner {
      KeyPair::Mldsa44(key) => Buffer::from(signature::KeyPair::public_key(key).as_ref()),
      KeyPair::Ed25519(key) => Buffer::from(signature::KeyPair::public_key(key).as_ref()),
      KeyPair::Rsa(key) => Buffer::from(signature::KeyPair::public_key(key).as_ref()),
    }
  }

  fn sign(&self, payload: Payload, callback: ThreadsafeFunction<Buffer>) -> Result<()> {
    let kp = self.inner.clone();
    THREAD_POOL
      .get()
      .ok_or_else(|| Error::new(Status::GenericFailure, "slacc is not initialized"))?
      .spawn(move || {
        let res = {
          let message: Vec<u8> = payload.into();
          match &*kp {
            KeyPair::Mldsa44(key) => {
              let mut sig = vec![0; key.algorithm().signature_len()];
              key.sign(&message, &mut sig).map(|_| Buffer::from(sig))
            }
            KeyPair::Ed25519(key) => key.try_sign(&message).map(|sig| Buffer::from(sig.as_ref())),
            KeyPair::Rsa(key) => {
              let mut sig = vec![0; key.public_modulus_len()];
              key
                .sign(
                  &signature::RSA_PKCS1_SHA256,
                  &aws_lc_rs::rand::SystemRandom::default(),
                  &message,
                  &mut sig,
                )
                .map(|_| Buffer::from(sig))
            }
          }
          .map_err(|e| Error::new(Status::GenericFailure, e.to_string()))
        };
        callback.call(res, ThreadsafeFunctionCallMode::Blocking);
      });
    Ok(())
  }

  #[napi]
  pub fn sign_parts(&self, parts: Vec<Buffer>, callback: ThreadsafeFunction<Buffer>) -> Result<()> {
    self.sign(Payload::Parts(parts), callback)
  }

  #[napi]
  pub fn sign_raw(&self, payload: Buffer, callback: ThreadsafeFunction<Buffer>) -> Result<()> {
    self.sign(Payload::Raw(payload), callback)
  }
}

#[napi(js_name = "Verifier")]
pub struct JsVerifier {
  inner: Arc<PublicKey>,
}

#[napi]
impl JsVerifier {
  #[napi(factory)]
  pub fn from_spki_der(suite: SignatureAlgorithmIdentifier, der: Buffer) -> Result<Self> {
    Self::from_spki(suite, der.to_vec())
  }

  #[napi(factory)]
  pub fn from_spki_pem(suite: SignatureAlgorithmIdentifier, pem: String) -> Result<Self> {
    Self::from_spki(
      suite,
      pkcs8::der::pem::decode_vec(pem.as_bytes())
        .map_err(|e| Error::new(Status::InvalidArg, e.to_string()))?
        .1,
    )
  }

  fn from_spki(suite: SignatureAlgorithmIdentifier, spki_der: Vec<u8>) -> Result<Self> {
    let spki = spki::SubjectPublicKeyInfoRef::try_from(spki_der.as_slice())
      .map_err(|e| Error::new(Status::InvalidArg, e.to_string()))?;
    Ok(Self {
      inner: Arc::new(match (suite, spki.subject_public_key.as_bytes()) {
        (SignatureAlgorithmIdentifier::Mldsa44, Some(raw)) => PublicKey::Mldsa44(raw.to_vec()),
        (SignatureAlgorithmIdentifier::Eddsa, Some(raw)) => PublicKey::Ed25519(raw.to_vec()),
        (SignatureAlgorithmIdentifier::Rsa2048_8192, _) => PublicKey::Rsa(spki_der),
        _ => {
          return Err(Error::new(
            Status::InvalidArg,
            "could not parse public key from SPKI",
          ))
        }
      }),
    })
  }

  fn verify(
    &self,
    signature: Buffer,
    payload: Payload,
    callback: ThreadsafeFunction<bool>,
  ) -> Result<()> {
    let pk = self.inner.clone();
    THREAD_POOL
      .get()
      .ok_or_else(|| Error::new(Status::GenericFailure, "slacc is not initialized"))?
      .spawn(move || {
        let message: Vec<u8> = payload.into();
        callback.call(
          Ok(
            match &*pk {
              PublicKey::Mldsa44(pk) => signature::UnparsedPublicKey::new(&ML_DSA_44, pk),
              PublicKey::Ed25519(pk) => signature::UnparsedPublicKey::new(&signature::ED25519, pk),
              PublicKey::Rsa(pk) => signature::UnparsedPublicKey::new(
                &aws_lc_rs::signature::RSA_PKCS1_2048_8192_SHA256,
                pk,
              ),
            }
            .verify(&message, signature.as_ref())
            .is_ok(),
          ),
          ThreadsafeFunctionCallMode::Blocking,
        );
      });
    Ok(())
  }

  #[napi]
  pub fn verify_parts(
    &self,
    signature: Buffer,
    parts: Vec<Buffer>,
    callback: ThreadsafeFunction<bool>,
  ) -> Result<()> {
    self.verify(signature, Payload::Parts(parts), callback)
  }

  #[napi]
  pub fn verify_raw(
    &self,
    signature: Buffer,
    payload: Buffer,
    callback: ThreadsafeFunction<bool>,
  ) -> Result<()> {
    self.verify(signature, Payload::Raw(payload), callback)
  }
}

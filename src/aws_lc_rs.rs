use aws_lc_rs;

use crate::THREAD_POOL;

pub struct RsaKeyPair {
  inner: aws_lc_rs::signature::RsaKeyPair,
}

#[napi(js_name = "RsaKeyPair")]
pub struct JsRsaKeyPair {
  rsa_key_pair: std::sync::Arc<RsaKeyPair>,
}

#[napi]
impl JsRsaKeyPair {
  #[napi(factory)]
  pub fn from_pem(pem: String) -> napi::Result<Self> {
    let pem_object: rustls_pki_types::PrivatePkcs8KeyDer =
      rustls_pki_types::pem::PemObject::from_pem_slice(pem.as_bytes())
        .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;
    let rsa_key_pair = aws_lc_rs::signature::RsaKeyPair::from_pkcs8(pem_object.secret_pkcs8_der())
      .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;
    Ok(Self {
      rsa_key_pair: std::sync::Arc::new(RsaKeyPair {
        inner: rsa_key_pair,
      }),
    })
  }

  #[napi]
  pub fn sign(
    &self,
    data: napi::bindgen_prelude::Buffer,
    callback: napi::threadsafe_function::ThreadsafeFunction<napi::bindgen_prelude::Buffer, ()>,
  ) -> napi::Result<()> {
    let rsa_key_pair = self.rsa_key_pair.clone();
    THREAD_POOL
      .get()
      .ok_or_else(|| {
        napi::Error::new(
          napi::Status::GenericFailure,
          "slacc is not initialized".to_string(),
        )
      })?
      .spawn(move || {
        let data = data.to_vec();
        let mut signature_bytes = vec![0; rsa_key_pair.inner.public_modulus_len()];
        callback.call(
          rsa_key_pair
            .inner
            .sign(
              &aws_lc_rs::signature::RSA_PKCS1_SHA256,
              &aws_lc_rs::rand::SystemRandom::default(),
              &data,
              &mut signature_bytes,
            )
            .map(|_| napi::bindgen_prelude::Buffer::from(signature_bytes))
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string())),
          napi::threadsafe_function::ThreadsafeFunctionCallMode::Blocking,
        );
      });
    Ok(())
  }
}

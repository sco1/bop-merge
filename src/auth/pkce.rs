#[cfg(test)]
#[path = "pkce_tests.rs"]
mod tests;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

const VERIFIER_LEN: usize = 128;

fn generate_verifier<const LEN: usize>() -> String {
    const {
        assert!(
            LEN >= 43 && LEN <= 128,
            "Verifier length must be between 43 and 128 characters"
        )
    };
    super::rand_string(LEN)
}

fn challenge_from(verifier: &str) -> String {
    let hash = Sha256::digest(verifier);
    URL_SAFE_NO_PAD.encode(hash)
}

pub struct Pkce {
    verifier: String,
    challenge: String,
}

impl Pkce {
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    pub fn challenge(&self) -> &str {
        &self.challenge
    }

    pub fn generate() -> Self {
        let verifier = generate_verifier::<VERIFIER_LEN>();
        let challenge = challenge_from(&verifier);
        Self {
            verifier,
            challenge,
        }
    }
}

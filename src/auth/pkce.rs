//! Proof Key for Code Exchange ([RFC 7636]) verifier and challenge generation.
//!
//! [RFC 7636]: https://datatracker.ietf.org/doc/html/rfc7636

#[cfg(test)]
#[path = "pkce_tests.rs"]
mod tests;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};

/// Length of the generated code verifier; 128 is the maximum allowed by RFC 7636.
const VERIFIER_LEN: usize = 128;

/// Generate a random code verifier of `LEN` characters.
///
/// Characters are drawn from `[A-Za-z0-9]`, a subset of the characters RFC 7636 allows.
/// RFC 7636 requires a length of 43 to 128; any other `LEN` fails to compile.
fn generate_verifier<const LEN: usize>() -> String {
    const {
        assert!(
            LEN >= 43 && LEN <= 128,
            "Verifier length must be between 43 and 128 characters"
        )
    };
    super::rand_string(LEN)
}

/// Derive the `S256` code challenge: the unpadded base64url encoding of the verifier's SHA-256
/// hash.
fn challenge_from(verifier: &str) -> String {
    let hash = Sha256::digest(verifier);
    URL_SAFE_NO_PAD.encode(hash)
}

/// A PKCE code verifier and its matching `S256` challenge.
pub struct Pkce {
    verifier: String,
    challenge: String,
}

impl Pkce {
    /// Return the code verifier, sent with the token exchange.
    pub fn verifier(&self) -> &str {
        &self.verifier
    }

    /// Return the `S256` code challenge, sent with the authorize request.
    pub fn challenge(&self) -> &str {
        &self.challenge
    }

    /// Generates a new random verifier and its matching `S256` challenge.
    pub fn generate() -> Self {
        let verifier = generate_verifier::<VERIFIER_LEN>();
        let challenge = challenge_from(&verifier);
        Self {
            verifier,
            challenge,
        }
    }
}

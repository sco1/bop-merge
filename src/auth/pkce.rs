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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_matches_rfc7636_appendix_b() {
        assert_eq!(
            challenge_from("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
        );
    }

    #[test]
    fn verifier_meets_rfc7636_constraints() {
        let v = generate_verifier::<VERIFIER_LEN>();
        assert_eq!(v.len(), VERIFIER_LEN);
        assert!(
            v.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c))
        );
    }

    #[test]
    fn generate_produces_matching_challenge() {
        let p = Pkce::generate();
        assert_eq!(p.challenge, challenge_from(&p.verifier));
    }
}

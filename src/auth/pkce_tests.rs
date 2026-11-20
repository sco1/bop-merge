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

use super::*;

#[test]
fn authorize_url_has_expected_params() {
    let pkce = Pkce::generate();
    let url = authorize_url("my-client", &pkce, "my-state");
    let params: std::collections::HashMap<_, _> = url.query_pairs().collect();

    assert_eq!(url.host_str(), Some("accounts.spotify.com"));
    assert_eq!(params["client_id"], "my-client");
    assert_eq!(params["code_challenge"], pkce.challenge());
    assert_eq!(params["code_challenge_method"], "S256");
    assert_eq!(params["redirect_uri"], REDIRECT_URI);
    assert_eq!(params["response_type"], "code");
    assert_eq!(params["scope"], SCOPES.join(" "));
    assert_eq!(params["state"], "my-state");
}

mod callback;
mod pkce;
mod token;

use rand::distr::{Alphanumeric, SampleString};
use url::Url;

pub use crate::auth::{
    callback::{bind_callback, wait_for_callback},
    pkce::Pkce,
    token::{TokenResponse, exchange_code},
};

const AUTHORIZE_ENDPOINT: &str = "https://accounts.spotify.com/authorize";
pub const REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";
const SCOPES: &[&str] = &["playlist-modify-public"];

fn rand_string(len: usize) -> String {
    Alphanumeric.sample_string(&mut rand::rng(), len)
}

pub fn generate_state() -> String {
    rand_string(32)
}

pub fn authorize_url(client_id: &str, pkce: &Pkce, state: &str) -> Url {
    let scope = SCOPES.join(" ");
    Url::parse_with_params(
        AUTHORIZE_ENDPOINT,
        &[
            ("client_id", client_id),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT_URI),
            ("code_challenge_method", "S256"),
            ("code_challenge", pkce.challenge()),
            ("state", state),
            ("scope", &scope),
        ],
    )
    .expect("AUTHORIZE_ENDPOINT is a valid URL")
}

#[cfg(test)]
mod tests {
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
}

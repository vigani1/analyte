//! Serves `GET /proof?nonce=…`: the engine proves it holds the admin key, so the command line sends that key only to the real engine (ADR 0025).
//!
//! A stopped service leaves its loopback admin port free, and any local
//! account could listen there and catch the key the moment someone runs
//! `machines list`. So the command line first sends a fresh random nonce and
//! sends the key only when the answer is the key's HMAC of that nonce
//! ([`Token::proof`]). The route is public like `/status`, with the same Host
//! and Origin checks, connection slots, request budget and `no-store`. It
//! takes one bounded hex nonce and answers 32 bytes that say nothing about
//! the key; a nonce chosen by anyone else is of no use for a later challenge.
use super::{Context, token::Nonce};
use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    http::StatusCode,
    response::{IntoResponse as _, Response},
};
use std::sync::Arc;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::admin) struct Challenge {
    nonce: String,
}

/// The whole answer: the proof in lowercase hex.
#[derive(serde::Serialize)]
struct Answer {
    proof: String,
}

pub(in crate::admin) async fn show(
    State(state): State<Arc<Context>>,
    challenge: Result<Query<Challenge>, QueryRejection>,
) -> Response {
    let Some(nonce) = challenge
        .ok()
        .and_then(|Query(challenge)| Nonce::parse(&challenge.nonce))
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    match state.token.proof(&nonce) {
        Some(proof) => Json(Answer {
            proof: super::token::hex(&proof),
        })
        .into_response(),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::token::{Nonce, Token};

    // ADR 0025: a nonce is 32 to 128 lowercase hex digits; anything else is
    // refused before a proof is made.
    #[test]
    fn nonces_are_bounded_lowercase_hex() {
        for good in ["0".repeat(32), "ab".repeat(32), "f".repeat(128)] {
            assert!(Nonce::parse(&good).is_some(), "{good}");
        }
        for bad in [
            "0".repeat(31),
            "0".repeat(129),
            "AB".repeat(16),
            "zz".repeat(16),
        ] {
            assert!(Nonce::parse(&bad).is_none(), "{bad}");
        }
        assert!(Nonce::parse("").is_none());
    }

    // ADR 0025: a proof verifies only with the key that made it and only for
    // its own nonce; a wrong or cut answer is refused.
    #[test]
    fn a_proof_holds_only_for_its_key_and_nonce() -> Result<(), crate::Error> {
        let directory = crate::admin::private_tempdir()?;
        crate::admin::rotate_token(directory.path())?;
        let key = Token::load(directory.path())?;
        let other = crate::admin::private_tempdir()?;
        crate::admin::rotate_token(other.path())?;
        let stranger = Token::load(other.path())?;
        let nonce = Nonce::fresh()?;
        let proof = key.proof(&nonce).ok_or("no proof")?;
        assert!(key.proves(&nonce, &proof));
        assert!(!stranger.proves(&nonce, &proof));
        assert!(!key.proves(&Nonce::fresh()?, &proof));
        assert!(!key.proves(&nonce, proof.get(..31).ok_or("short")?));
        assert!(!key.proves(&nonce, &[0; 32]));
        Ok(())
    }
}

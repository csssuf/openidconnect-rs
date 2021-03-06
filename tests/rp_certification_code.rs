extern crate curl;
extern crate env_logger;
extern crate failure;
#[macro_use]
extern crate log;
extern crate oauth2;
extern crate openidconnect;
#[macro_use]
extern crate pretty_assertions;
extern crate url;

use std::collections::HashMap;

use curl::easy::Easy;
use oauth2::prelude::*;
use oauth2::{AccessToken, AuthType, AuthorizationCode, CsrfToken, RequestTokenError, Scope};
use url::Url;

use openidconnect::core::{
    CoreClient, CoreClientAuthMethod, CoreClientRegistrationRequest,
    CoreClientRegistrationResponse, CoreIdToken, CoreIdTokenClaims, CoreIdTokenVerifier,
    CoreJsonWebKeySet, CoreJwsSigningAlgorithm, CoreProviderMetadata, CoreResponseType,
    CoreUserInfoClaims, CoreUserInfoVerifier,
};
use openidconnect::discovery::ProviderMetadata;
use openidconnect::registration::{
    ClientMetadata, ClientRegistrationRequest, ClientRegistrationResponse,
};
use openidconnect::Nonce;
use openidconnect::{
    AuthenticationFlow, ClaimsVerificationError, SignatureVerificationError, StandardClaims,
    SubjectIdentifier, UserInfoError,
};

#[macro_use]
mod rp_common;

use rp_common::{get_provider_metadata, init_log, issuer_url, register_client, PanicIfFail};

struct TestState {
    access_token: Option<AccessToken>,
    authorization_code: Option<AuthorizationCode>,
    client: CoreClient,
    id_token: Option<CoreIdToken>,
    nonce: Option<Nonce>,
    provider_metadata: CoreProviderMetadata,
    registration_response: CoreClientRegistrationResponse,
}
impl TestState {
    pub fn init<F>(test_id: &'static str, reg_request_fn: F) -> Self
    where
        F: FnOnce(CoreClientRegistrationRequest) -> CoreClientRegistrationRequest,
    {
        init_log(test_id);

        let _issuer_url = issuer_url(test_id);
        let provider_metadata = get_provider_metadata(test_id);
        let registration_response = register_client(&provider_metadata, reg_request_fn);

        let redirect_uri = registration_response.redirect_uris()[0].clone();
        let client: CoreClient =
            CoreClient::from_dynamic_registration(&provider_metadata, &registration_response)
                .set_redirect_uri(redirect_uri);

        TestState {
            access_token: None,
            authorization_code: None,
            client,
            id_token: None,
            nonce: None,
            provider_metadata,
            registration_response,
        }
    }

    pub fn access_token(&self) -> &AccessToken {
        self.access_token.as_ref().expect("no access_token")
    }

    pub fn authorize(mut self, scopes: &Vec<Scope>) -> Self {
        self.client = scopes.iter().fold(self.client, |mut client, scope| {
            client = client.add_scope(scope.clone());
            client
        });
        let (url, state, nonce) = self.client.authorize_url(
            &AuthenticationFlow::AuthorizationCode::<CoreResponseType>,
            CsrfToken::new_random,
            Nonce::new_random,
        );
        log_debug!("Authorize URL: {:?}", url);

        let mut easy = Easy::new();
        easy.url(&url.to_string()[..]).unwrap();
        easy.perform().unwrap();

        let redirected_url = Url::parse(easy.redirect_url().unwrap().unwrap()).unwrap();

        log_debug!("Authorization Server redirected to: {:?}", redirected_url);

        let mut query_params = HashMap::new();
        redirected_url.query_pairs().for_each(|(key, value)| {
            query_params.insert(key, value);
        });
        log_debug!(
            "Authorization Server returned query params: {:?}",
            query_params
        );

        assert_eq!(
            self.provider_metadata.issuer().as_str(),
            query_params.get("iss").unwrap()
        );
        assert_eq!(state.secret(), query_params.get("state").unwrap());

        log_info!("Successfully received authentication response from Authorization Server");

        let authorization_code =
            AuthorizationCode::new(query_params.get("code").unwrap().to_string());
        log_debug!(
            "Authorization Server returned authorization code: {}",
            authorization_code.secret()
        );

        self.authorization_code = Some(authorization_code);
        self.nonce = Some(nonce);

        self
    }

    pub fn exchange_code(mut self) -> Self {
        let token_response = self
            .client
            .exchange_code(
                self.authorization_code
                    .take()
                    .expect("no authorization_code"),
            ).panic_if_fail("failed to exchange authorization code for token");
        log_debug!(
            "Authorization Server returned token response: {:?}",
            token_response
        );

        self.access_token = Some(token_response.access_token().clone());

        let id_token = (*token_response.extra_fields().id_token()).clone();
        self.id_token = Some(id_token);

        self
    }

    pub fn id_token(&self) -> &CoreIdToken {
        self.id_token.as_ref().expect("no id_token")
    }

    pub fn id_token_verifier(&self, jwks: CoreJsonWebKeySet) -> CoreIdTokenVerifier {
        CoreIdTokenVerifier::new_private_client(
            self.registration_response.client_id().clone(),
            self.registration_response
                .client_secret()
                .expect("no client_secret")
                .clone(),
            self.provider_metadata.issuer().clone(),
            jwks,
        )
    }

    pub fn id_token_claims(&self) -> &CoreIdTokenClaims {
        let verifier = self.id_token_verifier(self.jwks());
        self.id_token()
            .claims(&verifier, self.nonce.as_ref().expect("no nonce"))
            .panic_if_fail("failed to validate claims")
    }

    pub fn id_token_claims_failure(&self) -> ClaimsVerificationError {
        let verifier = self.id_token_verifier(self.jwks());
        self.id_token()
            .claims(&verifier, self.nonce.as_ref().expect("no nonce"))
            .expect_err("claims verification succeeded but was expected to fail")
    }

    pub fn jwks(&self) -> CoreJsonWebKeySet {
        self.provider_metadata
            .jwks_uri()
            .unwrap()
            .get_keys()
            .panic_if_fail("failed to fetch JWK set")
    }

    pub fn set_auth_type(mut self, auth_type: AuthType) -> Self {
        self.client = self.client.set_auth_type(auth_type);
        self
    }

    pub fn user_info_verifier(
        &self,
        jwks: CoreJsonWebKeySet,
        sub: SubjectIdentifier,
    ) -> CoreUserInfoVerifier {
        CoreUserInfoVerifier::new(
            self.registration_response.client_id().clone(),
            self.provider_metadata.issuer().clone(),
            jwks,
            sub,
        )
    }

    pub fn user_info_claims(&self) -> CoreUserInfoClaims {
        let verifier = self.user_info_verifier(self.jwks(), self.id_token_claims().sub().clone());
        self.provider_metadata
            .userinfo_endpoint()
            .unwrap()
            .get_user_info(self.access_token(), &verifier)
            .panic_if_fail("failed to get UserInfo")
    }

    pub fn user_info_claims_failure(&self) -> UserInfoError {
        let verifier = self.user_info_verifier(self.jwks(), self.id_token_claims().sub().clone());
        let user_info_result: Result<CoreUserInfoClaims, UserInfoError> = self
            .provider_metadata
            .userinfo_endpoint()
            .unwrap()
            .get_user_info(self.access_token(), &verifier);
        match user_info_result {
            Err(err) => err,
            _ => panic!("claims verification succeeded but was expected to fail"),
        }
    }
}

#[test]
fn rp_response_type_code() {
    let test_state = TestState::init("rp-response_type-code", |reg| reg).authorize(&vec![]);
    assert!(
        test_state
            .authorization_code
            .expect("no authorization_code")
            .secret()
            != ""
    );
    log_info!("SUCCESS");
}

#[test]
fn rp_scope_userinfo_claims() {
    let user_info_scopes = vec!["profile", "email", "address", "phone"]
        .iter()
        .map(|scope| Scope::new(scope.to_string()))
        .collect::<Vec<_>>();
    let test_state = TestState::init("rp-scope-userinfo-claims", |reg| reg)
        .authorize(&user_info_scopes)
        .exchange_code();
    let id_token_claims = test_state.id_token_claims();
    log_debug!("ID token: {:?}", id_token_claims);

    let user_info_claims = test_state.user_info_claims();
    log_debug!("UserInfo response: {:?}", user_info_claims);

    assert!(id_token_claims.sub() == user_info_claims.sub());
    assert!(
        !user_info_claims
            .email()
            .expect("no email returned by UserInfo endpoint")
            .is_empty()
    );
    assert!(
        !user_info_claims
            .address()
            .expect("no address returned by UserInfo endpoint")
            .street_address()
            .expect("no street address returned by UserInfo endpoint")
            .is_empty()
    );
    assert!(
        !user_info_claims
            .phone_number()
            .expect("no phone_number returned by UserInfo endpoint")
            .is_empty()
    );

    log_info!("SUCCESS");
}

#[test]
fn rp_nonce_invalid() {
    let test_state = TestState::init("rp-nonce-invalid", |reg| reg)
        .authorize(&vec![])
        .exchange_code();

    match test_state.id_token_claims_failure() {
        ClaimsVerificationError::InvalidNonce(_) => {
            log_error!("ID token contains invalid nonce (expected result)")
        }
        other => panic!("Unexpected result verifying ID token claims: {:?}", other),
    }

    log_info!("SUCCESS");
}

#[test]
fn rp_token_endpoint_client_secret_basic() {
    let test_state = TestState::init("rp-token_endpoint-client_secret_basic", |reg| {
        reg.set_token_endpoint_auth_method(Some(CoreClientAuthMethod::ClientSecretBasic))
    }).set_auth_type(AuthType::BasicAuth)
    .authorize(&vec![])
    .exchange_code();

    let id_token_claims = test_state.id_token_claims();
    log_debug!("ID token: {:?}", id_token_claims);

    log_info!("SUCCESS");
}

#[test]
fn rp_token_endpoint_client_secret_post() {
    let test_state = TestState::init("rp-token_endpoint-client_secret_post", |reg| {
        reg.set_token_endpoint_auth_method(Some(CoreClientAuthMethod::ClientSecretPost))
    }).set_auth_type(AuthType::RequestBody)
    .authorize(&vec![])
    .exchange_code();

    let id_token_claims = test_state.id_token_claims();
    log_debug!("ID token: {:?}", id_token_claims);

    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_kid_absent_single_jwks() {
    let test_state = TestState::init("rp-id_token-kid-absent-single-jwks", |reg| reg)
        .authorize(&vec![])
        .exchange_code();

    let id_token_claims = test_state.id_token_claims();
    log_debug!("ID token: {:?}", id_token_claims);

    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_iat() {
    let mut test_state = TestState::init("rp-id_token-iat", |reg| reg).authorize(&vec![]);

    let token_response = test_state.client.exchange_code(
        test_state
            .authorization_code
            .take()
            .expect("no authorization_code"),
    );

    match token_response {
        Err(RequestTokenError::Parse(_)) => {
            log_error!("ID token failed to parse without `iat` claim (expected result)")
        }
        other => panic!("Unexpected result verifying ID token claims: {:?}", other),
    }
    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_aud() {
    let test_state = TestState::init("rp-id_token-aud", |reg| reg)
        .authorize(&vec![])
        .exchange_code();

    match test_state.id_token_claims_failure() {
        ClaimsVerificationError::InvalidAudience(_) => {
            log_error!("ID token has invalid audience (expected result)")
        }
        other => panic!("Unexpected result verifying ID token claims: {:?}", other),
    }

    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_kid_absent_multiple_jwks() {
    let test_state = TestState::init("rp-id_token-kid-absent-multiple-jwks", |reg| reg)
        .authorize(&vec![])
        .exchange_code();

    match test_state.id_token_claims_failure() {
        ClaimsVerificationError::SignatureVerification(
            SignatureVerificationError::AmbiguousKeyId(_),
        ) => log_error!("ID token has ambiguous key identification without KID (expected result)"),
        other => panic!("Unexpected result verifying ID token claims: {:?}", other),
    }

    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_sig_none() {
    let test_state = TestState::init("rp-id_token-sig-none", |reg| reg)
        .authorize(&vec![])
        .exchange_code();

    let verifier = test_state
        .id_token_verifier(test_state.jwks())
        .insecure_disable_signature_check();

    let id_token_claims = test_state
        .id_token()
        .claims(&verifier, test_state.nonce.as_ref().expect("no nonce"))
        .panic_if_fail("failed to validate claims");
    log_debug!("ID token: {:?}", id_token_claims);

    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_sig_rs256() {
    let test_state = TestState::init("rp-id_token-sig-rs256", |reg| reg)
        .authorize(&vec![])
        .exchange_code();

    let id_token_claims = test_state.id_token_claims();
    log_debug!("ID token: {:?}", id_token_claims);

    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_sig_hs256() {
    let test_state = TestState::init("rp-id_token-sig-hs256", |reg| reg)
        .authorize(&vec![])
        .exchange_code();

    let verifier = test_state
        .id_token_verifier(test_state.jwks())
        .set_allowed_algs(vec![CoreJwsSigningAlgorithm::HmacSha256]);
    let id_token_claims = test_state
        .id_token()
        .claims(&verifier, test_state.nonce.as_ref().expect("no nonce"))
        .panic_if_fail("failed to validate claims");
    log_debug!("ID token: {:?}", id_token_claims);

    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_sub() {
    let mut test_state = TestState::init("rp-id_token-sub", |reg| reg).authorize(&vec![]);

    let token_response = test_state.client.exchange_code(
        test_state
            .authorization_code
            .take()
            .expect("no authorization_code"),
    );

    match token_response {
        Err(RequestTokenError::Parse(_)) => {
            log_error!("ID token failed to parse without `sub` claim (expected result)")
        }
        other => panic!("Unexpected result verifying ID token claims: {:?}", other),
    }
    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_bad_sig_rs256() {
    let test_state = TestState::init("rp-id_token-bad-sig-rs256", |reg| reg)
        .authorize(&vec![])
        .exchange_code();

    match test_state.id_token_claims_failure() {
        ClaimsVerificationError::SignatureVerification(
            SignatureVerificationError::CryptoError(_),
        ) => log_error!("ID token has invalid signature (expected result)"),
        other => panic!("Unexpected result verifying ID token claims: {:?}", other),
    }

    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_bad_sig_hs256() {
    let test_state = TestState::init("rp-id_token-bad-sig-hs256", |reg| reg)
        .authorize(&vec![])
        .exchange_code();

    let verifier = test_state
        .id_token_verifier(test_state.jwks())
        .set_allowed_algs(vec![CoreJwsSigningAlgorithm::HmacSha256]);
    let id_token_err = test_state
        .id_token()
        .claims(&verifier, test_state.nonce.as_ref().expect("no nonce"))
        .expect_err("claims verification succeeded but was expected to fail");
    match id_token_err {
        ClaimsVerificationError::SignatureVerification(
            SignatureVerificationError::CryptoError(_),
        ) => log_error!("ID token has invalid signature (expected result)"),
        other => panic!("Unexpected result verifying ID token claims: {:?}", other),
    }

    log_info!("SUCCESS");
}

#[test]
fn rp_id_token_issuer_mismatch() {
    let test_state = TestState::init("rp-id_token-issuer-mismatch", |reg| reg)
        .authorize(&vec![])
        .exchange_code();

    match test_state.id_token_claims_failure() {
        ClaimsVerificationError::InvalidIssuer(_) => {
            log_error!("ID token has invalid issuer (expected result)")
        }
        other => panic!("Unexpected result verifying ID token claims: {:?}", other),
    }

    log_info!("SUCCESS");
}

#[test]
fn rp_userinfo_bad_sub_claim() {
    let test_state = TestState::init("rp-userinfo-bad-sub-claim", |reg| reg)
        .authorize(&vec![Scope::new("profile".to_string())])
        .exchange_code();
    let id_token_claims = test_state.id_token_claims();
    log_debug!("ID token: {:?}", id_token_claims);

    match test_state.user_info_claims_failure() {
        UserInfoError::ClaimsVerification(ClaimsVerificationError::InvalidSubject(_)) => {
            log_error!("UserInfo response has invalid subject (expected result)")
        }
        other => panic!("Unexpected result verifying ID token claims: {:?}", other),
    }
    log_info!("SUCCESS");
}

#[test]
fn rp_userinfo_bearer_header() {
    let test_state = TestState::init("rp-userinfo-bearer-header", |reg| reg)
        .authorize(&vec![Scope::new("profile".to_string())])
        .exchange_code();
    let id_token_claims = test_state.id_token_claims();
    log_debug!("ID token: {:?}", id_token_claims);

    let user_info_claims = test_state.user_info_claims();
    log_debug!("UserInfo response: {:?}", user_info_claims);
    log_info!("SUCCESS");
}

#[test]
fn rp_userinfo_sig() {
    let test_state = TestState::init("rp-userinfo-sig", |reg| {
        reg.set_userinfo_signed_response_alg(Some(CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256))
    }).authorize(&vec![Scope::new("profile".to_string())])
    .exchange_code();
    let id_token_claims = test_state.id_token_claims();
    log_debug!("ID token: {:?}", id_token_claims);

    let verifier = test_state
        .user_info_verifier(
            test_state.jwks(),
            test_state.id_token_claims().sub().clone(),
        ).require_signed_response(true)
        // For some reason, the test suite omits these claims even though the Core spec says
        // that the RP SHOULD verify these.
        .require_audience_match(false)
        .require_issuer_match(false);
    let user_info_claims: CoreUserInfoClaims = test_state
        .provider_metadata
        .userinfo_endpoint()
        .unwrap()
        .get_user_info(test_state.access_token(), &verifier)
        .panic_if_fail("failed to get user info");

    log_debug!("UserInfo response: {:?}", user_info_claims);
    log_info!("SUCCESS");
}

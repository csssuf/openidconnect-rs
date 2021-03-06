use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Result as FormatterResult};
use std::marker::{PhantomData, Send, Sync};
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use curl;
use oauth2::{AccessToken, ClientId, ClientSecret, ErrorResponse, ErrorResponseType, RedirectUrl};
use serde;
use serde::de::{Deserialize, DeserializeOwned, Deserializer, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use serde_json;

use super::discovery::JsonWebKeySetUrl;
use super::http::{
    auth_bearer, HttpRequest, HttpRequestMethod, ACCEPT_JSON, CONTENT_TYPE_JSON,
    HTTP_STATUS_BAD_REQUEST, HTTP_STATUS_CREATED, MIME_TYPE_JSON,
};
use super::macros::TraitStructExtract;
use super::types::helpers::split_language_tag_key;
use super::types::{
    ApplicationType, AuthenticationContextClass, ClientAuthMethod, ClientConfigUrl, ClientName,
    ClientUrl, ContactEmail, GrantType, InitiateLoginUrl, JsonWebKeyType, JsonWebKeyUse,
    JweContentEncryptionAlgorithm, JweKeyManagementAlgorithm, JwsSigningAlgorithm, LanguageTag,
    LogoUrl, PolicyUrl, RegistrationAccessToken, RegistrationUrl, RequestUrl, ResponseType,
    ResponseTypes, SectorIdentifierUrl, SubjectIdentifierType, ToSUrl,
};
use super::{JsonWebKey, JsonWebKeySet};

// FIXME: switch to embedding a flattened extra_fields struct
trait_struct![
    trait ClientMetadata[
        AT: ApplicationType,
        CA: ClientAuthMethod,
        G: GrantType,
        JE: JweContentEncryptionAlgorithm,
        JK: JweKeyManagementAlgorithm,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType,
        JU: JsonWebKeyUse,
        K: JsonWebKey<JS, JT, JU>,
        RT: ResponseType,
        S: SubjectIdentifierType,
    ] : [Clone + Debug + DeserializeOwned + PartialEq + Serialize] {}
    #[derive(Clone, Debug, PartialEq)]
    struct Registration10ClientMetadata[
        AT: ApplicationType,
        CA: ClientAuthMethod,
        G: GrantType,
        JE: JweContentEncryptionAlgorithm,
        JK: JweKeyManagementAlgorithm,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType,
        JU: JsonWebKeyUse,
        K: JsonWebKey<JS, JT, JU>,
        RT: ResponseType,
        S: SubjectIdentifierType,
    ] {
        redirect_uris(&Vec<RedirectUrl>) <- Vec<RedirectUrl>,
        response_types(Option<&Vec<ResponseTypes<RT>>>) <- Option<Vec<ResponseTypes<RT>>>,
        grant_types(Option<&Vec<G>>) <- Option<Vec<G>>,
        application_type(Option<&AT>) <- Option<AT>,
        contacts(Option<&Vec<ContactEmail>>) <- Option<Vec<ContactEmail>>,
        client_name(Option<&HashMap<Option<LanguageTag>, ClientName>>)
            <- Option<HashMap<Option<LanguageTag>, ClientName>>,
        logo_uri(Option<&HashMap<Option<LanguageTag>, LogoUrl>>)
            <- Option<HashMap<Option<LanguageTag>, LogoUrl>>,
        client_uri(Option<&HashMap<Option<LanguageTag>, ClientUrl>>)
            <- Option<HashMap<Option<LanguageTag>, ClientUrl>>,
        policy_uri(Option<&HashMap<Option<LanguageTag>, PolicyUrl>>)
            <- Option<HashMap<Option<LanguageTag>, PolicyUrl>>,
        tos_uri(Option<&HashMap<Option<LanguageTag>, ToSUrl>>)
            <- Option<HashMap<Option<LanguageTag>, ToSUrl>>,
        jwks_uri(Option<&JsonWebKeySetUrl>) <- Option<JsonWebKeySetUrl>,
        jwks(Option<&JsonWebKeySet<JS, JT, JU, K>>) <- Option<JsonWebKeySet<JS, JT, JU, K>>,
        sector_identifier_uri(Option<&SectorIdentifierUrl>) <- Option<SectorIdentifierUrl>,
        subject_type(Option<&S>) <- Option<S>,
        id_token_signed_response_alg(Option<&JS>) <- Option<JS>,
        id_token_encrypted_response_alg(Option<&JK>) <- Option<JK>,
        id_token_encrypted_response_enc(Option<&JE>) <- Option<JE>,
        userinfo_signed_response_alg(Option<&JS>) <- Option<JS>,
        userinfo_encrypted_response_alg(Option<&JK>) <- Option<JK>,
        userinfo_encrypted_response_enc(Option<&JE>) <- Option<JE>,
        request_object_signing_alg(Option<&JS>) <- Option<JS>,
        request_object_encryption_alg(Option<&JK>) <- Option<JK>,
        request_object_encryption_enc(Option<&JE>) <- Option<JE>,
        token_endpoint_auth_method(Option<&CA>) <- Option<CA>,
        token_endpoint_auth_signing_alg(Option<&JS>) <- Option<JS>,
        default_max_age(Option<&Duration>) <- Option<Duration>,
        require_auth_time(Option<bool>) <- Option<bool>,
        default_acr_values(Option<&Vec<AuthenticationContextClass>>)
            <- Option<Vec<AuthenticationContextClass>>,
        initiate_login_uri(Option<&InitiateLoginUrl>) <- Option<InitiateLoginUrl>,
        request_uris(Option<&Vec<RequestUrl>>) <- Option<Vec<RequestUrl>>,
    }
    impl [
        AT: ApplicationType,
        CA: ClientAuthMethod,
        G: GrantType,
        JE: JweContentEncryptionAlgorithm,
        JK: JweKeyManagementAlgorithm,
        JS: JwsSigningAlgorithm<JT>,
        JT: JsonWebKeyType,
        JU: JsonWebKeyUse,
        K: JsonWebKey<JS, JT, JU>,
        RT: ResponseType,
        S: SubjectIdentifierType,
    ] trait[AT, CA, G, JE, JK, JS, JT, JU, K, RT, S] for
    struct[AT, CA, G, JE, JK, JS, JT, JU, K, RT, S]
];
impl<'de, AT, CA, G, JE, JK, JS, JT, JU, K, RT, S> Deserialize<'de>
    for Registration10ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>
where
    AT: ApplicationType,
    CA: ClientAuthMethod,
    G: GrantType,
    JE: JweContentEncryptionAlgorithm,
    JK: JweKeyManagementAlgorithm,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    RT: ResponseType,
    S: SubjectIdentifierType,
{
    ///
    /// Special deserializer that supports [RFC 5646](https://tools.ietf.org/html/rfc5646) language
    /// tags associated with human-readable client metadata fields.
    ///
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct MetadataVisitor<
            AT: ApplicationType,
            CA: ClientAuthMethod,
            G: GrantType,
            JE: JweContentEncryptionAlgorithm,
            JK: JweKeyManagementAlgorithm,
            JS: JwsSigningAlgorithm<JT>,
            JT: JsonWebKeyType,
            JU: JsonWebKeyUse,
            K: JsonWebKey<JS, JT, JU>,
            RT: ResponseType,
            S: SubjectIdentifierType,
        >(
            PhantomData<AT>,
            PhantomData<CA>,
            PhantomData<G>,
            PhantomData<JE>,
            PhantomData<JK>,
            PhantomData<JS>,
            PhantomData<JT>,
            PhantomData<JU>,
            PhantomData<K>,
            PhantomData<RT>,
            PhantomData<S>,
        );
        impl<'de, AT, CA, G, JE, JK, JS, JT, JU, K, RT, S> Visitor<'de>
            for MetadataVisitor<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>
        where
            AT: ApplicationType,
            CA: ClientAuthMethod,
            G: GrantType,
            JE: JweContentEncryptionAlgorithm,
            JK: JweKeyManagementAlgorithm,
            JS: JwsSigningAlgorithm<JT>,
            JT: JsonWebKeyType,
            JU: JsonWebKeyUse,
            K: JsonWebKey<JS, JT, JU>,
            RT: ResponseType,
            S: SubjectIdentifierType,
        {
            type Value = Registration10ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>;

            fn expecting(&self, formatter: &mut Formatter) -> FormatterResult {
                formatter.write_str("struct Registration10ClientMetadata")
            }
            fn visit_map<V>(self, mut map: V) -> Result<Self::Value, V::Error>
            where
                V: MapAccess<'de>,
            {
                deserialize_fields!{
                    map {
                        [redirect_uris]
                        [Option(response_types)]
                        [Option(grant_types)]
                        [Option(application_type)]
                        [Option(contacts)]
                        [LanguageTag(client_name)]
                        [LanguageTag(logo_uri)]
                        [LanguageTag(client_uri)]
                        [LanguageTag(policy_uri)]
                        [LanguageTag(tos_uri)]
                        [Option(jwks_uri)]
                        [Option(jwks)]
                        [Option(sector_identifier_uri)]
                        [Option(subject_type)]
                        [Option(id_token_signed_response_alg)]
                        [Option(id_token_encrypted_response_alg)]
                        [Option(id_token_encrypted_response_enc)]
                        [Option(userinfo_signed_response_alg)]
                        [Option(userinfo_encrypted_response_alg)]
                        [Option(userinfo_encrypted_response_enc)]
                        [Option(request_object_signing_alg)]
                        [Option(request_object_encryption_alg)]
                        [Option(request_object_encryption_enc)]
                        [Option(token_endpoint_auth_method)]
                        [Option(token_endpoint_auth_signing_alg)]
                        [Option(Seconds(default_max_age))]
                        [Option(require_auth_time)]
                        [Option(default_acr_values)]
                        [Option(initiate_login_uri)]
                        [Option(request_uris)]
                    }
                }
            }
        }
        deserializer.deserialize_map(MetadataVisitor(
            PhantomData,
            PhantomData,
            PhantomData,
            PhantomData,
            PhantomData,
            PhantomData,
            PhantomData,
            PhantomData,
            PhantomData,
            PhantomData,
            PhantomData,
        ))
    }
}
impl<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S> Serialize
    for Registration10ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>
where
    AT: ApplicationType,
    CA: ClientAuthMethod,
    G: GrantType,
    JE: JweContentEncryptionAlgorithm,
    JK: JweKeyManagementAlgorithm,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    RT: ResponseType,
    S: SubjectIdentifierType,
{
    fn serialize<SE>(&self, serializer: SE) -> Result<SE::Ok, SE::Error>
    where
        SE: Serializer,
    {
        serialize_fields!{
            self -> serializer {
                [redirect_uris]
                [Option(response_types)]
                [Option(grant_types)]
                [Option(application_type)]
                [Option(contacts)]
                [LanguageTag(client_name)]
                [LanguageTag(logo_uri)]
                [LanguageTag(client_uri)]
                [LanguageTag(policy_uri)]
                [LanguageTag(tos_uri)]
                [Option(jwks_uri)]
                [Option(jwks)]
                [Option(sector_identifier_uri)]
                [Option(subject_type)]
                [Option(id_token_signed_response_alg)]
                [Option(id_token_encrypted_response_alg)]
                [Option(id_token_encrypted_response_enc)]
                [Option(userinfo_signed_response_alg)]
                [Option(userinfo_encrypted_response_alg)]
                [Option(userinfo_encrypted_response_enc)]
                [Option(request_object_signing_alg)]
                [Option(request_object_encryption_alg)]
                [Option(request_object_encryption_enc)]
                [Option(token_endpoint_auth_method)]
                [Option(token_endpoint_auth_signing_alg)]
                [Option(Seconds(default_max_age))]
                [Option(require_auth_time)]
                [Option(default_acr_values)]
                [Option(initiate_login_uri)]
                [Option(request_uris)]
            }
        }
    }
}

// FIXME: switch to embedding a flattened extra_fields struct
pub trait ClientRegistrationRequest<AT, CA, CM, CR, ET, G, JE, JK, JS, JT, JU, K, RT, S>:
    Debug + PartialEq
where
    AT: ApplicationType,
    CA: ClientAuthMethod,
    CM: ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>,
    CR: ClientRegistrationResponse<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>,
    ET: RegisterErrorResponseType,
    G: GrantType,
    JE: JweContentEncryptionAlgorithm,
    JK: JweKeyManagementAlgorithm,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    RT: ResponseType,
    S: SubjectIdentifierType,
{
    fn new(redirect_uris: Vec<RedirectUrl>) -> Self;
    fn client_metadata(&self) -> &CM;

    // FIXME: should this be an InitialAccessToken?
    fn initial_access_token(&self) -> Option<&AccessToken>;
    fn set_initial_access_token(&mut self, access_token: Option<AccessToken>);

    fn register(
        &self,
        registration_endpoint: &RegistrationUrl,
    ) -> Result<CR, ClientRegistrationError<ET>> {
        let request_json = serde_json::to_string(self.client_metadata())
            .map_err(ClientRegistrationError::Json)?
            .into_bytes();

        let auth_header_opt = if let Some(initial_access_token) = self.initial_access_token() {
            Some(auth_bearer(initial_access_token))
        } else {
            None
        };

        let mut headers = vec![ACCEPT_JSON, CONTENT_TYPE_JSON];
        if let Some((header, ref value)) = auth_header_opt {
            headers.push((header, value.as_ref()));
        }

        let register_response = HttpRequest {
            url: registration_endpoint.url(),
            method: HttpRequestMethod::Post,
            headers: &headers,
            post_body: &request_json,
        }.request()
        .map_err(ClientRegistrationError::Request)?;

        // FIXME: check for WWW-Authenticate response header if bearer auth was used (see
        //   https://tools.ietf.org/html/rfc6750#section-3)
        // FIXME: improve error handling (i.e., is there a body response?)
        // FIXME: other necessary response validation? check spec

        // Spec says that a successful response SHOULD use 201 Created, and a registration error
        // condition returns (no "SHOULD") 400 Bad Request. For now, only accept these two status
        // codes. We may need to relax the success status to improve interoperability.
        if register_response.status_code != HTTP_STATUS_CREATED
            && register_response.status_code != HTTP_STATUS_BAD_REQUEST
        {
            return Err(ClientRegistrationError::Response(
                register_response.status_code,
                "unexpected HTTP status code".to_string(),
            ));
        }

        register_response
            .check_content_type(MIME_TYPE_JSON)
            .map_err(|err_msg| {
                ClientRegistrationError::Response(register_response.status_code, err_msg)
            })?;

        let response_body = String::from_utf8(register_response.body).map_err(|parse_error| {
            ClientRegistrationError::Other(format!(
                "couldn't parse response as UTF-8: {}",
                parse_error
            ))
        })?;

        if register_response.status_code == HTTP_STATUS_BAD_REQUEST {
            let response_error: ErrorResponse<ET> =
                serde_json::from_str(&response_body).map_err(ClientRegistrationError::Json)?;
            return Err(ClientRegistrationError::ServerResponse(response_error));
        }

        serde_json::from_str(&response_body).map_err(ClientRegistrationError::Json)
    }

    field_setter_decls![
        set_redirect_uris -> redirect_uris[Vec<RedirectUrl>],
        set_response_types -> response_types[Option<Vec<ResponseTypes<RT>>>],
        set_grant_types -> grant_types[Option<Vec<G>>],
        set_application_type -> application_type[Option<AT>],
        set_contacts -> contacts[Option<Vec<ContactEmail>>],
        set_client_name -> client_name[Option<HashMap<Option<LanguageTag>, ClientName> >],
        set_logo_uri -> logo_uri[Option<HashMap<Option<LanguageTag>, LogoUrl> >],
        set_client_uri -> client_uri[Option<HashMap<Option<LanguageTag>, ClientUrl> >],
        set_policy_uri -> policy_uri[Option<HashMap<Option<LanguageTag>, PolicyUrl> >],
        set_tos_uri -> tos_uri[Option<HashMap<Option<LanguageTag>, ToSUrl> >],
        set_jwks_uri -> jwks_uri[Option<JsonWebKeySetUrl>],
        set_jwks -> jwks[Option<JsonWebKeySet<JS, JT, JU, K>>],
        set_sector_identifier_uri -> sector_identifier_uri[Option<SectorIdentifierUrl>],
        set_subject_type -> subject_type[Option<S>],
        set_id_token_signed_response_alg -> id_token_signed_response_alg[Option<JS>],
        set_id_token_encrypted_response_alg -> id_token_encrypted_response_alg[Option<JK>],
        set_id_token_encrypted_response_enc -> id_token_encrypted_response_enc[Option<JE>],
        set_userinfo_signed_response_alg -> userinfo_signed_response_alg[Option<JS>],
        set_userinfo_encrypted_response_alg -> userinfo_encrypted_response_alg[Option<JK>],
        set_userinfo_encrypted_response_enc -> userinfo_encrypted_response_enc[Option<JE>],
        set_request_object_signing_alg -> request_object_signing_alg[Option<JS>],
        set_request_object_encryption_alg -> request_object_encryption_alg[Option<JK>],
        set_request_object_encryption_enc -> request_object_encryption_enc[Option<JE>],
        set_token_endpoint_auth_method -> token_endpoint_auth_method[Option<CA>],
        set_token_endpoint_auth_signing_alg -> token_endpoint_auth_signing_alg[Option<JS>],
        set_default_max_age -> default_max_age[Option<Duration>],
        set_require_auth_time -> require_auth_time[Option<bool>],
        set_default_acr_values -> default_acr_values[Option<Vec<AuthenticationContextClass>>],
        set_initiate_login_uri -> initiate_login_uri[Option<InitiateLoginUrl>],
        set_request_uris -> request_uris[Option<Vec<RequestUrl>>],
    ];
}
#[derive(Clone, Debug, PartialEq)]
pub struct Registration10ClientRegistrationRequest<
    AT: ApplicationType,
    CA: ClientAuthMethod,
    CR: ClientRegistrationResponse<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>,
    ET: RegisterErrorResponseType,
    G: GrantType,
    JE: JweContentEncryptionAlgorithm,
    JK: JweKeyManagementAlgorithm,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    RT: ResponseType,
    S: SubjectIdentifierType,
> {
    client_metadata: Registration10ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>,
    initial_access_token: Option<AccessToken>,
    _phantom_cr: PhantomData<CR>,
    _phantom_et: PhantomData<ET>,
}
impl<AT, CA, CR, ET, G, JE, JK, JS, JT, JU, K, RT, S>
    ClientRegistrationRequest<
        AT,
        CA,
        Registration10ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>,
        CR,
        ET,
        G,
        JE,
        JK,
        JS,
        JT,
        JU,
        K,
        RT,
        S,
    > for Registration10ClientRegistrationRequest<AT, CA, CR, ET, G, JE, JK, JS, JT, JU, K, RT, S>
where
    AT: ApplicationType,
    CA: ClientAuthMethod,
    CR: ClientRegistrationResponse<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>,
    ET: RegisterErrorResponseType,
    G: GrantType,
    JE: JweContentEncryptionAlgorithm,
    JK: JweKeyManagementAlgorithm,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    RT: ResponseType,
    S: SubjectIdentifierType,
{
    fn new(redirect_uris: Vec<RedirectUrl>) -> Self {
        Registration10ClientRegistrationRequest {
            client_metadata: Registration10ClientMetadata {
                redirect_uris,
                response_types: None,
                grant_types: None,
                application_type: None,
                contacts: None,
                client_name: None,
                logo_uri: None,
                client_uri: None,
                policy_uri: None,
                tos_uri: None,
                jwks_uri: None,
                jwks: None,
                sector_identifier_uri: None,
                subject_type: None,
                id_token_signed_response_alg: None,
                id_token_encrypted_response_alg: None,
                id_token_encrypted_response_enc: None,
                userinfo_signed_response_alg: None,
                userinfo_encrypted_response_alg: None,
                userinfo_encrypted_response_enc: None,
                request_object_signing_alg: None,
                request_object_encryption_alg: None,
                request_object_encryption_enc: None,
                token_endpoint_auth_method: None,
                token_endpoint_auth_signing_alg: None,
                default_max_age: None,
                require_auth_time: None,
                default_acr_values: None,
                initiate_login_uri: None,
                request_uris: None,
            },
            initial_access_token: None,
            _phantom_cr: PhantomData,
            _phantom_et: PhantomData,
        }
    }
    fn client_metadata(
        &self,
    ) -> &Registration10ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S> {
        &self.client_metadata
    }

    fn initial_access_token(&self) -> Option<&AccessToken> {
        self.initial_access_token.as_ref()
    }
    fn set_initial_access_token(&mut self, access_token: Option<AccessToken>) {
        self.initial_access_token = access_token
    }

    field_setters![
        self [self.client_metadata] {
            set_redirect_uris -> redirect_uris[Vec<RedirectUrl>],
            set_response_types -> response_types[Option<Vec<ResponseTypes<RT>>>],
            set_grant_types -> grant_types[Option<Vec<G>>],
            set_application_type -> application_type[Option<AT>],
            set_contacts -> contacts[Option<Vec<ContactEmail>>],
            set_client_name -> client_name[Option<HashMap<Option<LanguageTag>, ClientName> >],
            set_logo_uri -> logo_uri[Option<HashMap<Option<LanguageTag>, LogoUrl> >],
            set_client_uri -> client_uri[Option<HashMap<Option<LanguageTag>, ClientUrl> >],
            set_policy_uri -> policy_uri[Option<HashMap<Option<LanguageTag>, PolicyUrl> >],
            set_tos_uri -> tos_uri[Option<HashMap<Option<LanguageTag>, ToSUrl> >],
            set_jwks_uri -> jwks_uri[Option<JsonWebKeySetUrl>],
            set_jwks -> jwks[Option<JsonWebKeySet<JS, JT, JU, K>>],
            set_sector_identifier_uri -> sector_identifier_uri[Option<SectorIdentifierUrl>],
            set_subject_type -> subject_type[Option<S>],
            set_id_token_signed_response_alg -> id_token_signed_response_alg[Option<JS>],
            set_id_token_encrypted_response_alg -> id_token_encrypted_response_alg[Option<JK>],
            set_id_token_encrypted_response_enc -> id_token_encrypted_response_enc[Option<JE>],
            set_userinfo_signed_response_alg -> userinfo_signed_response_alg[Option<JS>],
            set_userinfo_encrypted_response_alg -> userinfo_encrypted_response_alg[Option<JK>],
            set_userinfo_encrypted_response_enc -> userinfo_encrypted_response_enc[Option<JE>],
            set_request_object_signing_alg -> request_object_signing_alg[Option<JS>],
            set_request_object_encryption_alg -> request_object_encryption_alg[Option<JK>],
            set_request_object_encryption_enc -> request_object_encryption_enc[Option<JE>],
            set_token_endpoint_auth_method -> token_endpoint_auth_method[Option<CA>],
            set_token_endpoint_auth_signing_alg -> token_endpoint_auth_signing_alg[Option<JS>],
            set_default_max_age -> default_max_age[Option<Duration>],
            set_require_auth_time -> require_auth_time[Option<bool>],
            set_default_acr_values -> default_acr_values[Option<Vec<AuthenticationContextClass>>],
            set_initiate_login_uri -> initiate_login_uri[Option<InitiateLoginUrl>],
            set_request_uris -> request_uris[Option<Vec<RequestUrl>>],
        }
    ];
}

// FIXME: switch to embedding a flattened extra_fields struct
pub trait ClientRegistrationResponse<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>:
    ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>
    + Debug
    + DeserializeOwned
    + PartialEq
    + Serialize
where
    AT: ApplicationType,
    CA: ClientAuthMethod,
    G: GrantType,
    JE: JweContentEncryptionAlgorithm,
    JK: JweKeyManagementAlgorithm,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    RT: ResponseType,
    S: SubjectIdentifierType,
{
    fn client_id(&self) -> &ClientId;
    fn client_secret(&self) -> Option<&ClientSecret>;
    fn registration_access_token(&self) -> Option<&RegistrationAccessToken>;
    fn registration_client_uri(&self) -> Option<&ClientConfigUrl>;
    fn client_id_issued_at(&self) -> Option<Result<DateTime<Utc>, ()>>;
    fn client_secret_expires_at(&self) -> Option<Result<DateTime<Utc>, ()>>;
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Registration10ClientRegistrationResponse<AT, CA, CM, G, JE, JK, JS, JT, JU, K, RT, S>
where
    AT: ApplicationType,
    CA: ClientAuthMethod,
    CM: ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>,
    G: GrantType,
    JE: JweContentEncryptionAlgorithm,
    JK: JweKeyManagementAlgorithm,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    RT: ResponseType,
    S: SubjectIdentifierType,
{
    client_id: ClientId,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_secret: Option<ClientSecret>,
    #[serde(skip_serializing_if = "Option::is_none")]
    registration_access_token: Option<RegistrationAccessToken>,
    #[serde(skip_serializing_if = "Option::is_none")]
    registration_client_uri: Option<ClientConfigUrl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id_issued_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_secret_expires_at: Option<u64>,
    #[serde(
        flatten,
        bound = "CM: ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>"
    )]
    client_metadata: CM,
    #[serde(skip)]
    _phantom_at: PhantomData<AT>,
    #[serde(skip)]
    _phantom_ca: PhantomData<CA>,
    #[serde(skip)]
    _phantom_g: PhantomData<G>,
    #[serde(skip)]
    _phantom_je: PhantomData<JE>,
    #[serde(skip)]
    _phantom_jk: PhantomData<JK>,
    #[serde(skip)]
    _phantom_js: PhantomData<JS>,
    #[serde(skip)]
    _phantom_jt: PhantomData<JT>,
    #[serde(skip)]
    _phantom_ju: PhantomData<JU>,
    #[serde(skip)]
    _phantom_jw: PhantomData<K>,
    #[serde(skip)]
    _phantom_rt: PhantomData<RT>,
    #[serde(skip)]
    _phantom_s: PhantomData<S>,
}
impl<AT, CA, CM, G, JE, JK, JS, JT, JU, K, RT, S>
    ClientRegistrationResponse<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>
    for Registration10ClientRegistrationResponse<AT, CA, CM, G, JE, JK, JS, JT, JU, K, RT, S>
where
    AT: ApplicationType,
    CA: ClientAuthMethod,
    CM: ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>,
    G: GrantType,
    JE: JweContentEncryptionAlgorithm,
    JK: JweKeyManagementAlgorithm,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    RT: ResponseType,
    S: SubjectIdentifierType,
{
    fn client_id(&self) -> &ClientId {
        &self.client_id
    }
    fn client_secret(&self) -> Option<&ClientSecret> {
        self.client_secret.as_ref()
    }
    fn registration_access_token(&self) -> Option<&RegistrationAccessToken> {
        self.registration_access_token.as_ref()
    }
    fn registration_client_uri(&self) -> Option<&ClientConfigUrl> {
        self.registration_client_uri.as_ref()
    }
    fn client_id_issued_at(&self) -> Option<Result<DateTime<Utc>, ()>> {
        self.client_id_issued_at
            .map(|seconds| Utc.timestamp_opt(seconds as i64, 0).single().ok_or(()))
    }
    fn client_secret_expires_at(&self) -> Option<Result<DateTime<Utc>, ()>> {
        self.client_secret_expires_at
            .map(|seconds| Utc.timestamp_opt(seconds as i64, 0).single().ok_or(()))
    }
}
impl<AT, CA, CM, G, JE, JK, JS, JT, JU, K, RT, S>
    ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>
    for Registration10ClientRegistrationResponse<AT, CA, CM, G, JE, JK, JS, JT, JU, K, RT, S>
where
    AT: ApplicationType,
    CA: ClientAuthMethod,
    CM: ClientMetadata<AT, CA, G, JE, JK, JS, JT, JU, K, RT, S>,
    G: GrantType,
    JE: JweContentEncryptionAlgorithm,
    JK: JweKeyManagementAlgorithm,
    JS: JwsSigningAlgorithm<JT>,
    JT: JsonWebKeyType,
    JU: JsonWebKeyUse,
    K: JsonWebKey<JS, JT, JU>,
    RT: ResponseType,
    S: SubjectIdentifierType,
{
    field_getters![
        self [self.client_metadata]() {
            redirect_uris[&Vec<RedirectUrl>],
            response_types[Option<&Vec<ResponseTypes<RT>>>],
            grant_types[Option<&Vec<G>>],
            application_type[Option<&AT>],
            contacts[Option<&Vec<ContactEmail>>],
            client_name[Option<&HashMap<Option<LanguageTag>, ClientName>>],
            logo_uri[Option<&HashMap<Option<LanguageTag>, LogoUrl>>],
            client_uri[Option<&HashMap<Option<LanguageTag>, ClientUrl>>],
            policy_uri[Option<&HashMap<Option<LanguageTag>, PolicyUrl>>],
            tos_uri[Option<&HashMap<Option<LanguageTag>, ToSUrl>>],
            jwks_uri[Option<&JsonWebKeySetUrl>],
            jwks[Option<&JsonWebKeySet<JS, JT, JU, K>>],
            sector_identifier_uri[Option<&SectorIdentifierUrl>],
            subject_type[Option<&S>],
            id_token_signed_response_alg[Option<&JS>],
            id_token_encrypted_response_alg[Option<&JK>],
            id_token_encrypted_response_enc[Option<&JE>],
            userinfo_signed_response_alg[Option<&JS>],
            userinfo_encrypted_response_alg[Option<&JK>],
            userinfo_encrypted_response_enc[Option<&JE>],
            request_object_signing_alg[Option<&JS>],
            request_object_encryption_alg[Option<&JK>],
            request_object_encryption_enc[Option<&JE>],
            token_endpoint_auth_method[Option<&CA>],
            token_endpoint_auth_signing_alg[Option<&JS>],
            default_max_age[Option<&Duration>],
            require_auth_time[Option<bool>],
            default_acr_values[Option<&Vec<AuthenticationContextClass>>],
            initiate_login_uri[Option<&InitiateLoginUrl>],
            request_uris[Option<&Vec<RequestUrl>>],
        }
    ];
}

// FIXME: implement client configuration endpoint request (Section 4)

pub trait RegisterErrorResponseType: 'static + Clone + ErrorResponseType + Send + Sync {}

#[derive(Debug, Fail)]
pub enum ClientRegistrationError<T: RegisterErrorResponseType> {
    #[fail(display = "Request error: {}", _0)]
    Request(curl::Error),
    #[fail(display = "Response error (status={}): {}", _0, _1)]
    Response(u32, String),
    #[fail(display = "JSON error: {}", _0)]
    Json(serde_json::Error),
    #[fail(display = "Server response: {}", _0)]
    ServerResponse(ErrorResponse<T>),
    #[fail(display = "Validation error: {}", _0)]
    Validation(String),
    #[fail(display = "Other error: {}", _0)]
    Other(String),
}

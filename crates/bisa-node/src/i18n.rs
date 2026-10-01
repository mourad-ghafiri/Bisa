//! The language a request wants ([17 — Internationalisation]): every person-
//! facing sentence the node itself renders — a setting's label, a git config
//! key's hint, an error body's `error` — is rendered in it.
//!
//! [`Lang`] is the extractor a handler names when it renders words: the
//! `Accept-Language` header negotiated against the languages the platform
//! ships (`bisa_i18n::Locale::from_accept_language`), English when the header
//! is absent or names nothing shipped. The desktop sends the header on every
//! request (`api.ts`), the CLI its own locale; a raw `curl` gets English.
//! Everything else the node sends is a `Text` the reader renders.

use axum::extract::{FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use bisa_i18n::Locale;
use std::convert::Infallible;

/// The request's language, negotiated. Never fails: no header is English.
#[derive(Debug, Clone)]
pub struct Lang(pub Locale);

impl Lang {
    /// The locale a header asks for; none is English.
    pub fn of(header: Option<&str>) -> Lang {
        Lang(match header {
            Some(h) => Locale::from_accept_language(h),
            None => Locale::english(),
        })
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Lang {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Infallible> {
        let header = parts
            .headers
            .get(axum::http::header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok());
        Ok(Lang::of(header))
    }
}

/// What an `ApiError` leaves on its response for [`localize`]: the status
/// and the body with its `error` in English, to be re-said in the request's
/// language.
#[derive(Clone)]
pub(crate) struct Pending {
    pub status: StatusCode,
    pub body: crate::dto::ErrorBody,
}

/// The one place an error body meets the request's language: a response an
/// `ApiError` produced, with a language other than English asked for, has its
/// `error` rendered again from its `Text`; every other response passes.
pub(crate) async fn localize(req: Request, next: Next) -> Response {
    let lang = Lang::of(
        req.headers()
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok()),
    );
    let mut response = next.run(req).await;
    if lang.0.tag() == bisa_i18n::DEFAULT {
        return response;
    }
    let Some(pending) = response.extensions_mut().remove::<Pending>() else {
        return response;
    };
    let mut body = pending.body;
    body.error = bisa_i18n::render(&lang.0, &body.text);
    let mut said = (pending.status, Json(body)).into_response();
    // The headers the refusal came with — a 401's `WWW-Authenticate` — stay.
    for (name, value) in response.headers() {
        if name != header::CONTENT_TYPE && name != header::CONTENT_LENGTH {
            said.headers_mut().insert(name.clone(), value.clone());
        }
    }
    said
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_header_is_english_and_a_header_is_negotiated() {
        assert_eq!(Lang::of(None).0.tag(), "en");
        assert_eq!(Lang::of(Some("fr-FR, en;q=0.5")).0.tag(), "en");
        assert_eq!(Lang::of(Some("xx")).0.tag(), "en");
    }
}

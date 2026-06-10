use url::{Url, form_urlencoded};

pub(crate) const REDACTED_VALUE: &str = "<redacted>";

/// Returns display text with URL-contained secrets masked where possible.
pub(crate) fn redact_text_secrets(value: &str) -> String {
  let mut redacted = String::with_capacity(value.len());
  let mut changed = false;
  let mut start = 0;

  for (index, ch) in value.char_indices() {
    if ch.is_whitespace() {
      let token = &value[start..index];
      let redacted_token = redact_token_secrets(token);
      changed |= redacted_token != token;
      redacted.push_str(&redacted_token);
      redacted.push(ch);
      start = index + ch.len_utf8();
    }
  }

  let token = &value[start..];
  let redacted_token = redact_token_secrets(token);
  changed |= redacted_token != token;
  redacted.push_str(&redacted_token);

  if changed { redacted } else { value.to_string() }
}

/// Returns a display-safe URL with userinfo and sensitive query values masked.
pub(crate) fn redact_url_secrets(value: &str) -> String {
  let Ok(url) = Url::parse(value) else {
    return value.to_string();
  };

  let has_userinfo = !url.username().is_empty() || url.password().is_some();
  let redacted_query = redact_sensitive_query(url.query());
  if !has_userinfo && redacted_query.is_none() {
    return value.to_string();
  }

  let mut display = url.as_str().to_string();
  if let Some(query) = redacted_query {
    display = replace_url_query(&display, &query);
  }
  if has_userinfo {
    display = replace_url_userinfo(&display);
  }
  display
}

fn redact_token_secrets(token: &str) -> String {
  let Some(start) = url_scheme_start(token) else {
    return token.to_string();
  };
  let (prefix, candidate) = token.split_at(start);
  let (url, suffix) = split_url_suffix(candidate);
  let redacted_url = redact_url_secrets(url);
  if redacted_url == url {
    token.to_string()
  } else {
    format!("{prefix}{redacted_url}{suffix}")
  }
}

fn url_scheme_start(value: &str) -> Option<usize> {
  let lower = value.to_ascii_lowercase();
  [
    "ftp://", "http://", "https://", "sftp://", "ws://", "wss://",
  ]
  .iter()
  .filter_map(|scheme| lower.find(scheme))
  .min()
}

fn split_url_suffix(value: &str) -> (&str, &str) {
  let mut end = value.len();
  while end > 0 {
    let ch = value[..end]
      .chars()
      .next_back()
      .expect("non-empty slice should have last character");
    if !matches!(ch, ',' | '.' | ';' | ')' | ']' | '}' | '"' | '\'') {
      break;
    }
    end -= ch.len_utf8();
  }
  (&value[..end], &value[end..])
}

fn redact_sensitive_query(query: Option<&str>) -> Option<String> {
  let query = query?;
  let mut changed = false;
  let mut pairs = Vec::new();

  for segment in query.split('&') {
    let key = segment.split_once('=').map_or(segment, |(key, _)| key);
    let decoded_key = form_urlencoded::parse(key.as_bytes())
      .next()
      .map_or_else(|| key.to_string(), |(key, _)| key.into_owned());
    if is_sensitive_query_key(&decoded_key) {
      changed = true;
      pairs.push(format!("{key}={REDACTED_VALUE}"));
    } else {
      pairs.push(segment.to_string());
    }
  }

  changed.then(|| pairs.join("&"))
}

fn is_sensitive_query_key(key: &str) -> bool {
  matches!(
    normalize_identifier(key).as_str(),
    "accesstoken"
      | "apikey"
      | "authorization"
      | "authorizationkey"
      | "basicauthpassword"
      | "clientsecret"
      | "idtag"
      | "idtoken"
      | "password"
      | "passwd"
      | "refreshtoken"
      | "secret"
      | "token"
  )
}

fn normalize_identifier(text: &str) -> String {
  text
    .chars()
    .filter(char::is_ascii_alphanumeric)
    .map(|ch| ch.to_ascii_lowercase())
    .collect()
}

fn replace_url_query(url: &str, query: &str) -> String {
  let Some(query_start) = url.find('?') else {
    return url.to_string();
  };
  let query_end = url[query_start + 1..]
    .find('#')
    .map_or(url.len(), |index| query_start + 1 + index);
  format!("{}{}{}", &url[..=query_start], query, &url[query_end..])
}

fn replace_url_userinfo(url: &str) -> String {
  let authority_start = url.find("://").map_or(0, |index| index + 3);
  let authority_end = url[authority_start..]
    .find(['/', '?', '#'])
    .map_or(url.len(), |index| authority_start + index);
  let authority = &url[authority_start..authority_end];
  let Some(at_index) = authority.rfind('@') else {
    return url.to_string();
  };
  let host_start = authority_start + at_index + 1;
  format!(
    "{}{}@{}",
    &url[..authority_start],
    REDACTED_VALUE,
    &url[host_start..]
  )
}

#[cfg(test)]
mod tests {
  use super::{redact_text_secrets, redact_url_secrets};

  #[test]
  fn redacts_url_userinfo() {
    let redacted =
      redact_url_secrets("wss://user:secret@example.test/ocpp?safe=1");

    assert_eq!(redacted, "wss://<redacted>@example.test/ocpp?safe=1");
  }

  #[test]
  fn redacts_sensitive_url_query_values() {
    let redacted =
      redact_url_secrets("https://example.test/log?token=SECRET&safe=1");

    assert_eq!(redacted, "https://example.test/log?token=<redacted>&safe=1");
  }

  #[test]
  fn leaves_non_url_text_unchanged() {
    assert_eq!(redact_url_secrets("not a url"), "not a url");
  }

  #[test]
  fn redacts_url_secrets_inside_text() {
    let redacted = redact_text_secrets(
      "Connecting to wss://user:secret@example.test/ocpp?token=SECRET.",
    );

    assert_eq!(
      redacted,
      "Connecting to wss://<redacted>@example.test/ocpp?token=<redacted>."
    );
  }
}

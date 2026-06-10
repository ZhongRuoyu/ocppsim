use serde_json::Value;

use crate::sensitive::{REDACTED_VALUE, redact_text_secrets};

use super::{
  OcppFrame, build_call, build_call_error, build_call_result, json,
  normalize_identifier, parse_frame,
};

pub(in crate::simulator) fn sanitized_trace_frame_text(text: &str) -> String {
  parse_frame(text).map_or_else(
    |_| "OCPP frame omitted from trace.".to_string(),
    |frame| sanitized_trace_frame(&frame),
  )
}

pub(in crate::simulator) fn sanitized_trace_frame(frame: &OcppFrame) -> String {
  match frame {
    OcppFrame::Call {
      message_id,
      action,
      payload,
    } => build_call(message_id, action, &redact_call_payload(action, payload)),
    OcppFrame::CallResult {
      message_id,
      payload,
    } => build_call_result(message_id, &redact_sensitive_fields(payload)),
    OcppFrame::CallError {
      message_id,
      code,
      description,
      details,
    }
    | OcppFrame::CallResultError {
      message_id,
      code,
      description,
      details,
    } => build_call_error(
      message_id,
      code,
      description,
      &redact_sensitive_fields(details),
    ),
    OcppFrame::Send {
      message_id,
      action,
      payload,
    } => json!([6, message_id, action, redact_call_payload(action, payload)])
      .to_string(),
    OcppFrame::Unsupported {
      message_type,
      message_id,
    } => json!([message_type, message_id]).to_string(),
  }
}

pub(in crate::simulator) fn sanitized_trace_details(details: &Value) -> String {
  redact_sensitive_fields(details).to_string()
}

pub(in crate::simulator) fn sanitized_trace_payload(
  action: &str,
  payload: &Value,
) -> String {
  redact_call_payload(action, payload).to_string()
}

fn redact_call_payload(action: &str, payload: &Value) -> Value {
  let mut redacted = redact_sensitive_fields(payload);
  match action {
    "ChangeConfiguration" => redact_change_configuration(&mut redacted),
    "SetNetworkProfile" => redact_set_network_profile(&mut redacted),
    "SetVariables" => redact_set_variables(&mut redacted),
    _ => {}
  }
  redacted
}

fn redact_change_configuration(payload: &mut Value) {
  let Some(key) = payload.get("key").and_then(Value::as_str) else {
    return;
  };
  if is_secret_variable(key)
    && let Some(object) = payload.as_object_mut()
  {
    object.insert(
      "value".to_string(),
      Value::String(REDACTED_VALUE.to_string()),
    );
  }
}

fn redact_set_variables(payload: &mut Value) {
  let Some(entries) = payload
    .get_mut("setVariableData")
    .and_then(Value::as_array_mut)
  else {
    return;
  };
  for entry in entries {
    let variable_name = entry
      .get("variable")
      .and_then(Value::as_object)
      .and_then(|variable| variable.get("name"))
      .and_then(Value::as_str);
    if variable_name.is_some_and(is_secret_variable)
      && let Some(object) = entry.as_object_mut()
    {
      object.insert(
        "attributeValue".to_string(),
        Value::String(REDACTED_VALUE.to_string()),
      );
    }
  }
}

fn redact_set_network_profile(payload: &mut Value) {
  let Some(vpn) = payload
    .get_mut("connectionData")
    .and_then(Value::as_object_mut)
    .and_then(|connection_data| connection_data.get_mut("vpn"))
    .and_then(Value::as_object_mut)
  else {
    return;
  };
  if vpn.contains_key("key") {
    vpn.insert("key".to_string(), Value::String(REDACTED_VALUE.to_string()));
  }
}

fn redact_sensitive_fields(value: &Value) -> Value {
  match value {
    Value::Object(object) => Value::Object(
      object
        .iter()
        .map(|(key, value)| {
          let redacted_value =
            if is_secret_variable(key) || is_token_field(key, value) {
              Value::String(REDACTED_VALUE.to_string())
            } else {
              redact_sensitive_fields(value)
            };
          (key.clone(), redacted_value)
        })
        .collect(),
    ),
    Value::Array(items) => {
      Value::Array(items.iter().map(redact_sensitive_fields).collect())
    }
    Value::String(text) => Value::String(redact_text_secrets(text)),
    _ => value.clone(),
  }
}

fn is_secret_variable(value: &str) -> bool {
  matches!(
    normalize_identifier(value).as_str(),
    "apnpassword"
      | "authorizationkey"
      | "basicauthpassword"
      | "password"
      | "simpin"
  )
}

fn is_token_field(key: &str, value: &Value) -> bool {
  matches!(
    normalize_identifier(key).as_str(),
    "additionalidtoken" | "groupidtoken" | "idtag" | "idtoken" | "parentidtag"
  ) && value.is_string()
}

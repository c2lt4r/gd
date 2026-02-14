use super::{DaemonResponse, DaemonServer, error_response};

pub fn dispatch_hover(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(file) = params.get("file").and_then(|f| f.as_str()) else {
        return error_response("missing 'file' parameter");
    };
    let Some(line) = params.get("line").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'line' parameter");
    };
    let Some(column) = params.get("column").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'column' parameter");
    };

    let godot = server.godot.lock().unwrap();
    match crate::lsp::query::query_hover(file, line as usize, column as usize, godot.as_ref()) {
        Ok(output) => match serde_json::to_value(&output) {
            Ok(val) => DaemonResponse {
                result: Some(val),
                error: None,
            },
            Err(e) => error_response(&format!("serialize error: {e}")),
        },
        Err(e) => error_response(&format!("{e}")),
    }
}

pub fn dispatch_completion(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(file) = params.get("file").and_then(|f| f.as_str()) else {
        return error_response("missing 'file' parameter");
    };
    let Some(line) = params.get("line").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'line' parameter");
    };
    let Some(column) = params.get("column").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'column' parameter");
    };

    let godot = server.godot.lock().unwrap();
    match crate::lsp::query::query_completions(file, line as usize, column as usize, godot.as_ref())
    {
        Ok(output) => match serde_json::to_value(&output) {
            Ok(val) => DaemonResponse {
                result: Some(val),
                error: None,
            },
            Err(e) => error_response(&format!("serialize error: {e}")),
        },
        Err(e) => error_response(&format!("{e}")),
    }
}

pub fn dispatch_definition(server: &DaemonServer, params: &serde_json::Value) -> DaemonResponse {
    let Some(file) = params.get("file").and_then(|f| f.as_str()) else {
        return error_response("missing 'file' parameter");
    };
    let Some(line) = params.get("line").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'line' parameter");
    };
    let Some(column) = params.get("column").and_then(serde_json::Value::as_u64) else {
        return error_response("missing 'column' parameter");
    };

    let godot = server.godot.lock().unwrap();
    match crate::lsp::query::query_definition(file, line as usize, column as usize, godot.as_ref())
    {
        Ok(output) => match serde_json::to_value(&output) {
            Ok(val) => DaemonResponse {
                result: Some(val),
                error: None,
            },
            Err(e) => error_response(&format!("serialize error: {e}")),
        },
        Err(e) => error_response(&format!("{e}")),
    }
}

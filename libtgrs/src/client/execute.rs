//! TDLib requests that run synchronously, without a client: logging set up
//! before the first client starts. tdlib_rs only binds the asynchronous calls.

use std::ffi::{CStr, CString, c_char};

use serde_json::{Value, json};

use super::TdLogging;

unsafe extern "C" {
    fn td_execute(request: *const c_char) -> *const c_char;
}

/// Runs a synchronous TDLib request and returns its result.
pub(crate) fn execute(request: &Value) -> Result<Value, String> {
    let request = CString::new(request.to_string()).map_err(|e| e.to_string())?;
    // SAFETY: `request` is a valid C string for the duration of the call. The
    // result is null or a C string owned by TDLib, valid until the next
    // `td_execute` on this thread, so it is copied right away.
    let response = unsafe {
        let response = td_execute(request.as_ptr());
        if response.is_null() {
            return Err("TDLib returned no response".into());
        }
        CStr::from_ptr(response).to_string_lossy().into_owned()
    };
    let response: Value = serde_json::from_str(&response).map_err(|e| e.to_string())?;
    if response["@type"] == "error" {
        return Err(response["message"]
            .as_str()
            .unwrap_or("unknown error")
            .into());
    }
    Ok(response)
}

/// Sets TDLib's log verbosity and log file for every client. Called before the
/// first client is created, so TDLib doesn't log its start-up to the terminal.
pub fn configure_logging(logging: &TdLogging) -> Result<(), String> {
    execute(&json!({
        "@type": "setLogVerbosityLevel",
        "new_verbosity_level": logging.verbosity_level,
    }))?;
    execute(&json!({
        "@type": "setLogStream",
        "log_stream": {
            "@type": "logStreamFile",
            "path": logging.path.to_string_lossy(),
            "max_file_size": 1_i64 << 27,
            "redirect_stderr": logging.redirect_stderr,
        },
    }))?;
    Ok(())
}

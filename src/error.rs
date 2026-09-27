/// Why a hook payload could not be parsed.
///
/// Only input that is not a JSON object fails. Unknown events, unknown or
/// wrong-typed fields and unknown tools parse.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The input is not valid JSON.
    #[error("hook payload is not JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// The input is JSON but not an object.
    #[error("hook payload is not a JSON object")]
    NotObject,
}

use std::io::{self, Read};

const KEYRING_SERVICE: &str = "com.sabitech.sbtdesktool.translation-sync";
const TOKEN_ACCOUNT: &str = "enterprise-api-token";

fn main() -> Result<(), String> {
    if std::env::args().any(|argument| argument == "--check") {
        keyring::Entry::new(KEYRING_SERVICE, TOKEN_ACCOUNT)
            .map_err(|error| error.to_string())?
            .get_password()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }
    let mut token = String::new();
    io::stdin()
        .read_to_string(&mut token)
        .map_err(|error| error.to_string())?;
    if token.trim().is_empty() {
        return Err("Sync token is required on stdin".into());
    }
    keyring::Entry::new(KEYRING_SERVICE, TOKEN_ACCOUNT)
        .map_err(|error| error.to_string())?
        .set_password(token.trim())
        .map_err(|error| error.to_string())
}

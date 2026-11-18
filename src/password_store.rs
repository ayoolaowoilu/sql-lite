use keyring::{Entry, Error};

const SERVICE: &str = "MySQLite";
const USER: &str = "mysql-root-password";

pub fn load_password() -> Result<Option<String>, String> {
    let entry = Entry::new(SERVICE, USER).map_err(|error| error.to_string())?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(Error::NoEntry) => match std::env::var("MYSQL_PASSWORD") {
            Ok(password) => {
                if !(8..=128).contains(&password.chars().count()) {
                    return Err(
                        "MYSQL_PASSWORD must be between 8 and 128 characters to migrate."
                            .to_string(),
                    );
                }
                save_password(&password)?;
                Ok(Some(password))
            }
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(error) => Err(format!("Could not read MYSQL_PASSWORD: {}", error)),
        },
        Err(error) => Err(error.to_string()),
    }
}

pub fn save_password(password: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, USER).map_err(|error| error.to_string())?;
    entry
        .set_password(password)
        .map_err(|error| error.to_string())
}

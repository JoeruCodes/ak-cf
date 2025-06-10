use worker::{Request, Response, Result, D1Database, Env};
use crate::sql::{self, UserCredentials};
use sha2::{Sha256, Digest};

pub async fn authenticate_user(req: &Request, env: &Env) -> Result<Option<String>> {
    let Some(username_header) = req.headers().get("username")? else {
        return Ok(None);
    };
    let Some(password_header) = req.headers().get("password")? else {
        return Ok(None);
    };

    let db = env.d1("D1_DATABASE")?;
    
    match sql::get_user_credentials(&db, &username_header).await {
        Ok(Some(UserCredentials { user_id, user_name, password })) => {
            let mut hasher = Sha256::new();
            hasher.update(password_header.as_bytes());
            let password_hash = hex::encode(hasher.finalize());

            if (user_name.as_ref().map(|s| s == &username_header).unwrap_or(false) || user_id == username_header) && password == password_hash {
                Ok(Some(user_id)) // Authentication successful, return the canonical user_id
            } else {
                Ok(None) // Invalid credentials
            }
        }
        Ok(None) => Ok(None), // User not found
        Err(e) => {
            // Log the database error and treat as an authentication failure.
            worker::console_error!("Authentication DB error: {:?}", e);
            Err(e)
        }
    }
} 
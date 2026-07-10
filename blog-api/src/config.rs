//! Environment-driven configuration.

/// Runtime configuration loaded from environment variables.
#[derive(Debug)]
pub struct Config {
    pub db_path: String,
    pub admin_token: String,
    pub bind_addr: String,
    pub dev_cors_origin: Option<String>,
    /// When true, new comments are published immediately instead of held
    /// as `pending` for admin approval. Off by default (moderation on).
    pub auto_approve: bool,
}

/// Parse a boolean-ish env value: `1`, `true`, `yes`, `on` (any case) are
/// true; everything else (including unset/empty) is false.
fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

impl Config {
    /// Load configuration from the process environment.
    ///
    /// `BLOG_API_DB_PATH` is required unless `STATE_DIRECTORY` is set, in
    /// which case the DB path falls back to `${STATE_DIRECTORY}/blog.db`
    /// (this matches systemd's `StateDirectory=` convention).
    pub fn from_env() -> Result<Self, String> {
        let db_path = match std::env::var("BLOG_API_DB_PATH") {
            Ok(v) if !v.is_empty() => v,
            _ => match std::env::var("STATE_DIRECTORY") {
                Ok(dir) if !dir.is_empty() => format!("{}/blog.db", dir.trim_end_matches('/')),
                _ => {
                    return Err(
                        "BLOG_API_DB_PATH must be set (or STATE_DIRECTORY as a fallback)"
                            .to_string(),
                    )
                }
            },
        };

        let admin_token = std::env::var("BLOG_API_ADMIN_TOKEN")
            .map_err(|_| "BLOG_API_ADMIN_TOKEN must be set".to_string())?;
        if admin_token.is_empty() {
            return Err("BLOG_API_ADMIN_TOKEN must not be empty".to_string());
        }

        let bind_addr =
            std::env::var("BLOG_API_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8787".to_string());

        let dev_cors_origin = std::env::var("BLOG_API_DEV_CORS_ORIGIN")
            .ok()
            .filter(|s| !s.is_empty());

        let auto_approve = env_flag("BLOG_API_AUTO_APPROVE");

        Ok(Config {
            db_path,
            admin_token,
            bind_addr,
            dev_cors_origin,
            auto_approve,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Environment variable mutation must be serialized across tests in this
    // module since `std::env` is process-global.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_env() {
        for var in [
            "BLOG_API_DB_PATH",
            "BLOG_API_ADMIN_TOKEN",
            "BLOG_API_BIND_ADDR",
            "BLOG_API_DEV_CORS_ORIGIN",
            "BLOG_API_AUTO_APPROVE",
            "STATE_DIRECTORY",
        ] {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn requires_admin_token() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("BLOG_API_DB_PATH", "/tmp/blog.db");
        let err = Config::from_env().unwrap_err();
        assert!(err.contains("ADMIN_TOKEN"));
        clear_env();
    }

    #[test]
    fn falls_back_to_state_directory() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("STATE_DIRECTORY", "/var/lib/blog-api");
        std::env::set_var("BLOG_API_ADMIN_TOKEN", "secret");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.db_path, "/var/lib/blog-api/blog.db");
        assert_eq!(cfg.bind_addr, "127.0.0.1:8787");
        // Moderation is on by default: comments are not auto-approved.
        assert!(!cfg.auto_approve);
        clear_env();
    }

    #[test]
    fn auto_approve_flag_parses_truthy_values() {
        let _guard = ENV_LOCK.lock().unwrap();
        for truthy in ["1", "true", "TRUE", "yes", "on"] {
            clear_env();
            std::env::set_var("BLOG_API_DB_PATH", "/tmp/blog.db");
            std::env::set_var("BLOG_API_ADMIN_TOKEN", "secret");
            std::env::set_var("BLOG_API_AUTO_APPROVE", truthy);
            assert!(
                Config::from_env().unwrap().auto_approve,
                "{truthy:?} should enable auto-approve"
            );
        }
        for falsy in ["0", "false", "no", "off", ""] {
            clear_env();
            std::env::set_var("BLOG_API_DB_PATH", "/tmp/blog.db");
            std::env::set_var("BLOG_API_ADMIN_TOKEN", "secret");
            std::env::set_var("BLOG_API_AUTO_APPROVE", falsy);
            assert!(
                !Config::from_env().unwrap().auto_approve,
                "{falsy:?} should leave auto-approve off"
            );
        }
        clear_env();
    }

    #[test]
    fn explicit_db_path_wins() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("BLOG_API_DB_PATH", "/data/blog.db");
        std::env::set_var("BLOG_API_ADMIN_TOKEN", "secret");
        std::env::set_var("BLOG_API_BIND_ADDR", "0.0.0.0:9000");
        std::env::set_var("BLOG_API_DEV_CORS_ORIGIN", "http://localhost:1111");
        let cfg = Config::from_env().unwrap();
        assert_eq!(cfg.db_path, "/data/blog.db");
        assert_eq!(cfg.bind_addr, "0.0.0.0:9000");
        assert_eq!(
            cfg.dev_cors_origin.as_deref(),
            Some("http://localhost:1111")
        );
        clear_env();
    }
}

pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub seaweedfs_filer: String,
    pub allowed_origin: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let config = Self {
            database_url: std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL not set")?,
            jwt_secret: std::env::var("JWT_SECRET").map_err(|_| "JWT_SECRET not set")?,
            seaweedfs_filer: std::env::var("SEAWEEDFS_FILER")
                .unwrap_or_else(|_| "http://localhost:8888".to_string()),
            allowed_origin: std::env::var("ALLOWED_ORIGIN")
                .map_err(|_| "ALLOWED_ORIGIN not set")?,
        };
        if config.jwt_secret.len() < 32 {
            return Err("JWT_SECRET must contain at least 32 bytes".to_string());
        }
        Ok(config)
    }
}

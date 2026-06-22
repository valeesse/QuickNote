pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub seaweedfs_filer: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL")
                .map_err(|_| "DATABASE_URL not set")?,
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "dev-secret".to_string()),
            seaweedfs_filer: std::env::var("SEAWEEDFS_FILER")
                .unwrap_or_else(|_| "http://localhost:8888".to_string()),
        })
    }
}

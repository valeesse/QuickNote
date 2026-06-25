pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub seaweedfs_filer: String,
    pub allowed_origin: String,
    pub cookie_secure: bool,
    pub billing_provider: Option<String>,
    pub billing_public_origin: String,
    pub billing_manage_url: Option<String>,
    pub billing_support_email: Option<String>,
    pub lemonsqueezy_api_key: Option<String>,
    pub lemonsqueezy_store_id: Option<String>,
    pub lemonsqueezy_webhook_secret: Option<String>,
    pub lemonsqueezy_monthly_variant_id: Option<String>,
    pub lemonsqueezy_yearly_variant_id: Option<String>,
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
            cookie_secure: false,
            billing_provider: std::env::var("BILLING_PROVIDER").ok(),
            billing_public_origin: std::env::var("BILLING_PUBLIC_ORIGIN")
                .or_else(|_| std::env::var("PUBLIC_ORIGIN"))
                .unwrap_or_else(|_| "http://localhost:8081".to_string()),
            billing_manage_url: std::env::var("BILLING_MANAGE_URL").ok(),
            billing_support_email: std::env::var("BILLING_SUPPORT_EMAIL").ok(),
            lemonsqueezy_api_key: std::env::var("LEMONSQUEEZY_API_KEY").ok(),
            lemonsqueezy_store_id: std::env::var("LEMONSQUEEZY_STORE_ID").ok(),
            lemonsqueezy_webhook_secret: std::env::var("LEMONSQUEEZY_WEBHOOK_SECRET").ok(),
            lemonsqueezy_monthly_variant_id: std::env::var("LEMONSQUEEZY_MONTHLY_VARIANT_ID").ok(),
            lemonsqueezy_yearly_variant_id: std::env::var("LEMONSQUEEZY_YEARLY_VARIANT_ID").ok(),
        };
        if config.jwt_secret.len() < 32 {
            return Err("JWT_SECRET must contain at least 32 bytes".to_string());
        }
        Ok(Self {
            cookie_secure: config.allowed_origin.starts_with("https://"),
            ..config
        })
    }
}

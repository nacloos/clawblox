use http::HeaderMap;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::Region;
use std::sync::Arc;

#[derive(Clone)]
pub struct R2Client {
    bucket: Arc<Bucket>,
    public_url: String,
}

impl R2Client {
    /// Initialize from env vars. Returns None if R2 is not configured.
    pub fn from_env() -> Option<Self> {
        let account_id = std::env::var("R2_ACCOUNT_ID").ok()?;
        let access_key = std::env::var("R2_ACCESS_KEY_ID").ok()?;
        let secret_key = std::env::var("R2_SECRET_ACCESS_KEY").ok()?;
        let bucket_name = std::env::var("R2_BUCKET").ok()?;
        let public_url = std::env::var("R2_PUBLIC_URL").ok()?;

        let credentials = Credentials::new(
            Some(&access_key),
            Some(&secret_key),
            None,
            None,
            None,
        )
        .ok()?;

        let region = Region::R2 { account_id };

        let bucket = Bucket::new(&bucket_name, region, credentials)
            .ok()?
            .with_path_style();

        Some(Self {
            bucket: Arc::new(*bucket),
            public_url,
        })
    }

    /// Upload a file to R2. Returns the public URL.
    pub async fn upload(
        &self,
        key: &str,
        data: &[u8],
        content_type: &str,
    ) -> Result<String, String> {
        let mut custom_headers = HeaderMap::new();
        custom_headers.insert(
            "Cache-Control",
            "public, max-age=31536000, immutable"
                .parse()
                .unwrap(),
        );
        custom_headers.insert(
            "Content-Disposition",
            "attachment".parse().unwrap(),
        );

        self.bucket
            .put_object_with_content_type_and_headers(
                key,
                data,
                content_type,
                Some(custom_headers),
            )
            .await
            .map_err(|e| format!("R2 upload failed: {}", e))?;

        Ok(self.public_url(key))
    }

    pub fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_url.trim_end_matches('/'), key)
    }

    pub fn base_url(&self) -> &str {
        &self.public_url
    }
}

use aws_config::Region;
use aws_credential_types::Credentials;
use aws_sdk_s3::{config::Builder, Client};
use std::env;

pub struct R2Storage {
    client: Client,
    bucket: String,
}

impl R2Storage {
    pub async fn new() -> Result<Self, String> {
        let endpoint = env::var("R2_ENDPOINT")
            .map_err(|_| "R2_ENDPOINT not set")?;
        let access_key = env::var("R2_ACCESS_KEY_ID")
            .map_err(|_| "R2_ACCESS_KEY_ID not set")?;
        let secret_key = env::var("R2_SECRET_ACCESS_KEY")
            .map_err(|_| "R2_SECRET_ACCESS_KEY not set")?;
        let bucket = env::var("R2_BUCKET_NAME")
            .map_err(|_| "R2_BUCKET_NAME not set")?;

        let credentials = Credentials::new(
            access_key,
            secret_key,
            None,
            None,
            "r2",
        );

        let config = Builder::new()
            .endpoint_url(endpoint)
            .region(Region::new("auto"))
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();

        let client = Client::from_conf(config);

        Ok(Self { client, bucket })
    }

    pub async fn upload_wasm(&self, game_id: &str, wasm_bytes: Vec<u8>) -> Result<String, String> {
        let key = format!("games/{}.wasm", game_id);

        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(wasm_bytes.into())
            .content_type("application/wasm")
            .send()
            .await
            .map_err(|e| format!("Failed to upload WASM: {}", e))?;

        Ok(key)
    }

    pub async fn download_wasm(&self, game_id: &str) -> Result<Vec<u8>, String> {
        let key = format!("games/{}.wasm", game_id);

        let response = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| format!("Failed to download WASM: {}", e))?;

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|e| format!("Failed to read WASM body: {}", e))?
            .into_bytes()
            .to_vec();

        Ok(bytes)
    }

    pub async fn delete_wasm(&self, game_id: &str) -> Result<(), String> {
        let key = format!("games/{}.wasm", game_id);

        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| format!("Failed to delete WASM: {}", e))?;

        Ok(())
    }
}

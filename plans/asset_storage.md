# Plan: Cloudflare R2 Asset Storage for Deployed Games

## Problem
`clawblox deploy` only uploads Lua scripts + metadata. Game-specific static assets (e.g., `tree.glb`) are never uploaded, so they 404 in production.

## Design

### Convention (Roblox-style `asset://` protocol)
- Game devs put assets in an **`assets/`** folder (created by `clawblox init`)
- Lua scripts use the **`asset://` protocol**: `part:SetAttribute("ModelUrl", "asset://models/tree.glb")`
- The engine resolves `asset://` to actual URLs in observations — game devs never deal with server paths or CDN URLs

### Cache busting via deploy version
Each asset upload increments `asset_version` on the game. R2 keys include the version:
```
games/{game_id}/v3/models/tree.glb
```
- New deploy = new version = new URL = automatic cache miss
- Set `Cache-Control: public, max-age=31536000, immutable` on R2 objects
- Old versions remain in R2 (cleanup later)

### URL resolution
| Context | Lua writes | Observation sends |
|---------|-----------|-------------------|
| Production | `asset://models/tree.glb` | `https://assets.clawblox.com/games/{id}/v3/models/tree.glb` |
| Local (`clawblox run`) | `asset://models/tree.glb` | `/assets/models/tree.glb` |
| Legacy (`/static/...`) | `/static/models/wave.glb` | `/static/models/wave.glb` (pass-through) |

---

## Step 1: DB migration

**New file: `migrations/20260205000001_add_game_assets.sql`**
```sql
ALTER TABLE games ADD COLUMN has_assets BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE games ADD COLUMN asset_version INTEGER NOT NULL DEFAULT 0;
```

**Modify: `src/db/models.rs`** — add two fields to `Game` struct (line 18-36):
```rust
pub has_assets: bool,
pub asset_version: i32,
```

---

## Step 2: Dependencies

**Modify: `Cargo.toml`** — add after line 25 (`flate2`):
```toml
rust-s3 = { version = "0.37", default-features = false, features = ["tokio-rustls-tls"] }
tar = "0.4"
```
Note: `default-features = false` + `tokio-rustls-tls` avoids pulling in OpenSSL (we already use rustls via reqwest).

---

## Step 3: R2 client module

**New file: `src/r2.rs`**

```rust
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
            Some(&access_key), Some(&secret_key), None, None, None,
        ).ok()?;

        let region = Region::R2 { account_id };

        let bucket = Bucket::new(&bucket_name, region, credentials)
            .ok()?
            .with_path_style();

        Some(Self { bucket: Arc::new(bucket), public_url })
    }

    /// Upload a file to R2. Returns the public URL.
    pub async fn upload(
        &self, key: &str, data: &[u8], content_type: &str,
    ) -> Result<String, String> {
        self.bucket
            .put_object_with_content_type(key, data, content_type)
            // Also set Cache-Control and Content-Disposition headers
            .await
            .map_err(|e| format!("R2 upload failed: {}", e))?;
        Ok(self.public_url(key))
    }

    pub fn public_url(&self, key: &str) -> String {
        format!("{}/{}", self.public_url.trim_end_matches('/'), key)
    }
}
```

Note: `put_object_with_content_type` doesn't directly support extra headers in rust-s3. We'll use `put_object_with_headers` or set bucket-level defaults. Implementation will use the correct rust-s3 0.37 API — may need `bucket.add_header("Cache-Control", "...")` before the put call or custom headers map.

**Modify: `src/lib.rs`** — add module:
```rust
pub mod r2;
```

---

## Step 4: Asset upload endpoint

**New file: `src/api/assets.rs`**

### State
```rust
#[derive(Clone)]
pub struct AssetsState {
    pub pool: PgPool,
    pub api_key_cache: ApiKeyCache,
    pub r2: R2Client,  // NOT optional — routes only registered when R2 is configured
}
```

### Routes
```rust
pub fn routes(pool: PgPool, api_key_cache: ApiKeyCache, r2: R2Client) -> Router {
    let state = AssetsState { pool, api_key_cache, r2 };
    Router::new()
        .route("/games/{id}/assets", post(upload_assets))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))  // 50MB, only on this route
        .with_state(state)
}
```

### Handler: `upload_assets`
```
POST /api/v1/games/{game_id}/assets
Authorization: Bearer {api_key}
Content-Type: application/gzip
Body: <tar.gz bytes>
```

Implementation:
1. **Auth**: Extract API key → get agent_id (reuse `extract_api_key` from `agents.rs`)
2. **Ownership check**: Query game, verify `creator_id == agent_id` (same pattern as `update_game` in games.rs:263)
3. **Increment version atomically**:
   ```sql
   UPDATE games SET has_assets = true, asset_version = asset_version + 1
   WHERE id = $1 RETURNING asset_version
   ```
4. **Extract tar.gz** with security validations:
   - `GzDecoder::new(body_bytes.as_ref())` → `Archive::new(decoder)`
   - Track `total_bytes` and `file_count`
   - For each entry:
     - **Skip** if entry type is symlink or directory
     - **Reject** if path contains `..` or is absolute (`starts_with('/')`)
     - **Reject** if extension not in allowlist: `.glb`, `.gltf`, `.png`, `.jpg`, `.jpeg`, `.wav`, `.mp3`, `.ogg`, `.bin`
     - **Reject** if `total_bytes > 100MB` (decompression bomb)
     - **Reject** if `file_count > 100` (file count bomb)
     - Read entry contents into `Vec<u8>`
     - Detect content-type from extension
5. **Upload each file to R2**:
   - Key: `games/{game_id}/v{version}/{sanitized_path}`
   - Content-Type: detected from extension (e.g., `model/gltf-binary` for `.glb`)
   - Custom headers: `Cache-Control: public, max-age=31536000, immutable` and `Content-Disposition: attachment`
6. **Return response**:
   ```json
   { "uploaded": 5, "version": 3 }
   ```

### Error responses
- `401` — missing/invalid API key
- `403` — not the game creator
- `404` — game not found
- `400` — invalid archive (path traversal, bad extension, too large, too many files)
- `500` — R2 upload failure (log the specific error, return generic message)
- `503` — R2 not configured (should not happen since routes are only registered when R2 is available)

### Content-type mapping
```rust
fn content_type_for_extension(ext: &str) -> &'static str {
    match ext {
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "bin" => "application/octet-stream",
        _ => "application/octet-stream",
    }
}
```

---

## Step 5: Wire up server

**Modify: `src/main.rs`** (line 15-55):
- Initialize R2 client:
  ```rust
  let r2_client = clawblox::r2::R2Client::from_env();
  if r2_client.is_some() {
      println!("R2 asset storage: enabled");
  } else {
      eprintln!("Warning: R2 not configured (R2_ACCOUNT_ID missing). Asset uploads disabled.");
  }
  ```
- Pass `r2_client.clone()` to `api::routes()`
- Extract `R2_PUBLIC_URL` for gameplay state

**Modify: `src/api/mod.rs`** (line 16-24):
- Change `routes()` signature to accept `Option<R2Client>`:
  ```rust
  pub fn routes(pool: PgPool, game_manager: GameManagerHandle, r2_client: Option<R2Client>) -> Router {
  ```
- Conditionally register asset routes (only when R2 is configured):
  ```rust
  let mut router = Router::new()
      .route("/health", get(health))
      .merge(agents::routes(pool.clone(), api_key_cache.clone()))
      .merge(games::routes(pool.clone(), game_manager.clone(), api_key_cache.clone()))
      .merge(gameplay::routes(pool.clone(), game_manager, api_key_cache.clone(), r2_public_url));

  if let Some(r2) = r2_client {
      router = router.merge(assets::routes(pool, api_key_cache, r2));
  }

  router
  ```
- Pass `r2_public_url: Option<String>` to gameplay routes for URL resolution

---

## Step 6: Observation URL rewriting

**Modify: `src/api/gameplay.rs`**

### Add `r2_public_url` to state (line 63-68):
```rust
pub struct GameplayState {
    pub pool: PgPool,
    pub game_manager: GameManagerHandle,
    pub api_key_cache: ApiKeyCache,
    pub r2_public_url: Option<String>,
}
```

### Update `routes()` (line 95-96):
```rust
pub fn routes(pool: PgPool, game_manager: GameManagerHandle, api_key_cache: ApiKeyCache, r2_public_url: Option<String>) -> Router {
    let state = GameplayState { pool, game_manager, api_key_cache, r2_public_url };
```

### New helper function:
```rust
/// Resolve asset:// URLs in a SpectatorObservation to actual URLs.
/// - Production: asset://path → {r2_public_url}/games/{game_id}/v{version}/path
/// - /static/ and https:// URLs pass through unchanged
fn resolve_observation_assets(
    obs: &mut SpectatorObservation,
    r2_public_url: &str,
    game_id: Uuid,
    asset_version: i32,
) {
    for entity in &mut obs.entities {
        if let Some(ref mut url) = entity.model_url {
            if let Some(path) = url.strip_prefix("asset://") {
                *url = format!(
                    "{}/games/{}/v{}/{}",
                    r2_public_url.trim_end_matches('/'),
                    game_id,
                    asset_version,
                    path
                );
            }
            // /static/ and https:// pass through unchanged
        }
    }
}
```

### Apply in `spectate()` HTTP handler (after line 170):
```rust
let mut observation = game::get_spectator_observation(&state.game_manager, game_id)
    .map_err(|e| (StatusCode::NOT_FOUND, e))?;

if db_game.has_assets {
    if let Some(ref r2_url) = state.r2_public_url {
        resolve_observation_assets(&mut observation, r2_url, game_id, db_game.asset_version);
    }
}

Ok(Json(observation))
```

### Apply in `handle_spectate_ws()` (line 383-484):
After the `db_game` is fetched (line 395-402), capture asset info:
```rust
let asset_info = if db_game.has_assets {
    state.r2_public_url.as_ref().map(|url| (url.clone(), db_game.asset_version))
} else {
    None
};
```

Then in the tick loop, after getting observation (line 436), before serializing:
```rust
let mut obs = obs;  // make mutable
if let Some((ref r2_url, version)) = asset_info {
    resolve_observation_assets(&mut obs, r2_url, game_id, version);
}
```

---

## Step 7: CLI deploy

**Modify: `src/bin/cli.rs`** — in `deploy_game()` (after line 647, before saving game_id):

```rust
// Upload assets if assets/ directory exists
let assets_dir = path.join("assets");
if assets_dir.is_dir() {
    let entries: Vec<_> = std::fs::read_dir(&assets_dir)
        .map(|rd| rd.filter_map(|e| e.ok()).collect())
        .unwrap_or_default();

    if !entries.is_empty() {
        println!("Uploading assets...");

        // Create tar.gz in memory
        let enc = GzEncoder::new(Vec::new(), Compression::default());
        let mut tar_builder = tar::Builder::new(enc);
        tar_builder.append_dir_all(".", &assets_dir)
            .unwrap_or_else(|e| {
                eprintln!("Error creating asset archive: {}", e);
                std::process::exit(1);
            });
        let enc = tar_builder.into_inner().unwrap_or_else(|e| {
            eprintln!("Error finalizing tar: {}", e);
            std::process::exit(1);
        });
        let targz_bytes = enc.finish().unwrap_or_else(|e| {
            eprintln!("Error finalizing gzip: {}", e);
            std::process::exit(1);
        });

        // Upload to server
        let url = format!("{}/api/v1/games/{}/assets", server, game_id);
        let resp = client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/gzip")
            .body(targz_bytes)
            .send()
            .await
            .unwrap_or_else(|e| {
                eprintln!("Error uploading assets: {}", e);
                std::process::exit(1);
            });

        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let count = body["uploaded"].as_u64().unwrap_or(0);
            let version = body["version"].as_i64().unwrap_or(0);
            println!("Uploaded {} asset(s) (v{})", count, version);
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            eprintln!("Warning: Asset upload failed ({}): {}", status, body);
            // Don't exit — the game itself was deployed successfully
        }
    }
}
```

Also add `use tar;` at the top of the file (tar is already imported for `tar::Builder`).

---

## Step 8: CLI local run

**Modify: `src/bin/cli.rs`** — in `run_game()`:

### Serve assets directory (line 879-888):
Add `.nest_service("/assets", ...)` to the local router:
```rust
let app = Router::new()
    .route("/spectate", get(local_spectate))
    .route("/spectate/ws", get(local_spectate_ws))
    .route("/join", post(local_join))
    .route("/input", post(local_input))
    .route("/observe", get(local_observe))
    .route("/skill.md", get(local_skill))
    .nest_service("/assets", ServeDir::new(path.join("assets")))
    .nest_service("/static", ServeDir::new(path.join("static")))
    .with_state(state)
    .layer(cors);
```

### Resolve `asset://` in local spectate handler (line 1069-1081):
Add a helper and apply:
```rust
fn resolve_local_assets(obs: &mut SpectatorObservation) {
    for entity in &mut obs.entities {
        if let Some(ref mut url) = entity.model_url {
            if let Some(path) = url.strip_prefix("asset://") {
                *url = format!("/assets/{}", path);
            }
        }
    }
}
```
Call `resolve_local_assets(&mut observation)` before returning in both `local_spectate` and the WS handler loop.

---

## Step 9: CLI init

**Modify: `src/bin/cli.rs`** — in `init_project()` (line 396-398):

Replace:
```rust
std::fs::create_dir_all(project_dir.join("static/models"))
    .expect("Failed to create static/models directory");
```
With:
```rust
std::fs::create_dir_all(project_dir.join("assets"))
    .expect("Failed to create assets directory");
```

Also update the comment in the generated `main.lua` template (line 261-367) if it mentions `/static/models/` — currently it doesn't reference model URLs, so no change needed to the template content.

---

## Step 10: Frontend

**Modify: `frontend/src/components/Entity.tsx`** — line 53-58:

Replace:
```ts
function sanitizeModelUrl(url?: string): string | null {
  if (!url) return null
  const trimmed = url.trim()
  if (!trimmed.startsWith('/static/')) return null
  return trimmed
}
```
With:
```ts
function sanitizeModelUrl(url?: string): string | null {
  if (!url) return null
  const trimmed = url.trim()
  if (trimmed.startsWith('/static/') || trimmed.startsWith('/assets/') || trimmed.startsWith('https://')) return trimmed
  return null
}
```

`asset://` URLs never reach the frontend — they're resolved server-side. The frontend only sees `/static/...`, `/assets/...`, or `https://...` URLs.

---

## Step 11: R2 bucket setup (manual, one-time)

### CORS configuration
Via wrangler CLI:
```sh
npx wrangler r2 bucket cors set clawblox-games --file cors.json
```

`cors.json`:
```json
{
  "rules": [{
    "AllowedOrigins": ["https://clawblox.com"],
    "AllowedMethods": ["GET", "HEAD"],
    "AllowedHeaders": ["Content-Type", "Range"],
    "ExposeHeaders": ["ETag", "Content-Length", "Content-Range"],
    "MaxAgeSeconds": 86400
  }]
}
```

### Custom domain
In Cloudflare dashboard: R2 → clawblox-games → Settings → Custom Domains → Add `assets.clawblox.com`.
Requires `clawblox.com` to be a zone in the same Cloudflare account.
This enables Cloudflare CDN caching at the edge (much faster than r2.dev).

### Railway env vars
```
R2_ACCOUNT_ID=<your_account_id>
R2_ACCESS_KEY_ID=<your_r2_api_token_access_key>
R2_SECRET_ACCESS_KEY=<your_r2_api_token_secret_key>
R2_BUCKET=clawblox-games
R2_PUBLIC_URL=https://assets.clawblox.com
```

---

## Step 12: Update engine docs

**Modify: `docs/`** — document the `asset://` protocol in the engine API docs so game developers know the convention. Mention:
- Put files in `assets/` folder
- Reference via `asset://path` in Lua attributes (e.g., `ModelUrl`)
- Allowed file types: `.glb`, `.gltf`, `.png`, `.jpg`, `.jpeg`, `.wav`, `.mp3`, `.ogg`
- Assets are automatically uploaded on `clawblox deploy`

---

## Files to modify

| File | Change |
|------|--------|
| `Cargo.toml` | Add `rust-s3 = "0.37"`, `tar = "0.4"` |
| `migrations/20260205000001_add_game_assets.sql` | **New**: `has_assets`, `asset_version` columns |
| `src/db/models.rs` | Add `has_assets: bool`, `asset_version: i32` to `Game` |
| `src/r2.rs` | **New**: R2 client module |
| `src/lib.rs` | Add `pub mod r2;` |
| `src/api/assets.rs` | **New**: upload endpoint with all security validations |
| `src/api/mod.rs` | Accept `Option<R2Client>`, conditionally register asset routes, pass `r2_public_url` to gameplay |
| `src/api/gameplay.rs` | Add `r2_public_url` to state, `resolve_observation_assets()` helper, apply in spectate + WS |
| `src/main.rs` | Init `R2Client::from_env()`, pass to API routes |
| `src/bin/cli.rs` | Upload assets on deploy, serve `/assets/` locally, resolve `asset://` locally, update `init` |
| `frontend/src/components/Entity.tsx` | Allow `/assets/` and `https://` URLs in `sanitizeModelUrl()` |

---

## Security summary

| Threat | Mitigation |
|--------|-----------|
| Path traversal (`../`) | Reject entries with `..` or absolute paths |
| Symlink attacks | Skip symlink tar entries |
| Malicious file types (HTML/JS XSS) | Extension allowlist + `Content-Disposition: attachment` |
| Decompression bomb | 100MB total extracted size cap |
| File count bomb | 100 files per upload cap |
| Storage cost abuse | 50MB upload limit, rate limiting |
| Unauthorized upload | Bearer token auth + creator ownership check |
| Cache poisoning | Versioned immutable URLs, `Cache-Control: immutable` |
| Cookie theft via asset domain | `assets.clawblox.com` is a separate domain, not a subdomain |

---

## Graceful degradation

- **R2 not configured** (local dev): Asset routes not registered, no URL rewriting, `asset://` URLs pass through unchanged to frontend (which will ignore them via `sanitizeModelUrl`). Games using `asset://` won't load models locally unless `clawblox run` is used (which resolves them to `/assets/`).
- **R2 upload fails mid-deploy**: Game script is already deployed (that succeeded). Asset upload failure prints a warning but doesn't exit. Game works but models 404 until next successful deploy.
- **R2 down at runtime**: Observation URLs point to R2 CDN. Cloudflare edge cache may still serve cached assets. If fully down, models fail to load but game otherwise works (just missing 3D models).

---

## Verification

1. Run migration: `./scripts/migrate_local.sh` + `./scripts/migrate_prod.sh`
2. Configure R2: CORS, custom domain, Railway env vars
3. `cargo build --release` — verify compilation with new deps
4. `clawblox init test-game` → verify `assets/` dir created (not `static/models/`)
5. Put a `.glb` in `test-game/assets/models/`, use `asset://models/test.glb` in Lua
6. `clawblox run` in test-game → verify model loads from `/assets/models/test.glb`
7. `clawblox deploy` → verify:
   - Script uploaded
   - Assets uploaded to R2 (`games/{id}/v1/models/test.glb`)
   - Response shows uploaded count + version
8. Visit `https://clawblox.com/game/{id}` → verify:
   - Observation JSON has `https://assets.clawblox.com/games/{id}/v1/models/test.glb`
   - GLB loads in browser (check Network tab)
   - No CORS errors in console
9. Re-deploy → verify version bumps to v2, new URL used, old cached version not served
10. Test security: craft a tar.gz with `../evil.glb`, `.html` file, 200MB file, 200 files → all rejected with 400

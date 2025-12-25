# Configuration Guide

Baram supports two methods for configuration: environment variables and TOML files.

## Environment Variables

The following environment variables are supported:

### Crawler Settings
- `BARAM_MAX_CONCURRENT_REQUESTS` - Maximum concurrent HTTP requests (default: 10)
- `BARAM_RATE_LIMIT` - Rate limit in requests per second (default: 2.0)
- `BARAM_REQUEST_TIMEOUT` - Request timeout in seconds (default: 30)
- `BARAM_USER_AGENT` - Custom user agent string (default: "baram/{version}")

### Database Settings
- `BARAM_SQLITE_PATH` - SQLite database path (default: "data/metadata.db")
- `POSTGRES_URL` - PostgreSQL connection string (preferred)
- `DATABASE_URL` - Alternative PostgreSQL connection string (fallback)

### OpenSearch Settings
- `OPENSEARCH_URL` - OpenSearch endpoint URL (default: "http://localhost:9200")
- `OPENSEARCH_INDEX` - Index name for articles (default: "baram-articles")
- `OPENSEARCH_USERNAME` - Optional authentication username
- `OPENSEARCH_PASSWORD` - Optional authentication password

### Logging Settings
- `BARAM_LOG_LEVEL` - Log level: trace, debug, info, warn, error (default: "info")
- `BARAM_LOG_FORMAT` - Log format: text, json (default: "text")

## TOML Configuration File

Alternatively, you can use a TOML configuration file. See `config.example.toml` for a complete example.

### Usage

```rust
use baram::config::Config;
use std::path::Path;

// Load from environment variables
let config = Config::from_env()?;

// Load from TOML file
let config = Config::from_file(Path::new("config.toml"))?;

// Validate configuration
config.validate()?;
```

### Example TOML File

```toml
[crawler]
max_concurrent_requests = 10
rate_limit = 2.0
request_timeout_secs = 30
user_agent = "baram/0.1.6"
enable_cookies = true

[database]
sqlite_path = "data/metadata.db"
postgres_url = "postgresql://localhost:5432/baram"
pool_size = 10

[opensearch]
url = "http://localhost:9200"
index_name = "baram-articles"

[logging]
level = "info"
format = "text"
```

## Priority

When using environment variables, they take precedence over default values. When loading from a file, all values must be specified in the TOML file.

For database configuration:
1. `POSTGRES_URL` is checked first
2. If not set, `DATABASE_URL` is used as fallback
3. If neither is set, defaults to "postgresql://localhost/baram"

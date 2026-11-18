#[path = "unit/cache.rs"]
mod cache;
#[path = "unit/candidates.rs"]
mod candidates;
#[path = "unit/hash.rs"]
mod hash;
#[allow(dead_code)] // shared with the integration binary; unit tests use only part of it
#[path = "integration/mock_server.rs"]
mod mock_server;
#[path = "unit/policy.rs"]
mod policy;
#[path = "unit/quota.rs"]
mod quota;
#[path = "unit/redaction.rs"]
mod redaction;
#[path = "unit/settings.rs"]
mod settings;
#[path = "unit/strings.rs"]
mod strings;
#[path = "unit/upload.rs"]
mod upload;

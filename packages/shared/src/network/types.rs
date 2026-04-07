//! Network data types for Minecraft server scanning.

#[derive(Debug, Clone)]
pub struct PlayerSample {
    pub name: String,
    pub uuid: String,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub ip: String,
    pub port: u16,
    pub server_type: String,
    pub online: bool,
    pub players_online: i32,
    pub players_max: i32,
    pub motd: Option<String>,
    pub version: Option<String>,
    pub favicon: Option<String>,
    pub brand: Option<String>,
    pub asn: Option<String>,
    pub country: Option<Option<String>>,
    pub players_sample: Option<Vec<PlayerSample>>,
    pub timestamp: chrono::NaiveDateTime,
}

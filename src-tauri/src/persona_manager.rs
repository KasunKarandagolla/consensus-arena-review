use rand::RngExt;
use rand_chacha::ChaChaRng;
use rand_core::SeedableRng;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintProfile {
    pub user_agent: String,
    pub screen_width: u32,
    pub screen_height: u32,
    pub color_depth: u32,
    pub hardware_concurrency: u32,
    pub device_memory: f64,
    pub timezone: String,
    pub language: String,
    pub canvas_noise_seed: u64,
    pub webgl_vendor: String,
    pub webgl_renderer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Persona {
    pub id: String,
    pub service_id: String,
    pub service_url: String,
    pub variant: String,
    pub plan_tier: String,
    pub fingerprint: FingerprintProfile,
    pub behavior: serde_json::Value,
    pub typing_cadence: serde_json::Value,
    pub session_jitter: f64,
    pub canvas_noise_seed: u64,
    pub role: String,
    pub role_weight: f64,
    pub has_veto: bool,
    pub health_score: f64,
}

pub fn generate_fingerprint(persona_id: &str) -> FingerprintProfile {
    let seed: u64 = persona_id
        .bytes()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
    let mut rng = ChaChaRng::seed_from_u64(seed);

    FingerprintProfile {
        user_agent: format!(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{} Safari/537.36",
            rng.random_range(90u32..120)
        ),
        screen_width: rng.random_range(1024u32..3840),
        screen_height: rng.random_range(768u32..2160),
        color_depth: 24,
        hardware_concurrency: rng.random_range(2u32..16),
        device_memory: rng.random_range(2.0f64..16.0),
        timezone: "America/New_York".to_string(),
        language: "en-US".to_string(),
        canvas_noise_seed: rng.random::<u64>(),
        webgl_vendor: "Google Inc.".to_string(),
        webgl_renderer: "ANGLE (Intel, Mesa Intel(R) Graphics (RKL GT1), OpenGL 4.5)".to_string(),
    }
}

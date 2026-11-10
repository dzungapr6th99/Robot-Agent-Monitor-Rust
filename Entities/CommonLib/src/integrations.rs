pub struct MediaClientOptions {
    pub base_url: String,
    pub access_token: String,
    pub timeout_milliseconds: u64,
}

impl Default for MediaClientOptions {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8090/".into(),
            access_token: String::new(),
            timeout_milliseconds: 10000,
        }
    }
}

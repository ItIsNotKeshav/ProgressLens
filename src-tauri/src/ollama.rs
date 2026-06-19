use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
    pub timeout_seconds: u64,
    pub max_tool_iterations: usize,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            endpoint: "http://localhost:11434".into(),
            model: "qwen3:8b".into(),
            timeout_seconds: 90,
            max_tool_iterations: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: &'a [ChatMessage],
    stream: bool,
    format: &'static str,
    think: bool,
    options: Value,
}

#[derive(Deserialize)]
struct ChatResponse {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<ModelInfo>,
}

#[derive(Deserialize)]
struct ModelInfo {
    name: String,
}

pub struct OllamaClient {
    client: reqwest::Client,
    base_url: Url,
    config: OllamaConfig,
}

impl OllamaClient {
    pub fn new(config: OllamaConfig) -> Result<Self, String> {
        validate_local_endpoint(&config.endpoint)?;
        if config.model.trim().is_empty() || config.model.len() > 128 {
            return Err("AI_CONFIG_INVALID: Model name must contain between 1 and 128 characters.".into());
        }
        let base_url = Url::parse(config.endpoint.trim_end_matches('/'))
            .map_err(|_| "AI_CONFIG_INVALID: Ollama endpoint is not a valid URL.".to_string())?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(config.timeout_seconds.clamp(10, 300)))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|error| format!("AI_CLIENT_ERROR: Could not initialize the local AI client: {error}"))?;
        Ok(Self { client, base_url, config })
    }

    fn endpoint(&self, path: &str) -> Result<Url, String> {
        self.base_url.join(path).map_err(|_| "AI_CONFIG_INVALID: Could not build the Ollama API URL.".into())
    }

    pub async fn chat(&self, messages: &[ChatMessage]) -> Result<String, String> {
        let url = self.endpoint("/api/chat")?;
        let response = self.client.post(url).json(&ChatRequest {
            model: &self.config.model,
            messages,
            stream: false,
            format: "json",
            think: false,
            options: json!({
                "temperature":     0.0,   // deterministic JSON — no creativity needed
                "num_predict":     2048,  // enough for long final answers with tool data
                "num_ctx":         8192,  // explicit context window (Qwen3-8B default)
                "repeat_penalty":  1.05,  // mild penalty prevents tool_call loops
            }),
        }).send().await.map_err(map_transport_error)?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            if status.as_u16() == 404 || body.to_lowercase().contains("not found") {
                return Err(format!("AI_MODEL_MISSING: The local model '{}' is not installed. Run `ollama pull {}`.", self.config.model, self.config.model));
            }
            return Err(format!("AI_OLLAMA_ERROR: Ollama returned HTTP {}. {}", status.as_u16(), compact(&body)));
        }
        let parsed: ChatResponse = serde_json::from_str(&body)
            .map_err(|_| "AI_INVALID_RESPONSE: Ollama returned an unreadable response.".to_string())?;
        if parsed.message.content.trim().is_empty() {
            return Err("AI_INVALID_RESPONSE: The model returned an empty response.".into());
        }
        Ok(parsed.message.content)
    }

    pub async fn health(&self) -> Result<(), String> {
        let response = self.client.get(self.endpoint("/api/tags")?).send().await.map_err(map_transport_error)?;
        if !response.status().is_success() {
            return Err(format!("AI_OLLAMA_ERROR: Ollama health check returned HTTP {}.", response.status().as_u16()));
        }
        let tags: TagsResponse = response.json().await
            .map_err(|_| "AI_INVALID_RESPONSE: Ollama returned an unreadable model list.".to_string())?;
        let installed = tags.models.iter().any(|model| model.name == self.config.model || model.name.trim_end_matches(":latest") == self.config.model.trim_end_matches(":latest"));
        if !installed {
            return Err(format!("AI_MODEL_MISSING: Ollama is running, but '{}' is not installed. Run `ollama pull {}`.", self.config.model, self.config.model));
        }
        Ok(())
    }
}

pub fn validate_local_endpoint(endpoint: &str) -> Result<(), String> {
    let url = Url::parse(endpoint.trim()).map_err(|_| "AI_CONFIG_INVALID: Ollama endpoint is not a valid URL.".to_string())?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("AI_CONFIG_INVALID: Ollama endpoint must use HTTP or HTTPS.".into());
    }
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
    if !matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return Err("AI_PRIVACY_BLOCK: Only localhost Ollama endpoints are allowed, so student data cannot leave this device.".into());
    }
    if url.username() != "" || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
        return Err("AI_CONFIG_INVALID: Ollama endpoint must not contain credentials, query parameters, or fragments.".into());
    }
    Ok(())
}

fn map_transport_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "AI_TIMEOUT: The local model took too long to respond. Try again or increase the timeout in AI settings.".into()
    } else if error.is_connect() {
        "AI_OLLAMA_UNAVAILABLE: Ollama is not reachable. Start Ollama and verify the configured local endpoint.".into()
    } else {
        format!("AI_OLLAMA_UNAVAILABLE: Could not communicate with Ollama: {error}")
    }
}

fn compact(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(240).collect()
}

#[cfg(test)]
mod tests {
    use super::validate_local_endpoint;

    #[test]
    fn accepts_loopback_endpoints() {
        assert!(validate_local_endpoint("http://localhost:11434").is_ok());
        assert!(validate_local_endpoint("http://127.0.0.1:11434").is_ok());
        assert!(validate_local_endpoint("http://[::1]:11434").is_ok());
    }

    #[test]
    fn rejects_remote_endpoints() {
        assert!(validate_local_endpoint("https://example.com").is_err());
        assert!(validate_local_endpoint("http://192.168.1.5:11434").is_err());
    }
}

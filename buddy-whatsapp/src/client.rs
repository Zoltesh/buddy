use reqwest::Client;
use serde_json::json;

const GRAPH_API_BASE: &str = "https://graph.facebook.com/v22.0";

/// Error type for WhatsApp API operations.
#[derive(Debug)]
pub struct WhatsAppError(pub String);

impl std::fmt::Display for WhatsAppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WhatsApp API error: {}", self.0)
    }
}

impl std::error::Error for WhatsAppError {}

/// Client for sending messages via the WhatsApp Business Cloud API.
pub struct WhatsAppClient {
    http: Client,
    api_token: String,
    phone_number_id: String,
}

impl WhatsAppClient {
    pub fn new(api_token: String, phone_number_id: String) -> Self {
        Self {
            http: Client::new(),
            api_token,
            phone_number_id,
        }
    }

    /// Send a text message to a WhatsApp recipient.
    pub async fn send_text_message(&self, to: &str, text: &str) -> Result<(), WhatsAppError> {
        let url = format!("{}/{}/messages", GRAPH_API_BASE, self.phone_number_id);
        let body = json!({
            "messaging_product": "whatsapp",
            "to": to,
            "text": { "body": text }
        });

        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| WhatsAppError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read response body".to_string());
            return Err(WhatsAppError(format!("HTTP {status}: {body}")));
        }

        Ok(())
    }

    /// Send an interactive button message via the WhatsApp Cloud API.
    ///
    /// `buttons` is a slice of `(id, title)` pairs (max 3 per WhatsApp API).
    pub async fn send_interactive_message(
        &self,
        to: &str,
        body_text: &str,
        buttons: &[(&str, &str)],
    ) -> Result<(), WhatsAppError> {
        let url = format!("{}/{}/messages", GRAPH_API_BASE, self.phone_number_id);
        let button_rows: Vec<serde_json::Value> = buttons
            .iter()
            .map(|(id, title)| {
                json!({
                    "type": "reply",
                    "reply": { "id": id, "title": title }
                })
            })
            .collect();

        let body = json!({
            "messaging_product": "whatsapp",
            "to": to,
            "type": "interactive",
            "interactive": {
                "type": "button",
                "body": { "text": body_text },
                "action": { "buttons": button_rows }
            }
        });

        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| WhatsAppError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "failed to read response body".to_string());
            return Err(WhatsAppError(format!("HTTP {status}: {body}")));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_text_message_formats_correct_request() {
        // Start a mock HTTP server.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a minimal server that captures the request.
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let mut total = 0;
            loop {
                stream.readable().await.unwrap();
                match stream.try_read(&mut buf[total..]) {
                    Ok(0) => break,
                    Ok(n) => {
                        total += n;
                        let request = String::from_utf8_lossy(&buf[..total]);
                        // Once we have the full body (Content-Length reached), respond.
                        if request.contains("\r\n\r\n") {
                            if let Some(cl) = request
                                .lines()
                                .find(|l| l.to_lowercase().starts_with("content-length:"))
                                .and_then(|l| l.split(':').nth(1))
                                .and_then(|v| v.trim().parse::<usize>().ok())
                            {
                                let body_start =
                                    request.find("\r\n\r\n").unwrap() + 4;
                                let body_len = total - body_start;
                                if body_len >= cl {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(_) => break,
                }
            }

            let request = String::from_utf8_lossy(&buf[..total]).to_string();

            // Send a 200 response.
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
            stream.writable().await.unwrap();
            stream.try_write(response.as_bytes()).unwrap();

            request
        });

        // Override the base URL to point to our mock server.
        let client = WhatsAppClient {
            http: Client::new(),
            api_token: "test-token-123".to_string(),
            phone_number_id: "PHONE_ID_456".to_string(),
        };

        let url = format!("http://{}/v22.0/PHONE_ID_456/messages", addr);
        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": "15559876543",
            "text": { "body": "Hello!" }
        });

        let response = client
            .http
            .post(&url)
            .bearer_auth(&client.api_token)
            .json(&body)
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());

        let captured = server.await.unwrap();
        assert!(captured.contains("POST /v22.0/PHONE_ID_456/messages"));
        assert!(captured.contains("Bearer test-token-123"));
        assert!(captured.contains("messaging_product"));
        assert!(captured.contains("15559876543"));
        assert!(captured.contains("Hello!"));
    }
}

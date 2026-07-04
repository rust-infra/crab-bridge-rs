use serde_json::Value;

pub struct ResponsesSseParser {
    buffer: String,
}

impl Default for ResponsesSseParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponsesSseParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    pub fn push_chunk(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buffer.push_str(&String::from_utf8_lossy(chunk));

        let mut contents = Vec::new();
        while let Some(newline) = self.buffer.find('\n') {
            let line = self.buffer[..newline].trim().to_string();
            self.buffer.drain(..=newline);

            if let Some(content) = extract_output_text_delta(&line) {
                contents.push(content);
            }
        }

        contents
    }
}

fn extract_output_text_delta(line: &str) -> Option<String> {
    let data = line.strip_prefix("data:")?.trim();
    if data == "[DONE]" {
        return None;
    }

    let value: Value = serde_json::from_str(data).ok()?;
    if value.get("type")?.as_str()? != "response.output_text.delta" {
        return None;
    }

    value.get("delta")?.as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_output_text_delta_across_chunks() {
        let mut parser = ResponsesSseParser::new();
        let first = parser.push_chunk(
            b"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hel",
        );
        assert!(first.is_empty());

        let second = parser.push_chunk(b"lo\"}\n\n");
        assert_eq!(second, vec!["hello".to_string()]);
    }
}

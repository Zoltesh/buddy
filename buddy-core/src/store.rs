use std::path::Path;
use std::sync::Mutex;

use crate::types::{Message, MessageContent, Role};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

/// A conversation with all its messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<Message>,
}

/// A lightweight summary for listing conversations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: i64,
}

/// SQLite-backed conversation store.
///
/// Wraps a `Connection` in a `Mutex` so it is `Send + Sync`.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Open (or create) the database at `path` and run migrations.
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path)
            .map_err(|e| format!("failed to open database '{}': {e}", path.display()))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Open an in-memory database (for testing).
    ///
    /// This is a test helper and should not be used in production code.
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory()
            .map_err(|e| format!("failed to open in-memory database: {e}"))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Run schema migrations idempotently.
    fn migrate(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                content_type TEXT NOT NULL,
                content_json TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                sort_order INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_messages_conversation
                ON messages(conversation_id, sort_order);
            ",
        )
        .map_err(|e| format!("migration failed: {e}"))?;
        Ok(())
    }

    /// Create a new conversation with the given title.
    pub fn create_conversation(&self, title: &str) -> Result<Conversation, String> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO conversations (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, title, now_str, now_str],
        )
        .map_err(|e| format!("failed to create conversation: {e}"))?;

        Ok(Conversation {
            id,
            title: title.to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        })
    }

    /// List all conversations ordered by `updated_at` descending.
    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT c.id, c.title, c.created_at, c.updated_at,
                        (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) as msg_count
                 FROM conversations c
                 ORDER BY c.updated_at DESC",
            )
            .map_err(|e| format!("failed to prepare list query: {e}"))?;

        let rows = stmt
            .query_map([], |row| {
                let created_str: String = row.get(2)?;
                let updated_str: String = row.get(3)?;
                Ok(ConversationSummary {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: parse_datetime(&created_str),
                    updated_at: parse_datetime(&updated_str),
                    message_count: row.get(4)?,
                })
            })
            .map_err(|e| format!("failed to list conversations: {e}"))?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row.map_err(|e| format!("failed to read conversation row: {e}"))?);
        }
        Ok(result)
    }

    /// Get a conversation with all its messages, or `None` if not found.
    pub fn get_conversation(&self, id: &str) -> Result<Option<Conversation>, String> {
        let conn = self.conn.lock().unwrap();

        // Fetch conversation metadata.
        let mut stmt = conn
            .prepare("SELECT id, title, created_at, updated_at FROM conversations WHERE id = ?1")
            .map_err(|e| format!("failed to prepare get query: {e}"))?;

        let conv = stmt
            .query_row(params![id], |row| {
                let created_str: String = row.get(2)?;
                let updated_str: String = row.get(3)?;
                Ok(Conversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: parse_datetime(&created_str),
                    updated_at: parse_datetime(&updated_str),
                    messages: Vec::new(),
                })
            })
            .optional()
            .map_err(|e| format!("failed to get conversation: {e}"))?;

        let Some(mut conv) = conv else {
            return Ok(None);
        };

        // Fetch messages.
        let mut msg_stmt = conn
            .prepare(
                "SELECT role, content_type, content_json, timestamp
                 FROM messages
                 WHERE conversation_id = ?1
                 ORDER BY sort_order ASC",
            )
            .map_err(|e| format!("failed to prepare messages query: {e}"))?;

        let msg_rows = msg_stmt
            .query_map(params![id], |row| {
                let role_str: String = row.get(0)?;
                let _content_type: String = row.get(1)?;
                let content_json: String = row.get(2)?;
                let ts_str: String = row.get(3)?;
                Ok((role_str, content_json, ts_str))
            })
            .map_err(|e| format!("failed to query messages: {e}"))?;

        for row in msg_rows {
            let (role_str, content_json, ts_str) =
                row.map_err(|e| format!("failed to read message row: {e}"))?;
            let role: Role = serde_json::from_str(&format!("\"{role_str}\""))
                .map_err(|e| format!("invalid role '{role_str}': {e}"))?;
            let content: MessageContent = serde_json::from_str(&content_json)
                .map_err(|e| format!("invalid message content: {e}"))?;
            conv.messages.push(Message {
                role,
                content,
                timestamp: parse_datetime(&ts_str),
            });
        }

        Ok(Some(conv))
    }

    /// Delete a conversation and all its messages (via ON DELETE CASCADE).
    /// Returns `true` if a conversation was deleted, `false` if it didn't exist.
    pub fn delete_conversation(&self, id: &str) -> Result<bool, String> {
        let conn = self.conn.lock().unwrap();
        // Ensure foreign keys are enforced for CASCADE.
        conn.execute("PRAGMA foreign_keys = ON", [])
            .map_err(|e| format!("failed to enable foreign keys: {e}"))?;
        let rows = conn
            .execute("DELETE FROM conversations WHERE id = ?1", params![id])
            .map_err(|e| format!("failed to delete conversation: {e}"))?;
        Ok(rows > 0)
    }

    /// Append a single message to a conversation.
    pub fn append_message(&self, conversation_id: &str, message: &Message) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();

        // Determine the next sort_order.
        let sort_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM messages WHERE conversation_id = ?1",
                params![conversation_id],
                |row| row.get(0),
            )
            .map_err(|e| format!("failed to get next sort_order: {e}"))?;

        let msg_id = uuid::Uuid::new_v4().to_string();
        let role_str = serialize_role(&message.role);
        let (content_type, content_json) = serialize_content(&message.content);
        let ts_str = message.timestamp.to_rfc3339();

        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content_type, content_json, timestamp, sort_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![msg_id, conversation_id, role_str, content_type, content_json, ts_str, sort_order],
        )
        .map_err(|e| format!("failed to insert message: {e}"))?;

        // Update the conversation's updated_at.
        let now_str = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
            params![now_str, conversation_id],
        )
        .map_err(|e| format!("failed to update conversation timestamp: {e}"))?;

        Ok(())
    }

    /// Update a conversation's title.
    pub fn update_conversation_title(&self, id: &str, title: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let now_str = Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE conversations SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now_str, id],
        )
        .map_err(|e| format!("failed to update conversation title: {e}"))?;
        Ok(())
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Parse an RFC 3339 datetime string, falling back to epoch on failure.
fn parse_datetime(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_default()
}

/// Serialize a `Role` to its lowercase string representation.
fn serialize_role(role: &Role) -> String {
    match role {
        Role::User => "user".to_string(),
        Role::Assistant => "assistant".to_string(),
        Role::System => "system".to_string(),
    }
}

/// Serialize `MessageContent` to a (content_type, content_json) pair.
fn serialize_content(content: &MessageContent) -> (String, String) {
    let content_type = match content {
        MessageContent::Text { .. } => "text",
        MessageContent::ToolCall { .. } => "tool_call",
        MessageContent::ToolResult { .. } => "tool_result",
    };
    let json = serde_json::to_string(content).expect("MessageContent should always serialize");
    (content_type.to_string(), json)
}

/// Generate a conversation title from the first user message.
/// Truncates to ~80 chars at a word boundary. Uses `char_indices` to avoid
/// panicking on multi-byte UTF-8 characters.
pub fn title_from_message(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= 80 {
        return trimmed.to_string();
    }

    // Find the last char boundary at or before byte position 80.
    let byte_limit = trimmed
        .char_indices()
        .take_while(|&(i, _)| i <= 80)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(0);

    let truncated = &trimmed[..byte_limit];
    if let Some(pos) = truncated.rfind(' ') {
        trimmed[..pos].to_string()
    } else {
        truncated.to_string()
    }
}

/// Helper to add `optional()` support to `query_row`.
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::path::PathBuf;

    fn temp_db_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("buddy-test-{name}-{}.db", uuid::Uuid::new_v4()))
    }

    // ── Test: Create, append, drop, reopen, verify messages survive ──────

    #[test]
    fn messages_survive_store_reopen() {
        let path = temp_db_path("reopen");

        {
            let store = Store::open(&path).unwrap();
            let conv = store.create_conversation("Test conversation").unwrap();

            let messages = vec![
                Message {
                    role: Role::User,
                    content: MessageContent::Text { text: "Hello".into() },
                    timestamp: Utc::now(),
                },
                Message {
                    role: Role::Assistant,
                    content: MessageContent::Text { text: "Hi there!".into() },
                    timestamp: Utc::now(),
                },
                Message {
                    role: Role::User,
                    content: MessageContent::Text { text: "How are you?".into() },
                    timestamp: Utc::now(),
                },
            ];

            for msg in &messages {
                store.append_message(&conv.id, msg).unwrap();
            }
        }
        // Store is dropped — simulating restart.

        {
            let store = Store::open(&path).unwrap();
            let convs = store.list_conversations().unwrap();
            assert_eq!(convs.len(), 1);

            let conv = store.get_conversation(&convs[0].id).unwrap().unwrap();
            assert_eq!(conv.messages.len(), 3);
            assert_eq!(
                conv.messages[0].content,
                MessageContent::Text { text: "Hello".into() }
            );
            assert_eq!(
                conv.messages[1].content,
                MessageContent::Text { text: "Hi there!".into() }
            );
            assert_eq!(
                conv.messages[2].content,
                MessageContent::Text { text: "How are you?".into() }
            );
        }

        std::fs::remove_file(&path).ok();
    }

    // ── Test: List conversations ordered by most recently updated ─────────

    #[test]
    fn list_conversations_ordered_by_updated_at() {
        let store = Store::open_in_memory().unwrap();

        let c1 = store.create_conversation("First").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let c2 = store.create_conversation("Second").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let c3 = store.create_conversation("Third").unwrap();

        let list = store.list_conversations().unwrap();
        assert_eq!(list.len(), 3);
        // Most recently created should be first.
        assert_eq!(list[0].id, c3.id);
        assert_eq!(list[1].id, c2.id);
        assert_eq!(list[2].id, c1.id);

        // Now update the first conversation — it should move to the top.
        std::thread::sleep(std::time::Duration::from_millis(10));
        store
            .append_message(
                &c1.id,
                &Message {
                    role: Role::User,
                    content: MessageContent::Text { text: "bump".into() },
                    timestamp: Utc::now(),
                },
            )
            .unwrap();

        let list = store.list_conversations().unwrap();
        assert_eq!(list[0].id, c1.id);
    }

    // ── Test: Delete conversation removes it and its messages ────────────

    #[test]
    fn delete_conversation_removes_everything() {
        let store = Store::open_in_memory().unwrap();

        let conv = store.create_conversation("To delete").unwrap();
        store
            .append_message(
                &conv.id,
                &Message {
                    role: Role::User,
                    content: MessageContent::Text { text: "msg".into() },
                    timestamp: Utc::now(),
                },
            )
            .unwrap();

        let deleted = store.delete_conversation(&conv.id).unwrap();
        assert!(deleted);
        assert!(store.get_conversation(&conv.id).unwrap().is_none());
    }

    // ── Test: ToolCall and ToolResult round-trip ─────────────────────────

    #[test]
    fn tool_call_and_result_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let conv = store.create_conversation("Tool test").unwrap();

        let tool_call = Message {
            role: Role::Assistant,
            content: MessageContent::ToolCall {
                id: "call_abc".into(),
                name: "read_file".into(),
                arguments: r#"{"path":"/tmp/test.txt"}"#.into(),
            },
            timestamp: Utc::now(),
        };

        let tool_result = Message {
            role: Role::User,
            content: MessageContent::ToolResult {
                id: "call_abc".into(),
                content: "file contents here".into(),
            },
            timestamp: Utc::now(),
        };

        store.append_message(&conv.id, &tool_call).unwrap();
        store.append_message(&conv.id, &tool_result).unwrap();

        let loaded = store.get_conversation(&conv.id).unwrap().unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, tool_call.content);
        assert_eq!(loaded.messages[1].content, tool_result.content);
        assert_eq!(loaded.messages[0].role, Role::Assistant);
        assert_eq!(loaded.messages[1].role, Role::User);
    }

    // ── Test: Title truncation at word boundary ─────────────────────────

    #[test]
    fn title_truncation_at_word_boundary() {
        let long_msg = "Tell me about the history of computing in the modern era and how it has shaped our daily lives forever";
        let title = title_from_message(long_msg);
        assert!(title.len() <= 80, "title should be <= 80 chars, got {}", title.len());
        assert!(!title.ends_with(' '), "title should not end with a space");
        // Should truncate at a word boundary.
        assert!(
            long_msg.starts_with(&title),
            "title should be a prefix of the original"
        );

        // Short message stays unchanged.
        let short = "Hello world";
        assert_eq!(title_from_message(short), "Hello world");
    }

    #[test]
    fn title_truncation_handles_multibyte_utf8() {
        // Each CJK character is 3 bytes. 30 characters = 90 bytes > 80 byte limit.
        let cjk = "日本語のテキストを使って長い文章を作成してテストを行います";
        // Should not panic, and result should be valid UTF-8.
        let title = title_from_message(cjk);
        assert!(title.len() <= 83, "title bytes should be near 80, got {}", title.len());
        assert!(cjk.starts_with(&title));

        // Mixed ASCII + emoji.
        let mixed = "Hello 🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍🌍";
        let title = title_from_message(mixed);
        assert!(title.len() <= 84, "title bytes should be near 80, got {}", title.len());
    }

    // ── Test: Idempotent migrations (open DB twice) ─────────────────────

    #[test]
    fn idempotent_migrations() {
        let path = temp_db_path("idempotent");

        {
            let _store = Store::open(&path).unwrap();
        }
        // Open again — migrations should not fail.
        {
            let store = Store::open(&path).unwrap();
            // Should still work fine.
            let list = store.list_conversations().unwrap();
            assert!(list.is_empty());
        }

        std::fs::remove_file(&path).ok();
    }

    // ── Test: create_conversation returns valid UUID ─────────────────────

    #[test]
    fn create_conversation_returns_valid_uuid() {
        let store = Store::open_in_memory().unwrap();
        let conv = store.create_conversation("UUID test").unwrap();
        // Should be parseable as a UUID.
        uuid::Uuid::parse_str(&conv.id).expect("id should be a valid UUID");
    }

    // ── Test: message_count in list_conversations ────────────────────────

    #[test]
    fn list_conversations_includes_message_count() {
        let store = Store::open_in_memory().unwrap();
        let conv = store.create_conversation("Count test").unwrap();

        for i in 0..5 {
            store
                .append_message(
                    &conv.id,
                    &Message {
                        role: Role::User,
                        content: MessageContent::Text {
                            text: format!("msg {i}"),
                        },
                        timestamp: Utc::now(),
                    },
                )
                .unwrap();
        }

        let list = store.list_conversations().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].message_count, 5);
    }

    // ── Test: update_conversation_title ──────────────────────────────────

    #[test]
    fn update_conversation_title_works() {
        let store = Store::open_in_memory().unwrap();
        let conv = store.create_conversation("Original").unwrap();

        store.update_conversation_title(&conv.id, "Updated").unwrap();

        let loaded = store.get_conversation(&conv.id).unwrap().unwrap();
        assert_eq!(loaded.title, "Updated");
    }
}

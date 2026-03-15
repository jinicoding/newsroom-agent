//! Project memory system for yoyo.
//!
//! Persists project-specific notes across sessions in `.yoyo/memory.json`.
//! Each memory is a `{note, timestamp}` pair stored as a JSON array.
//! Users can add memories with `/remember`, list with `/memories`, remove with `/forget`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A single project memory entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEntry {
    pub note: String,
    pub timestamp: String,
}

/// The in-memory store of project memories.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProjectMemory {
    pub entries: Vec<MemoryEntry>,
}

/// The directory name for yoyo project data.
const YOYO_DIR: &str = ".yoyo";

/// The filename for the memory store within `.yoyo/`.
const MEMORY_FILE: &str = "memory.json";

/// Get the path to the memory file for the current project.
pub fn memory_file_path() -> PathBuf {
    Path::new(YOYO_DIR).join(MEMORY_FILE)
}

/// Load project memories from `.yoyo/memory.json`.
/// Returns an empty `ProjectMemory` if the file doesn't exist or can't be parsed.
pub fn load_memories() -> ProjectMemory {
    load_memories_from(&memory_file_path())
}

/// Load project memories from a specific path (for testing).
pub fn load_memories_from(path: &Path) -> ProjectMemory {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => ProjectMemory::default(),
    }
}

/// Save project memories to `.yoyo/memory.json`.
/// Creates the `.yoyo/` directory if it doesn't exist.
pub fn save_memories(memory: &ProjectMemory) -> Result<(), String> {
    save_memories_to(memory, &memory_file_path())
}

/// Save project memories to a specific path (for testing).
pub fn save_memories_to(memory: &ProjectMemory, path: &Path) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }
    let json =
        serde_json::to_string_pretty(memory).map_err(|e| format!("Serialization error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

/// Add a new memory entry with the current timestamp.
pub fn add_memory(memory: &mut ProjectMemory, note: &str) {
    let timestamp = current_timestamp();
    memory.entries.push(MemoryEntry {
        note: note.to_string(),
        timestamp,
    });
}

/// Remove a memory entry by index (0-based).
/// Returns the removed entry, or None if the index is out of bounds.
pub fn remove_memory(memory: &mut ProjectMemory, index: usize) -> Option<MemoryEntry> {
    if index < memory.entries.len() {
        Some(memory.entries.remove(index))
    } else {
        None
    }
}

/// Format memories for display in the system prompt.
/// Returns None if there are no memories.
pub fn format_memories_for_prompt(memory: &ProjectMemory) -> Option<String> {
    if memory.entries.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    lines.push("## Project Memories".to_string());
    lines.push(String::new());
    for entry in &memory.entries {
        lines.push(format!("- {} ({})", entry.note, entry.timestamp));
    }
    Some(lines.join("\n"))
}

/// Get the current timestamp in a human-readable format.
fn current_timestamp() -> String {
    // Use a simple approach: shell out to date command for portability
    std::process::Command::new("date")
        .arg("+%Y-%m-%d %H:%M")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                String::from_utf8(o.stdout)
                    .ok()
                    .map(|s| s.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_memory_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("yoyo_test_memory_{}", name));
        let _ = fs::create_dir_all(&dir);
        dir.join(MEMORY_FILE)
    }

    fn cleanup(path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn test_memory_entry_serialize_deserialize() {
        let entry = MemoryEntry {
            note: "uses sqlx for database access".to_string(),
            timestamp: "2026-03-15 08:32".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, entry);
    }

    #[test]
    fn test_project_memory_serialize_deserialize() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "tests require docker running".to_string(),
                    timestamp: "2026-03-15 08:00".to_string(),
                },
                MemoryEntry {
                    note: "use pnpm not npm".to_string(),
                    timestamp: "2026-03-15 09:00".to_string(),
                },
            ],
        };
        let json = serde_json::to_string_pretty(&memory).unwrap();
        let parsed: ProjectMemory = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].note, "tests require docker running");
        assert_eq!(parsed.entries[1].note, "use pnpm not npm");
    }

    #[test]
    fn test_add_memory() {
        let mut memory = ProjectMemory::default();
        assert!(memory.entries.is_empty());

        add_memory(&mut memory, "this project uses sqlx");
        assert_eq!(memory.entries.len(), 1);
        assert_eq!(memory.entries[0].note, "this project uses sqlx");
        assert!(!memory.entries[0].timestamp.is_empty());

        add_memory(&mut memory, "tests need docker");
        assert_eq!(memory.entries.len(), 2);
        assert_eq!(memory.entries[1].note, "tests need docker");
    }

    #[test]
    fn test_remove_memory_valid_index() {
        let mut memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "note 0".to_string(),
                    timestamp: "t0".to_string(),
                },
                MemoryEntry {
                    note: "note 1".to_string(),
                    timestamp: "t1".to_string(),
                },
                MemoryEntry {
                    note: "note 2".to_string(),
                    timestamp: "t2".to_string(),
                },
            ],
        };

        let removed = remove_memory(&mut memory, 1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().note, "note 1");
        assert_eq!(memory.entries.len(), 2);
        assert_eq!(memory.entries[0].note, "note 0");
        assert_eq!(memory.entries[1].note, "note 2");
    }

    #[test]
    fn test_remove_memory_invalid_index() {
        let mut memory = ProjectMemory {
            entries: vec![MemoryEntry {
                note: "only one".to_string(),
                timestamp: "t0".to_string(),
            }],
        };

        let removed = remove_memory(&mut memory, 5);
        assert!(removed.is_none());
        assert_eq!(memory.entries.len(), 1);
    }

    #[test]
    fn test_remove_memory_empty() {
        let mut memory = ProjectMemory::default();
        let removed = remove_memory(&mut memory, 0);
        assert!(removed.is_none());
    }

    #[test]
    fn test_save_and_load_memories() {
        let path = temp_memory_path("save_load");
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "first note".to_string(),
                    timestamp: "2026-03-15 08:00".to_string(),
                },
                MemoryEntry {
                    note: "second note".to_string(),
                    timestamp: "2026-03-15 09:00".to_string(),
                },
            ],
        };

        let result = save_memories_to(&memory, &path);
        assert!(result.is_ok(), "Save should succeed: {:?}", result);

        let loaded = load_memories_from(&path);
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].note, "first note");
        assert_eq!(loaded.entries[1].note, "second note");

        cleanup(&path);
    }

    #[test]
    fn test_load_memories_nonexistent_file() {
        let path = Path::new("/tmp/yoyo_test_nonexistent_12345/memory.json");
        let memory = load_memories_from(path);
        assert!(memory.entries.is_empty());
    }

    #[test]
    fn test_load_memories_invalid_json() {
        let path = temp_memory_path("invalid_json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not valid json at all {{{").unwrap();

        let memory = load_memories_from(&path);
        assert!(
            memory.entries.is_empty(),
            "Invalid JSON should return empty memory"
        );

        cleanup(&path);
    }

    #[test]
    fn test_save_creates_directory() {
        let dir = std::env::temp_dir().join("yoyo_test_memory_create_dir");
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("subdir").join(MEMORY_FILE);

        let memory = ProjectMemory {
            entries: vec![MemoryEntry {
                note: "test".to_string(),
                timestamp: "now".to_string(),
            }],
        };

        let result = save_memories_to(&memory, &path);
        assert!(
            result.is_ok(),
            "Save should create parent dirs: {:?}",
            result
        );
        assert!(path.exists(), "File should exist after save");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_format_memories_for_prompt_empty() {
        let memory = ProjectMemory::default();
        assert!(format_memories_for_prompt(&memory).is_none());
    }

    #[test]
    fn test_format_memories_for_prompt_with_entries() {
        let memory = ProjectMemory {
            entries: vec![
                MemoryEntry {
                    note: "uses sqlx".to_string(),
                    timestamp: "2026-03-15 08:00".to_string(),
                },
                MemoryEntry {
                    note: "docker needed for tests".to_string(),
                    timestamp: "2026-03-15 09:00".to_string(),
                },
            ],
        };

        let prompt = format_memories_for_prompt(&memory).unwrap();
        assert!(prompt.contains("## Project Memories"));
        assert!(prompt.contains("uses sqlx"));
        assert!(prompt.contains("docker needed for tests"));
        assert!(prompt.contains("2026-03-15 08:00"));
    }

    #[test]
    fn test_memory_file_path() {
        let path = memory_file_path();
        assert!(path.to_string_lossy().contains(".yoyo"));
        assert!(path.to_string_lossy().contains("memory.json"));
    }

    #[test]
    fn test_full_crud_workflow() {
        let path = temp_memory_path("crud_workflow");

        // Start fresh
        let mut memory = load_memories_from(&path);
        assert!(memory.entries.is_empty());

        // Add entries
        add_memory(&mut memory, "first");
        add_memory(&mut memory, "second");
        add_memory(&mut memory, "third");
        assert_eq!(memory.entries.len(), 3);

        // Save
        save_memories_to(&memory, &path).unwrap();

        // Reload
        let mut loaded = load_memories_from(&path);
        assert_eq!(loaded.entries.len(), 3);
        assert_eq!(loaded.entries[0].note, "first");

        // Remove middle entry
        let removed = remove_memory(&mut loaded, 1);
        assert_eq!(removed.unwrap().note, "second");
        assert_eq!(loaded.entries.len(), 2);

        // Save and reload again
        save_memories_to(&loaded, &path).unwrap();
        let final_load = load_memories_from(&path);
        assert_eq!(final_load.entries.len(), 2);
        assert_eq!(final_load.entries[0].note, "first");
        assert_eq!(final_load.entries[1].note, "third");

        cleanup(&path);
    }
}

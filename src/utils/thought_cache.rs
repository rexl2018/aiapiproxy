//! Thought signature cache
//!
//! Caches thought_signatures from Gemini responses for use in subsequent requests.
//! This is needed because Claude Code doesn't preserve custom fields like thought_signature.

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;
use tracing::debug;

// Global cache for thought_signatures
// Maps tool_call_id -> thought_signature
static THOUGHT_SIGNATURE_CACHE: Lazy<RwLock<HashMap<String, String>>> = 
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Store a thought_signature for a tool call ID
pub fn cache_thought_signature(tool_call_id: &str, signature: &str) {
    if let Ok(mut cache) = THOUGHT_SIGNATURE_CACHE.write() {
        debug!("📝 Caching thought_signature for tool_call_id: {}", tool_call_id);
        cache.insert(tool_call_id.to_string(), signature.to_string());
        // Simple cleanup: if cache gets too large, clear old entries
        if cache.len() > 1000 {
            cache.clear();
        }
    }
}

/// Get a cached thought_signature for a tool call ID
pub fn get_cached_thought_signature(tool_call_id: &str) -> Option<String> {
    if let Ok(cache) = THOUGHT_SIGNATURE_CACHE.read() {
        let result = cache.get(tool_call_id).cloned();
        if result.is_some() {
            debug!("📖 Found cached thought_signature for tool_call_id: {}", tool_call_id);
        }
        result
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_and_retrieve() {
        let id = "test_tool_call_123";
        let sig = "test_signature_abc";
        
        cache_thought_signature(id, sig);
        
        let result = get_cached_thought_signature(id);
        assert_eq!(result, Some(sig.to_string()));
    }

    #[test]
    fn test_missing_entry() {
        let result = get_cached_thought_signature("non_existent_id");
        assert_eq!(result, None);
    }
}

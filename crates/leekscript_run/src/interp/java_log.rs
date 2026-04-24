//! Java-compatible identifiers for farmer / AI log metadata.

/// Same formula as Java [`String.hashCode`](https://docs.oracle.com/en/java/javase/21/docs/api/java.base/java/lang/String.html#hashCode()).
pub fn java_string_hash_code(s: &str) -> i32 {
    s.chars()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// `path.hashCode() & 0xfffffff` as used when constructing [`AIFile`](https://github.com/leek-wars/leek-wars-generator) in the JVM resolver.
pub fn java_path_file_id(normalized_path: &str) -> i32 {
    java_string_hash_code(normalized_path) & 0x0fffffff
}

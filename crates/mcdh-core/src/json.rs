use std::path::Path;

use jsonc_parser::{ParseOptions, parse_to_serde_value};
use serde::de::DeserializeOwned;

use crate::{CoreError, Result};

pub(crate) fn parse_jsonc<T: DeserializeOwned>(text: &str, path: impl AsRef<Path>) -> Result<T> {
    parse_to_serde_value(text.trim_start_matches('\u{feff}'), &parse_options())
        .map_err(|error| CoreError::json(path, error))
}

pub(crate) fn parse_options() -> ParseOptions {
    ParseOptions {
        allow_comments: true,
        allow_loose_object_property_names: false,
        allow_trailing_commas: true,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::parse_jsonc;

    #[test]
    fn accepts_jsonc_comments_and_trailing_commas() {
        let document: Value = parse_jsonc(
            r#"{
                // line comment
                "name": "JSONC",
                "values": [1, 2,],
                /* block comment */
            }"#,
            "jsonc-test",
        )
        .unwrap();
        assert_eq!(document["name"], "JSONC");
        assert_eq!(document["values"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn rejects_non_jsonc_json5_extensions() {
        for invalid in [
            "{ loose: true }",
            "{ 'single': true }",
            "{ \"a\": 1 \"b\": 2 }",
        ] {
            assert!(parse_jsonc::<Value>(invalid, "jsonc-test").is_err());
        }
    }
}

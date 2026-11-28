use std::path::PathBuf;

use serde::Deserialize;

const FILE: &str = "file://";
const HEX: u32 = 16;

/// A SARIF 2.1.0 document, the fields e-law reads of ty's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Sarif {
    /// The runs; ty writes one.
    pub runs: Vec<SarifRun>,
}

/// One run of a SARIF document.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SarifRun {
    /// Its results.
    pub results: Vec<SarifResult>,
}

/// One result: a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifResult {
    /// The tool's rule.
    pub rule_id: String,
    /// What it says.
    pub message: SarifMessage,
    /// Where; ty writes one.
    #[serde(default)]
    pub locations: Vec<SarifLocation>,
}

/// A result's message.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SarifMessage {
    /// Its text.
    pub text: String,
}

/// A result's location.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifLocation {
    /// The file and the region.
    pub physical_location: SarifPhysical,
}

/// A file and a region of it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifPhysical {
    /// The file.
    pub artifact_location: SarifArtifact,
    /// The region.
    pub region: SarifRegion,
}

/// A file, by URI.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SarifArtifact {
    /// A `file://` URI, percent-encoded.
    pub uri: String,
}

/// A region: 1-based lines, 1-based columns in characters as ty writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SarifRegion {
    /// The first line.
    pub start_line: u32,
    /// The first column.
    pub start_column: u32,
    /// The last line.
    pub end_line: u32,
    /// The column after the last.
    pub end_column: u32,
}

impl SarifArtifact {
    fn decoded(text: &str) -> Option<Vec<u8>> {
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut at = 0;
        while let Some(&byte) = bytes.get(at) {
            if byte == b'%' {
                let high = char::from(*bytes.get(at + 1)?).to_digit(HEX)?;
                let low = char::from(*bytes.get(at + 2)?).to_digit(HEX)?;
                out.push(u8::try_from(high * HEX + low).ok()?);
                at += 3;
            } else {
                out.push(byte);
                at += 1;
            }
        }
        Some(out)
    }

    /// The file the URI names; none when it is not a `file://` URI, or does not decode to UTF-8.
    #[must_use]
    pub fn path(&self) -> Option<PathBuf> {
        let encoded = self.uri.strip_prefix(FILE)?;
        Self::decoded(encoded)
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(PathBuf::from)
    }
}

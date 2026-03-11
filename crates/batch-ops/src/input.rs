//! Input parsing for batch operations.
//! Supports CSV files, inline args, and stdin.

use crate::types::{BatchOpsError, Transfer};
use alloy_primitives::{Address, U256};
use std::io::Read;
use std::str::FromStr;

/// Parses transfers from a CSV file.
///
/// Expected format (with or without header):
/// ```csv
/// address,amount
/// 0x1234...,1000000000000000000
/// 0x5678...,2000000000000000000
/// ```
pub fn parse_csv(path: &str) -> Result<Vec<Transfer>, BatchOpsError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)
        .map_err(|e| BatchOpsError::CsvParse(format!("failed to open CSV: {e}")))?;

    let mut transfers = Vec::new();
    for (i, result) in reader.records().enumerate() {
        let record = result
            .map_err(|e| BatchOpsError::CsvParse(format!("row {}: {e}", i + 1)))?;

        if record.len() < 2 {
            return Err(BatchOpsError::CsvParse(format!(
                "row {}: expected at least 2 columns (address,amount), got {}",
                i + 1,
                record.len()
            )));
        }

        let address = record[0]
            .trim()
            .parse::<Address>()
            .map_err(|e| BatchOpsError::CsvParse(format!("row {}: invalid address: {e}", i + 1)))?;

        let amount = U256::from_str(record[1].trim())
            .map_err(|e| BatchOpsError::CsvParse(format!("row {}: invalid amount: {e}", i + 1)))?;

        transfers.push(Transfer { to: address, amount });
    }

    if transfers.is_empty() {
        return Err(BatchOpsError::CsvParse("CSV file contains no transfers".to_string()));
    }

    Ok(transfers)
}

/// Parses transfers from stdin (CSV format).
pub fn parse_stdin() -> Result<Vec<Transfer>, BatchOpsError> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| BatchOpsError::CsvParse(format!("failed to read stdin: {e}")))?;

    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(input.as_bytes());

    let mut transfers = Vec::new();
    for (i, result) in reader.records().enumerate() {
        let record = result
            .map_err(|e| BatchOpsError::CsvParse(format!("row {}: {e}", i + 1)))?;

        if record.len() < 2 {
            return Err(BatchOpsError::CsvParse(format!(
                "row {}: expected at least 2 columns, got {}",
                i + 1,
                record.len()
            )));
        }

        let address = record[0]
            .trim()
            .parse::<Address>()
            .map_err(|e| BatchOpsError::CsvParse(format!("row {}: invalid address: {e}", i + 1)))?;

        let amount = U256::from_str(record[1].trim())
            .map_err(|e| BatchOpsError::CsvParse(format!("row {}: invalid amount: {e}", i + 1)))?;

        transfers.push(Transfer { to: address, amount });
    }

    if transfers.is_empty() {
        return Err(BatchOpsError::CsvParse("stdin contains no transfers".to_string()));
    }

    Ok(transfers)
}

/// Parses transfers from inline "address:amount,address:amount" format.
pub fn parse_inline(input: &str) -> Result<Vec<Transfer>, BatchOpsError> {
    let mut transfers = Vec::new();

    for (i, pair) in input.split(',').enumerate() {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }

        let parts: Vec<&str> = pair.split(':').collect();
        if parts.len() != 2 {
            return Err(BatchOpsError::CsvParse(format!(
                "entry {}: expected 'address:amount' format, got '{pair}'",
                i + 1
            )));
        }

        let address = parts[0]
            .trim()
            .parse::<Address>()
            .map_err(|e| BatchOpsError::CsvParse(format!("entry {}: invalid address: {e}", i + 1)))?;

        let amount = U256::from_str(parts[1].trim())
            .map_err(|e| BatchOpsError::CsvParse(format!("entry {}: invalid amount: {e}", i + 1)))?;

        transfers.push(Transfer { to: address, amount });
    }

    if transfers.is_empty() {
        return Err(BatchOpsError::CsvParse("no transfers provided".to_string()));
    }

    Ok(transfers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_inline() {
        let input = "0x0000000000000000000000000000000000000001:1000,0x0000000000000000000000000000000000000002:2000";
        let transfers = parse_inline(input).unwrap();
        assert_eq!(transfers.len(), 2);
        assert_eq!(transfers[0].amount, U256::from(1000));
        assert_eq!(transfers[1].amount, U256::from(2000));
    }

    #[test]
    fn test_parse_inline_with_spaces() {
        let input = " 0x0000000000000000000000000000000000000001 : 1000 , 0x0000000000000000000000000000000000000002 : 2000 ";
        let transfers = parse_inline(input).unwrap();
        assert_eq!(transfers.len(), 2);
    }

    #[test]
    fn test_parse_inline_empty() {
        assert!(parse_inline("").is_err());
    }

    #[test]
    fn test_parse_inline_invalid_format() {
        assert!(parse_inline("0x1234").is_err());
    }
}

//! Input parsing for batch operations.
//! Supports CSV files, inline args, and stdin.
//!
//! Amount values support ether unit notation via `alloy_dyn_abi`:
//! - Plain numbers: interpreted as wei (e.g. `1000000000000000000`)
//! - Hex: `0x` prefix (e.g. `0xde0b6b3a7640000`)
//! - With units: `1ether`, `1.5ether`, `10gwei`, `500000wei`

use crate::types::{BatchOpsError, Transfer};
use alloy_dyn_abi::DynSolType;
use alloy_primitives::{Address, U256};
use std::io::Read;

/// Parses an amount string into U256 (wei).
///
/// Supports:
/// - Plain integers: `1000000000000000000` (wei)
/// - Hex: `0xde0b6b3a7640000`
/// - Ether units: `1ether`, `1.5ether`, `10gwei`, `500000wei`
pub fn parse_amount(value: &str) -> Result<U256, String> {
    let value = value.trim();
    if value.starts_with("0x") {
        U256::from_str_radix(&value[2..], 16).map_err(|e| format!("invalid hex amount: {e}"))
    } else {
        DynSolType::coerce_str(&DynSolType::Uint(256), value)
            .map_err(|e| format!("invalid amount '{value}': {e}"))?
            .as_uint()
            .map(|(v, _)| v)
            .ok_or_else(|| format!("could not parse amount '{value}' as uint256"))
    }
}

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

        let amount = parse_amount(record[1].trim())
            .map_err(|e| BatchOpsError::CsvParse(format!("row {}: {e}", i + 1)))?;

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

        let amount = parse_amount(record[1].trim())
            .map_err(|e| BatchOpsError::CsvParse(format!("row {}: {e}", i + 1)))?;

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

        let amount = parse_amount(parts[1].trim())
            .map_err(|e| BatchOpsError::CsvParse(format!("entry {}: {e}", i + 1)))?;

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

    #[test]
    fn test_parse_csv_valid_data() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("transfers.csv");
        std::fs::write(
            &csv_path,
            "address,amount\n\
             0x0000000000000000000000000000000000000001,1000\n\
             0x0000000000000000000000000000000000000002,2000\n",
        )
        .unwrap();

        let transfers = parse_csv(csv_path.to_str().unwrap()).unwrap();
        assert_eq!(transfers.len(), 2);
        assert_eq!(
            transfers[0].to,
            "0x0000000000000000000000000000000000000001".parse::<Address>().unwrap()
        );
        assert_eq!(transfers[0].amount, U256::from(1000));
        assert_eq!(
            transfers[1].to,
            "0x0000000000000000000000000000000000000002".parse::<Address>().unwrap()
        );
        assert_eq!(transfers[1].amount, U256::from(2000));
    }

    #[test]
    fn test_parse_csv_file_not_found() {
        let result = parse_csv("/nonexistent/path/to/file.csv");
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            crate::types::BatchOpsError::CsvParse(msg) => {
                assert!(msg.contains("failed to open CSV"), "unexpected message: {msg}");
            }
            other => panic!("expected CsvParse error, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_csv_invalid_address() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("bad_addr.csv");
        std::fs::write(
            &csv_path,
            "address,amount\n\
             not_an_address,1000\n",
        )
        .unwrap();

        let result = parse_csv(csv_path.to_str().unwrap());
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            crate::types::BatchOpsError::CsvParse(msg) => {
                assert!(msg.contains("invalid address"), "unexpected message: {msg}");
            }
            other => panic!("expected CsvParse error, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_csv_invalid_amount() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("bad_amount.csv");
        std::fs::write(
            &csv_path,
            "address,amount\n\
             0x0000000000000000000000000000000000000001,not_a_number\n",
        )
        .unwrap();

        let result = parse_csv(csv_path.to_str().unwrap());
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            crate::types::BatchOpsError::CsvParse(msg) => {
                assert!(msg.contains("invalid amount"), "unexpected message: {msg}");
            }
            other => panic!("expected CsvParse error, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_csv_empty_header_only() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("empty.csv");
        std::fs::write(&csv_path, "address,amount\n").unwrap();

        let result = parse_csv(csv_path.to_str().unwrap());
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            crate::types::BatchOpsError::CsvParse(msg) => {
                assert!(msg.contains("no transfers"), "unexpected message: {msg}");
            }
            other => panic!("expected CsvParse error, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_csv_missing_columns() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("missing_col.csv");
        std::fs::write(
            &csv_path,
            "address\n\
             0x0000000000000000000000000000000000000001\n",
        )
        .unwrap();

        let result = parse_csv(csv_path.to_str().unwrap());
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            crate::types::BatchOpsError::CsvParse(msg) => {
                assert!(
                    msg.contains("expected at least 2 columns"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected CsvParse error, got: {other:?}"),
        }
    }

    #[test]
    fn test_parse_inline_single_entry() {
        let input = "0x0000000000000000000000000000000000000001:5000";
        let transfers = parse_inline(input).unwrap();
        assert_eq!(transfers.len(), 1);
        assert_eq!(
            transfers[0].to,
            "0x0000000000000000000000000000000000000001".parse::<Address>().unwrap()
        );
        assert_eq!(transfers[0].amount, U256::from(5000));
    }

    #[test]
    fn test_parse_inline_trailing_comma() {
        let input =
            "0x0000000000000000000000000000000000000001:1000,0x0000000000000000000000000000000000000002:2000,";
        let transfers = parse_inline(input).unwrap();
        assert_eq!(transfers.len(), 2);
        assert_eq!(transfers[0].amount, U256::from(1000));
        assert_eq!(transfers[1].amount, U256::from(2000));
    }

    // --- parse_amount ---

    #[test]
    fn test_parse_amount_plain_wei() {
        assert_eq!(parse_amount("1000").unwrap(), U256::from(1000));
        assert_eq!(parse_amount("0").unwrap(), U256::ZERO);
    }

    #[test]
    fn test_parse_amount_hex() {
        // 0xde0b6b3a7640000 = 1e18 = 1 ether
        assert_eq!(
            parse_amount("0xde0b6b3a7640000").unwrap(),
            U256::from(1_000_000_000_000_000_000u64)
        );
    }

    #[test]
    fn test_parse_amount_ether() {
        assert_eq!(
            parse_amount("1ether").unwrap(),
            U256::from(1_000_000_000_000_000_000u64)
        );
    }

    #[test]
    fn test_parse_amount_fractional_ether() {
        // 1.5 ether = 1.5e18
        assert_eq!(
            parse_amount("1.5ether").unwrap(),
            U256::from(1_500_000_000_000_000_000u64)
        );
    }

    #[test]
    fn test_parse_amount_gwei() {
        assert_eq!(
            parse_amount("10gwei").unwrap(),
            U256::from(10_000_000_000u64)
        );
    }

    #[test]
    fn test_parse_amount_fractional_gwei() {
        // 1.5 gwei = 1500000000
        assert_eq!(
            parse_amount("1.5gwei").unwrap(),
            U256::from(1_500_000_000u64)
        );
    }

    #[test]
    fn test_parse_amount_with_spaces() {
        assert_eq!(parse_amount("  1000  ").unwrap(), U256::from(1000));
        assert_eq!(
            parse_amount(" 1ether ").unwrap(),
            U256::from(1_000_000_000_000_000_000u64)
        );
    }

    #[test]
    fn test_parse_amount_scientific_notation() {
        assert_eq!(parse_amount("1e18").unwrap(), U256::from(1_000_000_000_000_000_000u64));
        assert_eq!(parse_amount("1.1e6").unwrap(), U256::from(1_100_000u64));
        assert_eq!(parse_amount("1.5e18").unwrap(), U256::from(1_500_000_000_000_000_000u64));
        assert_eq!(parse_amount("2e9").unwrap(), U256::from(2_000_000_000u64));
    }

    #[test]
    fn test_parse_amount_invalid() {
        assert!(parse_amount("abc").is_err());
        assert!(parse_amount("").is_err());
        assert!(parse_amount("1.5.3ether").is_err());
    }

    // --- inline with ether units ---

    #[test]
    fn test_parse_inline_ether_units() {
        let input = "0x0000000000000000000000000000000000000001:1.5ether,0x0000000000000000000000000000000000000002:10gwei";
        let transfers = parse_inline(input).unwrap();
        assert_eq!(transfers.len(), 2);
        assert_eq!(transfers[0].amount, U256::from(1_500_000_000_000_000_000u64));
        assert_eq!(transfers[1].amount, U256::from(10_000_000_000u64));
    }

    #[test]
    fn test_parse_csv_ether_units() {
        let dir = tempfile::tempdir().unwrap();
        let csv_path = dir.path().join("ether_transfers.csv");
        std::fs::write(
            &csv_path,
            "address,amount\n\
             0x0000000000000000000000000000000000000001,1ether\n\
             0x0000000000000000000000000000000000000002,0.5ether\n\
             0x0000000000000000000000000000000000000003,100gwei\n",
        )
        .unwrap();

        let transfers = parse_csv(csv_path.to_str().unwrap()).unwrap();
        assert_eq!(transfers.len(), 3);
        assert_eq!(transfers[0].amount, U256::from(1_000_000_000_000_000_000u64));
        assert_eq!(transfers[1].amount, U256::from(500_000_000_000_000_000u64));
        assert_eq!(transfers[2].amount, U256::from(100_000_000_000u64));
    }
}

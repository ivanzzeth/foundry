//! eth_simulateV1 tests

use alloy_consensus::BlockHeader;
use alloy_primitives::{Bytes, TxKind, U256, address, hex};
use alloy_rpc_types::{
    BlockOverrides,
    request::{TransactionInput, TransactionRequest},
    simulate::{SimBlock, SimulatePayload},
    state::{AccountOverride, StateOverridesBuilder},
};
use anvil::{NodeConfig, spawn};
use foundry_test_utils::rpc;

#[tokio::test(flavor = "multi_thread")]
async fn test_fork_simulate_v1() {
    crate::init_tracing();
    let (api, _) =
        spawn(NodeConfig::test().with_eth_rpc_url(Some(rpc::next_http_archive_rpc_url()))).await;
    let block_overrides =
        Some(BlockOverrides { base_fee: Some(U256::from(9)), ..Default::default() });
    let account_override =
        AccountOverride { balance: Some(U256::from(999999999999u64)), ..Default::default() };
    let state_overrides = Some(
        StateOverridesBuilder::with_capacity(1)
            .append(address!("0xc000000000000000000000000000000000000001"), account_override)
            .build(),
    );
    let tx_request = TransactionRequest {
        from: Some(address!("0xc000000000000000000000000000000000000001")),
        to: Some(TxKind::from(address!("0xc000000000000000000000000000000000000001"))),
        value: Some(U256::from(1)),
        ..Default::default()
    };
    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides,
            state_overrides,
            calls: vec![tx_request],
        }],
        trace_transfers: true,
        validation: false,
        return_full_transactions: true,
    };
    let _res = api.simulate_v1(payload, None).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_simulate_simple_transfer() {
    let (api, _) = spawn(NodeConfig::test()).await;

    let from = address!("0xc000000000000000000000000000000000000001");
    let to = address!("0xc000000000000000000000000000000000000002");

    let account_override =
        AccountOverride { balance: Some(U256::from(1_000_000_000_000_000_000u128)), ..Default::default() };
    let state_overrides = Some(
        StateOverridesBuilder::with_capacity(1)
            .append(from, account_override)
            .build(),
    );

    let tx_request = TransactionRequest {
        from: Some(from),
        to: Some(TxKind::Call(to)),
        value: Some(U256::from(1_000)),
        ..Default::default()
    };

    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides: None,
            state_overrides,
            calls: vec![tx_request],
        }],
        trace_transfers: false,
        validation: false,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await.unwrap();
    assert_eq!(res.len(), 1, "should return 1 block");
    assert_eq!(res[0].calls.len(), 1, "should return 1 call result");

    let call = &res[0].calls[0];
    assert!(call.status, "transfer should succeed");
    assert!(call.gas_used > 0, "gas_used should be non-zero");
    assert!(call.error.is_none(), "no error expected");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_simulate_multi_tx_state_dependency() {
    let (api, _) = spawn(NodeConfig::test()).await;

    let funder = address!("0xc000000000000000000000000000000000000001");
    let middle = address!("0xc000000000000000000000000000000000000002");
    let recipient = address!("0xc000000000000000000000000000000000000003");

    let account_override =
        AccountOverride { balance: Some(U256::from(1_000_000_000_000_000_000u128)), ..Default::default() };
    let state_overrides = Some(
        StateOverridesBuilder::with_capacity(1)
            .append(funder, account_override)
            .build(),
    );

    // tx1: funder -> middle (sends 500_000 wei)
    let tx1 = TransactionRequest {
        from: Some(funder),
        to: Some(TxKind::Call(middle)),
        value: Some(U256::from(500_000)),
        ..Default::default()
    };

    // tx2: middle -> recipient (sends 100_000 wei, depends on tx1 giving middle funds)
    let tx2 = TransactionRequest {
        from: Some(middle),
        to: Some(TxKind::Call(recipient)),
        value: Some(U256::from(100_000)),
        ..Default::default()
    };

    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides: None,
            state_overrides,
            calls: vec![tx1, tx2],
        }],
        trace_transfers: false,
        validation: false,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await.unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].calls.len(), 2);

    // tx1 should succeed
    assert!(res[0].calls[0].status, "tx1 (fund middle) should succeed");

    // tx2 should succeed because middle received funds from tx1
    assert!(res[0].calls[1].status, "tx2 (middle -> recipient) should succeed with funds from tx1");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_simulate_multi_block() {
    let (api, _) = spawn(NodeConfig::test()).await;

    let from = address!("0xc000000000000000000000000000000000000001");
    let to = address!("0xc000000000000000000000000000000000000002");

    let account_override =
        AccountOverride { balance: Some(U256::from(1_000_000_000_000_000_000u128)), ..Default::default() };
    let state_overrides = Some(
        StateOverridesBuilder::with_capacity(1)
            .append(from, account_override)
            .build(),
    );

    let tx1 = TransactionRequest {
        from: Some(from),
        to: Some(TxKind::Call(to)),
        value: Some(U256::from(1)),
        ..Default::default()
    };
    let tx2 = TransactionRequest {
        from: Some(from),
        to: Some(TxKind::Call(to)),
        value: Some(U256::from(2)),
        ..Default::default()
    };

    let payload = SimulatePayload {
        block_state_calls: vec![
            SimBlock {
                block_overrides: None,
                state_overrides,
                calls: vec![tx1],
            },
            SimBlock {
                block_overrides: None,
                state_overrides: None,
                calls: vec![tx2],
            },
        ],
        trace_transfers: false,
        validation: false,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await.unwrap();
    assert_eq!(res.len(), 2, "should return 2 blocks");

    let block0_number = res[0].inner.header.number();
    let block1_number = res[1].inner.header.number();
    assert_eq!(block1_number, block0_number + 1, "block numbers should increment");

    // parent_hash of block1 should equal hash of block0
    let block0_hash = res[0].inner.header.hash;
    let block1_parent_hash = res[1].inner.header.inner.parent_hash();
    assert_eq!(
        block1_parent_hash, block0_hash,
        "block1's parent_hash should be block0's hash"
    );

    // Both blocks should have successful calls
    assert!(res[0].calls[0].status);
    assert!(res[1].calls[0].status);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_simulate_state_overrides() {
    let (api, _) = spawn(NodeConfig::test()).await;

    // Address with no real balance
    let from = address!("0xdead000000000000000000000000000000000001");
    let to = address!("0xdead000000000000000000000000000000000002");

    // Override balance so the transfer can succeed
    let account_override =
        AccountOverride { balance: Some(U256::from(1_000_000_000_000_000_000u128)), ..Default::default() };
    let state_overrides = Some(
        StateOverridesBuilder::with_capacity(1)
            .append(from, account_override)
            .build(),
    );

    let tx_request = TransactionRequest {
        from: Some(from),
        to: Some(TxKind::Call(to)),
        value: Some(U256::from(1_000)),
        ..Default::default()
    };

    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides: None,
            state_overrides,
            calls: vec![tx_request],
        }],
        trace_transfers: false,
        validation: false,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await.unwrap();
    assert_eq!(res.len(), 1);
    assert!(res[0].calls[0].status, "transfer should succeed with overridden balance");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_simulate_block_overrides() {
    let (api, _) = spawn(NodeConfig::test()).await;

    let from = address!("0xc000000000000000000000000000000000000001");
    let to = address!("0xc000000000000000000000000000000000000002");

    let account_override =
        AccountOverride { balance: Some(U256::from(1_000_000_000_000_000_000u128)), ..Default::default() };
    let state_overrides = Some(
        StateOverridesBuilder::with_capacity(1)
            .append(from, account_override)
            .build(),
    );

    let custom_timestamp = 1_700_000_000u64;
    let custom_base_fee = 1_000_000u64;
    let block_overrides = Some(BlockOverrides {
        time: Some(custom_timestamp),
        base_fee: Some(U256::from(custom_base_fee)),
        ..Default::default()
    });

    let tx_request = TransactionRequest {
        from: Some(from),
        to: Some(TxKind::Call(to)),
        value: Some(U256::from(1)),
        ..Default::default()
    };

    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides,
            state_overrides,
            calls: vec![tx_request],
        }],
        trace_transfers: false,
        validation: false,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await.unwrap();
    assert_eq!(res.len(), 1);

    let header = &res[0].inner.header;
    assert_eq!(
        header.inner.timestamp(), custom_timestamp,
        "timestamp should match override"
    );
    assert_eq!(
        header.inner.base_fee_per_gas(),
        Some(custom_base_fee),
        "baseFee should match override"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_simulate_validation_mode() {
    let (api, _) = spawn(NodeConfig::test()).await;

    let from = address!("0xc000000000000000000000000000000000000001");
    let to = address!("0xc000000000000000000000000000000000000002");

    let account_override = AccountOverride {
        balance: Some(U256::from(1_000_000_000_000_000_000u128)),
        ..Default::default()
    };
    let state_overrides = Some(
        StateOverridesBuilder::with_capacity(1)
            .append(from, account_override)
            .build(),
    );

    // Test 1: validation=true with correct nonce (0 for fresh account) should succeed
    let tx_correct_nonce = TransactionRequest {
        from: Some(from),
        to: Some(TxKind::Call(to)),
        value: Some(U256::from(1)),
        nonce: Some(0),
        gas: Some(21_000),
        max_fee_per_gas: Some(2_000_000_000),
        max_priority_fee_per_gas: Some(1_000_000_000),
        ..Default::default()
    };

    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides: None,
            state_overrides: state_overrides.clone(),
            calls: vec![tx_correct_nonce],
        }],
        trace_transfers: false,
        validation: true,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await;
    assert!(res.is_ok(), "correct nonce with validation should succeed: {:?}", res.err());

    // Test 2: validation=true with wrong nonce should fail
    let tx_wrong_nonce = TransactionRequest {
        from: Some(from),
        to: Some(TxKind::Call(to)),
        value: Some(U256::from(1)),
        nonce: Some(999),
        gas: Some(21_000),
        max_fee_per_gas: Some(2_000_000_000),
        max_priority_fee_per_gas: Some(1_000_000_000),
        ..Default::default()
    };

    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides: None,
            state_overrides: state_overrides.clone(),
            calls: vec![tx_wrong_nonce],
        }],
        trace_transfers: false,
        validation: true,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await;
    assert!(res.is_err(), "wrong nonce with validation=true should fail");

    // Test 3: validation=true with insufficient balance should fail
    let tx_insufficient = TransactionRequest {
        from: Some(from),
        to: Some(TxKind::Call(to)),
        value: Some(U256::from(2_000_000_000_000_000_000u128)), // more than 1 ETH override
        nonce: Some(0),
        gas: Some(21_000),
        max_fee_per_gas: Some(2_000_000_000),
        max_priority_fee_per_gas: Some(1_000_000_000),
        ..Default::default()
    };

    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides: None,
            state_overrides: state_overrides.clone(),
            calls: vec![tx_insufficient],
        }],
        trace_transfers: false,
        validation: true,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await;
    assert!(res.is_err(), "insufficient balance with validation=true should fail");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_simulate_trace_transfers() {
    let (api, _) = spawn(NodeConfig::test()).await;

    let from = address!("0xc000000000000000000000000000000000000001");
    let to = address!("0xc000000000000000000000000000000000000002");

    let account_override =
        AccountOverride { balance: Some(U256::from(1_000_000_000_000_000_000u128)), ..Default::default() };
    let state_overrides = Some(
        StateOverridesBuilder::with_capacity(1)
            .append(from, account_override)
            .build(),
    );

    let tx_request = TransactionRequest {
        from: Some(from),
        to: Some(TxKind::Call(to)),
        value: Some(U256::from(1_000)),
        ..Default::default()
    };

    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides: None,
            state_overrides,
            calls: vec![tx_request],
        }],
        trace_transfers: true,
        validation: false,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await.unwrap();
    assert_eq!(res.len(), 1);
    assert!(res[0].calls[0].status, "transfer should succeed");

    // With traceTransfers=true, native ETH transfers should appear as logs
    // from the sentinel address 0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
    let logs = &res[0].calls[0].logs;
    assert!(!logs.is_empty(), "traceTransfers should produce logs for native ETH transfer");

    // Check that at least one log comes from the ETH sentinel address
    let eth_sentinel = address!("0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE");
    let has_eth_log = logs.iter().any(|log| log.address() == eth_sentinel);
    assert!(has_eth_log, "should have a log from ETH sentinel address 0xeeee...eeee");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_simulate_contract_deploy_and_call() {
    let (api, _) = spawn(NodeConfig::test()).await;

    let deployer = address!("0xc000000000000000000000000000000000000001");

    let account_override =
        AccountOverride { balance: Some(U256::from(1_000_000_000_000_000_000u128)), ..Default::default() };
    let state_overrides = Some(
        StateOverridesBuilder::with_capacity(1)
            .append(deployer, account_override)
            .build(),
    );

    // Simple contract: PUSH1 0x42 PUSH1 0x00 MSTORE PUSH1 0x20 PUSH1 0x00 RETURN
    // Returns 0x42 (66) as a 32-byte word
    // Runtime bytecode: 60 42 60 00 52 60 20 60 00 f3
    // Init code that deploys it: push runtime code to memory then return
    let init_code = Bytes::from(hex::decode(
        "69604260005260206000f3600052600a6016f3"
    ).unwrap());

    // tx1: deploy contract (TxKind::Create means create)
    let mut tx_deploy = TransactionRequest::default();
    tx_deploy.from = Some(deployer);
    tx_deploy.to = Some(TxKind::Create);
    tx_deploy.input = TransactionInput::new(init_code);

    // The deployed contract address depends on deployer + nonce.
    // For deployer 0xc000...0001, nonce=0:
    // We compute it below after getting the result
    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides: None,
            state_overrides,
            calls: vec![tx_deploy],
        }],
        trace_transfers: false,
        validation: false,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await.unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].calls.len(), 1);
    // Deploy should succeed
    assert!(res[0].calls[0].status, "contract deployment should succeed");
    // return_data for create tx contains the deployed contract address (or the deployed bytecode)
    assert!(!res[0].calls[0].return_data.is_empty(), "deploy should return bytecode");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_simulate_revert_with_data() {
    let (api, _) = spawn(NodeConfig::test()).await;

    let from = address!("0xc000000000000000000000000000000000000001");
    let contract_addr = address!("0xc000000000000000000000000000000000000099");

    // Deploy a contract that always reverts with a custom message
    // REVERT opcode: FD
    // Simple contract: PUSH1 0x00 PUSH1 0x00 REVERT  (60 00 60 00 fd)
    let revert_bytecode = Bytes::from(hex::decode("6000600060006000fd").unwrap());

    let account_override =
        AccountOverride { balance: Some(U256::from(1_000_000_000_000_000_000u128)), ..Default::default() };
    let contract_override = AccountOverride {
        code: Some(revert_bytecode),
        ..Default::default()
    };
    let state_overrides = Some(
        StateOverridesBuilder::with_capacity(2)
            .append(from, account_override)
            .append(contract_addr, contract_override)
            .build(),
    );

    let tx_request = TransactionRequest {
        from: Some(from),
        to: Some(TxKind::Call(contract_addr)),
        ..Default::default()
    };

    let payload = SimulatePayload {
        block_state_calls: vec![SimBlock {
            block_overrides: None,
            state_overrides,
            calls: vec![tx_request],
        }],
        trace_transfers: false,
        validation: false,
        return_full_transactions: true,
    };

    let res = api.simulate_v1(payload, None).await.unwrap();
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].calls.len(), 1);

    let call = &res[0].calls[0];
    assert!(!call.status, "call to reverting contract should fail");
    assert!(call.error.is_some(), "should have error info");

    let error = call.error.as_ref().unwrap();
    assert_eq!(error.code, -3200, "error code should be -3200");
    assert!(
        error.message.contains("execution reverted"),
        "error message should contain 'execution reverted', got: {}",
        error.message
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_simulate_max_blocks_exceeded() {
    let (api, _) = spawn(NodeConfig::test()).await;

    // Create 257 blocks (exceeds MAX_SIMULATE_BLOCKS = 256)
    let blocks: Vec<SimBlock> = (0..257)
        .map(|_| SimBlock {
            block_overrides: None,
            state_overrides: None,
            calls: vec![],
        })
        .collect();

    let payload = SimulatePayload {
        block_state_calls: blocks,
        trace_transfers: false,
        validation: false,
        return_full_transactions: false,
    };

    let res = api.simulate_v1(payload, None).await;
    assert!(res.is_err(), "257 blocks should exceed MAX_SIMULATE_BLOCKS limit");
    let err_msg = format!("{:?}", res.err().unwrap());
    assert!(
        err_msg.contains("too many blocks"),
        "error should mention 'too many blocks', got: {}",
        err_msg
    );
}

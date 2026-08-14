//! Independent read-only validation of a batch anchor on Base.

use serde_json::{Value, json};
use sha3::{Digest, Keccak256};

use crate::{client::Client, manifest::BatchManifest};

pub const REQUIRED_CONFIRMATIONS: u64 = 12;

#[derive(Debug)]
pub struct ChainVerification {
    pub confirmations: u64,
    pub latest_block: u64,
}

pub async fn verify(
    client: &Client,
    rpc_url: &str,
    manifest: &BatchManifest,
) -> Result<ChainVerification, String> {
    let chain_id = parse_quantity(
        rpc(client, rpc_url, "eth_chainId", json!([]))
            .await?
            .as_str()
            .ok_or_else(|| "eth_chainId did not return a hexadecimal string".to_string())?,
        "chain id",
    )?;
    let receipt = rpc(
        client,
        rpc_url,
        "eth_getTransactionReceipt",
        json!([manifest.chain_anchor.transaction_hash]),
    )
    .await?;
    let transaction = rpc(
        client,
        rpc_url,
        "eth_getTransactionByHash",
        json!([manifest.chain_anchor.transaction_hash]),
    )
    .await?;
    let latest_block = parse_quantity(
        rpc(client, rpc_url, "eth_blockNumber", json!([]))
            .await?
            .as_str()
            .ok_or_else(|| "eth_blockNumber did not return a hexadecimal string".to_string())?,
        "latest block",
    )?;
    let call_result = rpc(
        client,
        rpc_url,
        "eth_call",
        json!([{
            "to": manifest.chain_anchor.contract,
            "data": verify_calldata(&manifest.merkle_root)?,
        }, "latest"]),
    )
    .await?;

    validate_chain_data(
        manifest,
        chain_id,
        &receipt,
        &transaction,
        latest_block,
        call_result
            .as_str()
            .ok_or_else(|| "eth_call did not return hexadecimal data".to_string())?,
    )
}

async fn rpc(client: &Client, url: &str, method: &str, params: Value) -> Result<Value, String> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let body = client.public_post_json(url, &request).await?;
    let response: Value =
        serde_json::from_str(&body).map_err(|e| format!("parse {method} response: {e}"))?;
    if let Some(error) = response.get("error") {
        return Err(format!("{method} RPC error: {error}"));
    }
    response
        .get("result")
        .filter(|result| !result.is_null())
        .cloned()
        .ok_or_else(|| format!("{method} returned no result"))
}

fn validate_chain_data(
    manifest: &BatchManifest,
    chain_id: u64,
    receipt: &Value,
    transaction: &Value,
    latest_block: u64,
    call_result: &str,
) -> Result<ChainVerification, String> {
    if chain_id != manifest.chain_anchor.chain_id {
        return Err(format!(
            "RPC is on chain {chain_id}, expected {}",
            manifest.chain_anchor.chain_id
        ));
    }

    require_hash_field(
        receipt,
        "transactionHash",
        &manifest.chain_anchor.transaction_hash,
        "receipt transaction hash",
    )?;
    require_address_field(
        receipt,
        "to",
        &manifest.chain_anchor.contract,
        "receipt destination",
    )?;
    let status = quantity_field(receipt, "status", "receipt status")?;
    if status != 1 {
        return Err(format!("Base transaction reverted (status {status})"));
    }
    let receipt_block = quantity_field(receipt, "blockNumber", "receipt block number")?;
    if receipt_block != manifest.chain_anchor.block_number {
        return Err(format!(
            "receipt block {receipt_block} differs from manifest block {}",
            manifest.chain_anchor.block_number
        ));
    }

    require_hash_field(
        transaction,
        "hash",
        &manifest.chain_anchor.transaction_hash,
        "transaction hash",
    )?;
    require_address_field(
        transaction,
        "to",
        &manifest.chain_anchor.contract,
        "transaction destination",
    )?;
    let transaction_block = quantity_field(transaction, "blockNumber", "transaction block number")?;
    if transaction_block != receipt_block {
        return Err("transaction and receipt block numbers differ".into());
    }
    let input = transaction
        .get("input")
        .or_else(|| transaction.get("data"))
        .and_then(Value::as_str)
        .ok_or_else(|| "transaction omitted input calldata".to_string())?;
    require_same_hex(
        input,
        &anchor_calldata(&manifest.merkle_root, manifest.leaf_count)?,
        "anchor transaction calldata",
    )?;

    validate_anchor_event(manifest, receipt)?;
    validate_verify_result(manifest, call_result)?;

    if latest_block < receipt_block {
        return Err(format!(
            "latest block {latest_block} precedes receipt block {receipt_block}"
        ));
    }
    let confirmations = latest_block - receipt_block + 1;
    if confirmations < REQUIRED_CONFIRMATIONS {
        return Err(format!(
            "Base transaction has {confirmations} confirmations; {REQUIRED_CONFIRMATIONS} required"
        ));
    }
    Ok(ChainVerification {
        confirmations,
        latest_block,
    })
}

fn validate_anchor_event(manifest: &BatchManifest, receipt: &Value) -> Result<(), String> {
    let logs = receipt
        .get("logs")
        .and_then(Value::as_array)
        .ok_or_else(|| "transaction receipt omitted logs".to_string())?;
    let event_topic = format!(
        "0x{}",
        hex::encode(keccak(b"Anchored(bytes32,uint256,uint256)"))
    );
    let mut anchored_logs = Vec::new();
    for log in logs {
        let address = log.get("address").and_then(Value::as_str).unwrap_or("");
        let topic0 = log
            .get("topics")
            .and_then(Value::as_array)
            .and_then(|topics| topics.first())
            .and_then(Value::as_str)
            .unwrap_or("");
        if same_hex(address, &manifest.chain_anchor.contract) && same_hex(topic0, &event_topic) {
            anchored_logs.push(log);
        }
    }
    if anchored_logs.len() != 1 {
        return Err(format!(
            "expected exactly one Anchored event from the contract, found {}",
            anchored_logs.len()
        ));
    }

    let log = anchored_logs[0];
    if log.get("removed").and_then(Value::as_bool).unwrap_or(false) {
        return Err("Anchored event is marked removed".into());
    }
    let topics = log
        .get("topics")
        .and_then(Value::as_array)
        .ok_or_else(|| "Anchored event omitted topics".to_string())?;
    if topics.len() != 2 {
        return Err(format!(
            "Anchored event has {} topics, expected 2",
            topics.len()
        ));
    }
    let root_topic = topics[1]
        .as_str()
        .ok_or_else(|| "Anchored root topic is not hexadecimal".to_string())?;
    require_same_hex(
        root_topic,
        &format!("0x{}", manifest.merkle_root),
        "Anchored event root",
    )?;

    let data = decode_prefixed_hex(
        log.get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| "Anchored event omitted data".to_string())?,
        "Anchored event data",
    )?;
    if data.len() != 64 {
        return Err(format!(
            "Anchored event data is {} bytes, expected 64",
            data.len()
        ));
    }
    let leaf_count = word_as_u64(&data[..32], "Anchored leaf count")?;
    let timestamp = word_as_u64(&data[32..], "Anchored timestamp")?;
    if leaf_count != manifest.leaf_count as u64 {
        return Err(format!(
            "Anchored event leaf count {leaf_count} differs from manifest {}",
            manifest.leaf_count
        ));
    }
    if timestamp != manifest.chain_anchor.anchored_at_unix {
        return Err(format!(
            "Anchored event timestamp {timestamp} differs from manifest {}",
            manifest.chain_anchor.anchored_at_unix
        ));
    }
    Ok(())
}

fn validate_verify_result(manifest: &BatchManifest, result: &str) -> Result<(), String> {
    let data = decode_prefixed_hex(result, "verify(root) result")?;
    if data.len() != 64 {
        return Err(format!(
            "verify(root) returned {} bytes, expected 64",
            data.len()
        ));
    }
    let exists = word_as_u64(&data[..32], "verify(root) exists")?;
    if exists != 1 {
        return Err("contract verify(root) says the root is absent".into());
    }
    let timestamp = word_as_u64(&data[32..], "verify(root) timestamp")?;
    if timestamp != manifest.chain_anchor.anchored_at_unix {
        return Err(format!(
            "contract timestamp {timestamp} differs from manifest {}",
            manifest.chain_anchor.anchored_at_unix
        ));
    }
    Ok(())
}

fn anchor_calldata(root: &str, leaf_count: usize) -> Result<String, String> {
    crate::receipt::validate_sha256(root)?;
    let selector = &keccak(b"anchor(bytes32,uint256)")[..4];
    Ok(format!(
        "0x{}{}{leaf_count:064x}",
        hex::encode(selector),
        root
    ))
}

fn verify_calldata(root: &str) -> Result<String, String> {
    crate::receipt::validate_sha256(root)?;
    let selector = &keccak(b"verify(bytes32)")[..4];
    Ok(format!("0x{}{}", hex::encode(selector), root))
}

fn keccak(data: &[u8]) -> Vec<u8> {
    let mut hash = Keccak256::new();
    hash.update(data);
    hash.finalize().to_vec()
}

fn quantity_field(value: &Value, field: &str, label: &str) -> Result<u64, String> {
    let raw = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{label} is missing"))?;
    parse_quantity(raw, label)
}

fn parse_quantity(value: &str, label: &str) -> Result<u64, String> {
    let digits = value
        .strip_prefix("0x")
        .ok_or_else(|| format!("{label} is missing its 0x prefix"))?;
    if digits.is_empty() {
        return Err(format!("{label} is empty"));
    }
    u64::from_str_radix(digits, 16).map_err(|e| format!("invalid {label}: {e}"))
}

fn require_hash_field(
    value: &Value,
    field: &str,
    expected: &str,
    label: &str,
) -> Result<(), String> {
    let actual = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{label} is missing"))?;
    require_same_hex(actual, expected, label)
}

fn require_address_field(
    value: &Value,
    field: &str,
    expected: &str,
    label: &str,
) -> Result<(), String> {
    let actual = value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{label} is missing"))?;
    require_same_hex(actual, expected, label)
}

fn require_same_hex(actual: &str, expected: &str, label: &str) -> Result<(), String> {
    if !same_hex(actual, expected) {
        return Err(format!("{label} does not match the manifest"));
    }
    Ok(())
}

fn same_hex(left: &str, right: &str) -> bool {
    match (
        decode_prefixed_hex(left, "hex value"),
        decode_prefixed_hex(right, "hex value"),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn decode_prefixed_hex(value: &str, label: &str) -> Result<Vec<u8>, String> {
    let value = value
        .strip_prefix("0x")
        .ok_or_else(|| format!("{label} is missing its 0x prefix"))?;
    hex::decode(value).map_err(|e| format!("invalid {label}: {e}"))
}

fn word_as_u64(word: &[u8], label: &str) -> Result<u64, String> {
    if word.len() != 32 {
        return Err(format!("{label} is not a 32-byte ABI word"));
    }
    if word[..24].iter().any(|byte| *byte != 0) {
        return Err(format!("{label} does not fit in 64 bits"));
    }
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&word[24..]);
    Ok(u64::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{ANCHORED_EVENT, BASE_CHAIN_ID, BASE_NETWORK, BaseAnchor, TreeRules};
    use std::sync::{Arc, Mutex};

    fn manifest() -> BatchManifest {
        let leaves = vec!["11".repeat(32), "22".repeat(32), "33".repeat(32)];
        BatchManifest {
            schema: crate::manifest::SCHEMA.into(),
            period_start: "2026-08-14T12:00:00Z".into(),
            period_end: "2026-08-14T13:00:00Z".into(),
            leaf_count: leaves.len(),
            merkle_root: crate::manifest::merkle_root(&leaves).unwrap(),
            leaves,
            tree: TreeRules {
                certified_hash_version: crate::receipt::CERTIFIED_HASH_VERSION.into(),
                node_hash_algorithm: "sha256".into(),
                internal_node_prefix: "01".into(),
                padding: "duplicate-last-to-power-of-two-minimum-two".into(),
                minimum_width: 2,
            },
            chain_anchor: BaseAnchor {
                network: BASE_NETWORK.into(),
                chain_id: BASE_CHAIN_ID,
                contract: format!("0x{}", "12".repeat(20)),
                transaction_hash: format!("0x{}", "34".repeat(32)),
                block_number: 100,
                receipt_success: true,
                anchored_at_unix: 1_786_712_400,
                event: ANCHORED_EVENT.into(),
            },
        }
    }

    fn rpc_fixture(manifest: &BatchManifest) -> (Value, Value, String) {
        let event_topic = format!(
            "0x{}",
            hex::encode(keccak(b"Anchored(bytes32,uint256,uint256)"))
        );
        let event_data = format!(
            "0x{:064x}{:064x}",
            manifest.leaf_count, manifest.chain_anchor.anchored_at_unix
        );
        let receipt = json!({
            "transactionHash": manifest.chain_anchor.transaction_hash,
            "to": manifest.chain_anchor.contract,
            "status": "0x1",
            "blockNumber": "0x64",
            "logs": [{
                "address": manifest.chain_anchor.contract,
                "topics": [event_topic, format!("0x{}", manifest.merkle_root)],
                "data": event_data,
                "removed": false,
            }],
        });
        let transaction = json!({
            "hash": manifest.chain_anchor.transaction_hash,
            "to": manifest.chain_anchor.contract,
            "blockNumber": "0x64",
            "input": anchor_calldata(&manifest.merkle_root, manifest.leaf_count).unwrap(),
        });
        let call = format!(
            "0x{:064x}{:064x}",
            1, manifest.chain_anchor.anchored_at_unix
        );
        (receipt, transaction, call)
    }

    #[test]
    fn validates_the_complete_chain_evidence_tuple() {
        let manifest = manifest();
        let (receipt, transaction, call) = rpc_fixture(&manifest);
        let result =
            validate_chain_data(&manifest, BASE_CHAIN_ID, &receipt, &transaction, 111, &call)
                .unwrap();
        assert_eq!(result.confirmations, 12);
    }

    #[test]
    fn abi_selectors_match_the_deployed_contract_artifact() {
        let root = "11".repeat(32);
        assert!(anchor_calldata(&root, 3).unwrap().starts_with("0x8f5bae2e"));
        assert!(verify_calldata(&root).unwrap().starts_with("0x75e36616"));
        assert_eq!(
            hex::encode(keccak(b"Anchored(bytes32,uint256,uint256)")),
            "fae1ebf57baec5ccdbe7e05f562a97eb84281ca2ab6dcd76cdd4770f11c882db"
        );
    }

    #[test]
    fn rejects_wrong_chain_destination_calldata_and_revert() {
        let manifest = manifest();
        let (mut receipt, mut transaction, call) = rpc_fixture(&manifest);
        assert!(validate_chain_data(&manifest, 1, &receipt, &transaction, 111, &call).is_err());

        transaction["to"] = json!(format!("0x{}", "99".repeat(20)));
        assert!(
            validate_chain_data(&manifest, BASE_CHAIN_ID, &receipt, &transaction, 111, &call,)
                .is_err()
        );

        let (_, mut transaction, _) = rpc_fixture(&manifest);
        transaction["input"] = json!("0x00");
        assert!(
            validate_chain_data(&manifest, BASE_CHAIN_ID, &receipt, &transaction, 111, &call,)
                .is_err()
        );

        receipt["status"] = json!("0x0");
        let (_, transaction, _) = rpc_fixture(&manifest);
        assert!(
            validate_chain_data(&manifest, BASE_CHAIN_ID, &receipt, &transaction, 111, &call,)
                .is_err()
        );
    }

    #[test]
    fn rejects_event_contract_state_and_confirmation_mismatches() {
        let manifest = manifest();
        let (mut receipt, transaction, call) = rpc_fixture(&manifest);
        let duplicate = receipt["logs"][0].clone();
        receipt["logs"].as_array_mut().unwrap().push(duplicate);
        assert!(
            validate_chain_data(&manifest, BASE_CHAIN_ID, &receipt, &transaction, 111, &call,)
                .is_err()
        );

        let (receipt, transaction, _) = rpc_fixture(&manifest);
        let absent = format!("0x{:064x}{:064x}", 0, 0);
        assert!(
            validate_chain_data(
                &manifest,
                BASE_CHAIN_ID,
                &receipt,
                &transaction,
                111,
                &absent,
            )
            .is_err()
        );
        assert!(
            validate_chain_data(&manifest, BASE_CHAIN_ID, &receipt, &transaction, 110, &call,)
                .is_err()
        );
    }

    #[tokio::test]
    async fn verifier_uses_the_read_only_rpc_end_to_end() {
        use axum::{Json, Router, extract::State, routing::post};

        #[derive(Clone)]
        struct RpcState {
            receipt: Value,
            transaction: Value,
            call: String,
            methods: Arc<Mutex<Vec<String>>>,
        }

        async fn handle(State(state): State<RpcState>, Json(request): Json<Value>) -> Json<Value> {
            let method = request["method"].as_str().unwrap().to_string();
            state.methods.lock().unwrap().push(method.clone());
            let result = match method.as_str() {
                "eth_chainId" => json!("0x2105"),
                "eth_getTransactionReceipt" => state.receipt,
                "eth_getTransactionByHash" => state.transaction,
                "eth_blockNumber" => json!("0x6f"),
                "eth_call" => json!(state.call),
                _ => return Json(json!({"jsonrpc": "2.0", "id": 1, "error": {"code": -32601}})),
            };
            Json(json!({"jsonrpc": "2.0", "id": 1, "result": result}))
        }

        let manifest = manifest();
        let (receipt, transaction, call) = rpc_fixture(&manifest);
        let methods = Arc::new(Mutex::new(Vec::new()));
        let state = RpcState {
            receipt,
            transaction,
            call,
            methods: methods.clone(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/", post(handle)).with_state(state),
            )
            .await
            .unwrap();
        });

        let client = Client::new(&crate::config::Config::default());
        let verified = verify(&client, &format!("http://{address}"), &manifest)
            .await
            .unwrap();
        assert_eq!(verified.confirmations, 12);
        assert_eq!(
            *methods.lock().unwrap(),
            [
                "eth_chainId",
                "eth_getTransactionReceipt",
                "eth_getTransactionByHash",
                "eth_blockNumber",
                "eth_call",
            ]
        );
        server.abort();
    }
}

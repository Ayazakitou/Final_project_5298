use lyquid::http::{Header, Method, Request, RequestOptions};
use lyquid::prelude::*;
use lyquid::runtime::lyquor_api;
use lyquor_primitives::{alloy_primitives::keccak256, Address, B256, U256};
use serde_json::json;
use std::str::FromStr;

const DEFAULT_SEPOLIA_RPC_URL: &str = "https://ethereum-sepolia-rpc.publicnode.com";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransferLog {
    pub from: Address,
    pub to: Address,
    pub value: U256,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuerySummary {
    pub count: u64,
    pub total_value: U256,
    pub commitment: B256,
}

state! {
    network target_contract: Address = Address::ZERO;
    network event_topic: B256 = B256::ZERO;

    network oracle index_result;

    instance last_query_key: String = String::new();
    instance log_cache: Vec<TransferLog> = Vec::new();
    instance latest_commitment: B256 = B256::ZERO;
}

#[method::network(export = eth)]
fn constructor(ctx: &mut _, contract: Address, topic: B256) {
    *ctx.network.target_contract = contract;
    *ctx.network.event_topic = topic;
}

#[method::instance(export = eth)]
fn query_count(
    ctx: &mut _,
    from_block: u64,
    to_block: u64,
    filter: Address,
) -> LyquidResult<u64> {
    let summary = execute_indexed_query(&mut ctx, from_block, to_block, filter)?;
    Ok(summary.count)
}

#[method::instance(export = eth)]
fn query_aggregate(
    ctx: &mut _,
    from_block: u64,
    to_block: u64,
    filter: Address,
) -> LyquidResult<U256> {
    let summary = execute_indexed_query(&mut ctx, from_block, to_block, filter)?;
    Ok(summary.total_value)
}

#[method::instance(export = eth)]
fn query_commitment(ctx: &mut _) -> LyquidResult<B256> {
    Ok(*ctx.instance.latest_commitment.read())
}

fn execute_indexed_query(
    ctx: &mut crate::__lyquid::InstanceContext,
    from_block: u64,
    to_block: u64,
    filter: Address,
) -> LyquidResult<QuerySummary> {
    let contract = *ctx.network.target_contract;
    let topic = *ctx.network.event_topic;
    let query_key = build_query_key(contract, topic, from_block, to_block);

    let cached_key = ctx.instance.last_query_key.read().clone();
    let cached_logs = ctx.instance.log_cache.read().clone();

    let logs = if cached_key == query_key && !cached_logs.is_empty() {
        cached_logs
    } else {
        let fetched = fetch_transfer_logs_with_rpc(
            DEFAULT_SEPOLIA_RPC_URL,
            contract,
            topic,
            from_block,
            to_block,
        )?;
        *ctx.instance.last_query_key.write() = query_key;
        *ctx.instance.log_cache.write() = fetched.clone();
        fetched
    };

    let summary = summarize_logs(&logs, contract, topic, from_block, to_block, filter);
    *ctx.instance.latest_commitment.write() = summary.commitment;
    Ok(summary)
}

fn build_query_key(
    contract: Address,
    topic: B256,
    from_block: u64,
    to_block: u64,
) -> String {
    format!("{contract:?}:{topic:?}:{from_block}:{to_block}")
}

fn format_block_param(block: u64, is_upper_bound: bool) -> String {
    if block == 0 {
        if is_upper_bound {
            "latest".to_string()
        } else {
            "0x0".to_string()
        }
    } else {
        format!("0x{block:x}")
    }
}

fn build_get_logs_payload(
    target_contract: Address,
    event_topic: B256,
    from_block: u64,
    to_block: u64,
) -> String {
    let from_str = format_block_param(from_block, false);
    let to_str = format_block_param(to_block, true);

    json!({
        "jsonrpc": "2.0",
        "method": "eth_getLogs",
        "params": [{
            "address": format!("{target_contract:?}"),
            "topics": [format!("{event_topic:?}")],
            "fromBlock": from_str,
            "toBlock": to_str
        }],
        "id": 1
    })
    .to_string()
}

fn fetch_transfer_logs_with_rpc(
    rpc_url: &str,
    target_contract: Address,
    event_topic: B256,
    from_block: u64,
    to_block: u64,
) -> LyquidResult<Vec<TransferLog>> {
    let payload = build_get_logs_payload(target_contract, event_topic, from_block, to_block);

    let req = Request {
        method: Method::Post,
        url: rpc_url.to_string(),
        headers: vec![Header {
            name: "Content-Type".to_string(),
            value: b"application/json".to_vec(),
        }],
        body: Some(payload.into_bytes()),
    };

    let resp = lyquor_api::http_request(
        req,
        Some(RequestOptions {
            timeout_ms: Some(15_000),
        }),
    )?;

    let body_str = String::from_utf8_lossy(&resp.body);
    parse_transfer_logs(&body_str).map_err(|error| {
        LyquidError::LyquidRuntime(format!(
            "failed to parse eth_getLogs response: {error}"
        ))
    })
}

pub fn summarize_logs(
    logs: &[TransferLog],
    contract: Address,
    topic: B256,
    from_block: u64,
    to_block: u64,
    filter: Address,
) -> QuerySummary {
    let mut count: u64 = 0;
    let mut total_value = U256::ZERO;
    let mut hasher_input = Vec::new();

    hasher_input.extend_from_slice(b"VERIINDEX_COMMITMENT_V1");
    hasher_input.extend_from_slice(contract.as_slice());
    hasher_input.extend_from_slice(topic.as_slice());
    hasher_input.extend_from_slice(&from_block.to_be_bytes());
    hasher_input.extend_from_slice(&to_block.to_be_bytes());
    hasher_input.extend_from_slice(filter.as_slice());

    for log in logs {
        let matched = filter == Address::ZERO || log.from == filter || log.to == filter;
        if matched {
            count += 1;
            total_value += log.value;
            hasher_input.extend_from_slice(log.from.as_slice());
            hasher_input.extend_from_slice(log.to.as_slice());
            hasher_input.extend_from_slice(&log.value.to_be_bytes::<32>());
        }
    }

    hasher_input.extend_from_slice(&count.to_be_bytes());
    hasher_input.extend_from_slice(&total_value.to_be_bytes::<32>());

    let commitment: B256 = keccak256(&hasher_input).0.into();
    QuerySummary {
        count,
        total_value,
        commitment,
    }
}

fn parse_transfer_logs(body: &str) -> Result<Vec<TransferLog>, String> {
    let response: serde_json::Value =
        serde_json::from_str(body).map_err(|error| format!("invalid JSON: {error}"))?;

    if let Some(err) = response.get("error") {
        return Err(format!("RPC error: {err}"));
    }

    let entries = response
        .get("result")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "missing or invalid result array".to_string())?;

    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let field = |name: &str| {
                entry
                    .get(name)
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("log {index}: missing or invalid {name} field"))
            };
            let topic_address = |topic_index: usize, name: &str| {
                let topic = entry
                    .get("topics")
                    .and_then(serde_json::Value::as_array)
                    .and_then(|topics| topics.get(topic_index))
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("log {index}: missing or invalid {name} topic"))?;
                let hex = topic
                    .strip_prefix("0x")
                    .ok_or_else(|| format!("log {index}: {name} topic must start with 0x"))?;
                let bytes = hex.as_bytes();
                if bytes.len() != 64 || !bytes[..24].iter().all(|byte| *byte == b'0') {
                    return Err(format!("log {index}: invalid {name} topic encoding"));
                }
                Address::from_str(&format!("0x{}", &hex[24..]))
                    .map_err(|error| format!("log {index}: invalid {name} address: {error}"))
            };

            let from = topic_address(1, "from")?;
            let to = topic_address(2, "to")?;
            let value = field("data")?
                .strip_prefix("0x")
                .ok_or_else(|| format!("log {index}: data must be a 0x-prefixed hex value"))
                .and_then(|hex| {
                    U256::from_str_radix(hex, 16)
                        .map_err(|error| format!("log {index}: invalid data value: {error}"))
                })?;

            Ok(TransferLog { from, to, value })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ETH_GET_LOGS_RESPONSE: &str = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "result": [
            {
                "address": "0xfffffffffffffffffffffffffffffffffffffffe",
                "topics": [
                    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                    "0x00000000000000000000000016651f6be3941139e76442724c45fe631c3f4063",
                    "0x000000000000000000000000c44f0fd69b6352d73d3f1735bcd34b396366784b"
                ],
                "data": "0x0000000000000000000000000000000000000000000000000028729c74c04ac4"
            },
            {
                "address": "0xfffffffffffffffffffffffffffffffffffffffe",
                "topics": [
                    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                    "0x00000000000000000000000035af12026bfdde24f12ebf3ad926fc50c3f96abf",
                    "0x000000000000000000000000a3f001d419e907ceea92a59604a15047631b3231"
                ],
                "data": "0x0000000000000000000000000000000000000000000000008ac7230489e80000"
            }
        ]
    }"#;


    #[lyquid_test::test]
    fn test_parse_valid_transfer_log() {
        let raw_json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "result": [
                {
                    "blockNumber": "0x7a1201",
                    "topics": [
                        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
                        "0x0000000000000000000000001111111111111111111111111111111111111111",
                        "0x0000000000000000000000002222222222222222222222222222222222222222"
                    ],
                    "data": "0x00000000000000000000000000000000000000000000000000000000000003e8"
                }
            ]
        }"#;

        let logs = parse_transfer_logs(raw_json).expect("valid Transfer log should parse");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].block_number, 8_000_001);
        assert_eq!(logs[0].from, Address::from([0x11; 20]));
        assert_eq!(logs[0].to, Address::from([0x22; 20]));
        assert_eq!(logs[0].value, U256::from(1_000_u64));
    }

    #[lyquid_test::test]   
    fn test_parse_malformed_input_returns_err() {
        let bad_json = "{ not valid json";
        assert!(parse_transfer_logs(bad_json).is_err());

        let missing_fields = r#"{ "jsonrpc": "2.0", "id": 1, "result": [ { "topics": [] } ] }"#;
        assert!(parse_transfer_logs(missing_fields).is_err());
    }

    #[lyquid_test::test]
    fn test_parse_empty_logs() {
        let empty_json = r#"{ "jsonrpc": "2.0", "id": 1, "result": [] }"#;
        let logs = parse_transfer_logs(empty_json).expect("empty log list should succeed");
        assert!(logs.is_empty());
    }

    #[lyquid_test::test]
    fn test_filter_zero_address_matches_all() {
        let logs = sample_logs();
        let matched: Vec<_> = logs
            .iter()
            .filter(|l| Address::ZERO == Address::ZERO || l.from == Address::ZERO || l.to == Address::ZERO)
            .collect();
        assert_eq!(matched.len(), 3);
    }

    #[lyquid_test::test]
    fn test_filter_specific_address_from_and_to() {
        let logs = sample_logs();
        let target = Address::from([0x22; 20]);
        let matched: Vec<_> = logs
            .iter()
            .filter(|l| l.from == target || l.to == target)
            .collect();
        assert_eq!(matched.len(), 2);
    }

    #[lyquid_test::test]
    fn test_block_range_filter_count_and_aggregate_sum() {
        let logs = sample_logs();
        let from_block = 8_000_000_u64;
        let to_block = 8_000_100_u64;
        let target = Address::from([0x22; 20]);

        let filtered: Vec<_> = logs
            .iter()
            .filter(|l| {
                l.block_number >= from_block
                    && l.block_number <= to_block
                    && (l.from == target || l.to == target)
            })
            .collect();

        let count = filtered.len() as u64;
        let total: U256 = filtered.iter().fold(U256::ZERO, |acc, l| acc + l.value);

        assert_eq!(count, 2);
        assert_eq!(total, U256::from(3_500_u64));
    }
}
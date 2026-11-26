# my project

**5298 Final project** is an on-chain/off-chain-assisted log indexing and query service built on the **Lyquid LDK (v0.8.4)**, designed to parse, filter, and aggregate Ethereum event logs while exposing verifiable query interfaces and Solidity sequencer bindings.

---

## Key Features & Highlights

* **Lyquid LDK v0.8.4 Integration**: Built with Rust and compiled to `wasm32-unknown-unknown` for Lyquid execution and Solidity contract generation via `shaker solidity`.
* **Verified Evaluation on Sepolia USDC**: Benchmarked across **24,135 Sepolia USDC** event logs, achieving **~104 ms** cache re-parse latency versus **~4 s** cold fetch (**~39x speedup**).
* **Committee BFT Threshold Validation**: Validated quorum thresholds for committee sizes $N \in \{3, 4, 7, 10\}$ under the $\lceil 2N/3 \rceil$ rule.
* **Resilient Build & Test Workflow**: Includes workarounds for read-only filesystem target directories (`CARGO_TARGET_DIR=/tmp/...`), host vs. WASM test targets, and `shaker build` CLI panics.

---

## Tech Stack & Prerequisites

* **Rust & Cargo** (`wasm32-unknown-unknown` target installed via `rustup target add wasm32-unknown-unknown`)
* **Lyquid LDK (`v0.8.4`)** & **`shaker` CLI**
* **Docker & Docker Compose** (for the single-node local devnet)
* **Foundry (`cast`)** (for ABI encoding, contract calls, and transactions against `http://127.0.0.1:10087/api`)
* **Python 3** (for running evaluation and BFT simulation scripts)

---

## Quick Start: Local Devnet & Deployment

### 1. Start the Local Devnet

```bash
docker compose -f ~/.shakenup/ldk/docker/single/docker-compose.yaml up -d
docker compose -f ~/.shakenup/ldk/docker/single/docker-compose.yaml ps
```

### 2. Build `wasm32` Artifact & Export Solidity Interface

To avoid `shaker build` CLI panics, compile the `wasm32` artifact with `cargo` first and pass the `.wasm` path directly to `shaker solidity`:

```bash
# Optional: redirect target directory to /tmp if the project directory is read-only
export CARGO_TARGET_DIR=/tmp/cargo-target-my-project

cargo build --target wasm32-unknown-unknown
shaker solidity "$CARGO_TARGET_DIR"/wasm32-unknown-unknown/debug/_5298Fin.wasm
grep -E "function query_" generated/SequenceBackend.sol
```

### 3. Deploy my project

Deploy using the Sepolia USDC address (`0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238`) and ERC-20 `Transfer` topic (`0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef`):

```bash
shaker deploy Cargo.toml \
  --input $(cast abi-encode "constructor(address,bytes32)" \
    "0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238" \
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef")

shaker list
```

---

## Testing, Evaluation & BFT Simulation

### Unit Tests

Run the unit tests covering JSON log parsing, malformed input error handling, and transfer filtering/aggregation:

```bash
CARGO_TARGET_DIR=/tmp/cargo-target-my-project cargo test --lib
```

### Sepolia Evaluation (24,135 Events)

```bash
python3 scripts/evaluate_correctness.py
```

* **Dataset**: 24,135 Sepolia USDC events
* **Cold Fetch**: ~4 s
* **Cache Re-parse**: ~104 ms (**~39x speedup**)

### Committee BFT Threshold Simulation

```bash
python3 scripts/simulate_committee.py
```

| Committee Size ($N$) | Quorum Rule ($\lceil 2N/3 \rceil$) | Required Votes | Max Faulty Nodes ($f$) |
| :---: | :---: | :---: | :---: |
| 3 | $\lceil 6/3 \rceil$ | 2 | 1 |
| 4 | $\lceil 8/3 \rceil$ | 3 | 1 |
| 7 | $\lceil 14/3 \rceil$ | 5 | 2 |
| 10 | $\lceil 20/3 \rceil$ | 7 | 3 |

---

## Troubleshooting

* **`Read-only file system (os error 30)`**: Set `CARGO_TARGET_DIR=/tmp/cargo-target-my-project` so Cargo writes build artifacts and lock files to `/tmp`.
* **`Exec format error (os error 8)` when running `cargo test`**: Avoid executing `.wasm` test binaries directly on an `x86_64` host; run host-compatible tests with `--target x86_64-unknown-linux-gnu` where applicable.
* **`cast send` connection or private key errors**: Verify the devnet gateway is reachable at `http://127.0.0.1:10087/api`, keep commands on valid lines without broken URLs, and pass a valid 32-byte (64-hex-character) test private key rather than an address.

---

## Repository

Source code, scripts, and documentation: `https://github.com/Ayazakitou/Final_project_5298`
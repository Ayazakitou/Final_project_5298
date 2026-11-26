#!/usr/bin/env python3
"""
Parameterized BFT committee simulation for VeriIndex.
- Supports arbitrary committee size N and 2/3 threshold.
- Injects f malicious nodes and checks whether the committee
  accepts the honest value, accepts the wrong value, or rejects.
"""

import json
from collections import Counter

# ---------- Mock eth_getLogs response (same as before) ----------
ETH_GET_LOGS_RESPONSE = r"""{
  "jsonrpc": "2.0",
  "id": 1,
  "result": [
    {
      "topics": [
        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
        "0x00000000000000000000000016651f6be3941139e76442724c45fe631c3f4063",
        "0x000000000000000000000000c44f0fd69b6352d73d3f1735bcd34b396366784b"
      ],
      "data": "0x0000000000000000000000000000000000000000000000000028729c74c04ac4"
    },
    {
      "topics": [
        "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
        "0x00000000000000000000000035af12026bfdde24f12ebf3ad926fc50c3f96abf",
        "0x000000000000000000000000a3f001d419e907ceea92a59604a15047631b3231"
      ],
      "data": "0x0000000000000000000000000000000000000000000000008ac7230489e80000"
    }
  ]
}"""

FILTER_ADDRESS = "0x16651f6be3941139e76442724c45fe631c3f4063"


def parse_transfer_logs(body: str):
    resp = json.loads(body)
    if "error" in resp:
        raise ValueError(f"RPC error: {resp['error']}")
    entries = resp.get("result")
    if not isinstance(entries, list):
        raise ValueError("missing or invalid result array")

    logs = []
    for i, entry in enumerate(entries):
        topics = entry.get("topics", [])
        if len(topics) < 3:
            raise ValueError(f"log {i}: missing topics")
        from_addr = "0x" + topics[1][-40:]
        to_addr = "0x" + topics[2][-40:]
        data = entry.get("data", "")
        if not data.startswith("0x"):
            raise ValueError(f"log {i}: data must be 0x-prefixed")
        value = int(data[2:], 16) if data[2:] else 0
        logs.append({"from": from_addr.lower(), "to": to_addr.lower(), "value": value})
    return logs


def node_query(body: str, filter_addr: str):
    logs = parse_transfer_logs(body)
    f = filter_addr.lower()
    matched = [l for l in logs if f == "0x" + "0" * 40 or l["from"] == f or l["to"] == f]
    return {"count": len(matched), "sum": sum(l["value"] for l in matched)}


def required_votes(n: int) -> int:
    """2/3 threshold: ceil(2N/3), computed with integer arithmetic."""
    return (2 * n + 2) // 3


def run_committee(n: int, f_tampered: int):
    """
    Simulate a committee of n nodes with f_tampered malicious nodes.
    Returns a dict describing the outcome.
    """
    honest = node_query(ETH_GET_LOGS_RESPONSE, FILTER_ADDRESS)
    bad = {"count": honest["count"] + 5, "sum": honest["sum"] + 999}

    honest_votes = n - f_tampered
    bad_votes = f_tampered
    required = required_votes(n)

    if honest_votes >= required:
        outcome = "ACCEPT_HONEST"
        accepted_value = honest
    elif bad_votes >= required:
        outcome = "ACCEPT_WRONG"
        accepted_value = bad
    else:
        outcome = "REJECT"
        accepted_value = None

    return {
        "n": n,
        "f": f_tampered,
        "required": required,
        "honest_votes": honest_votes,
        "bad_votes": bad_votes,
        "outcome": outcome,
        "accepted_value": accepted_value,
    }


def run_matrix():
    """
    Run a matrix over committee sizes and tampered node counts.
    Prints a Markdown table for direct use in the report.
    """
    # (n, [list of f values to test])
    configs = [
        (3, [0, 1, 2]),
        (4, [0, 1, 2]),
        (7, [0, 1, 2, 3]),
        (10, [0, 1, 2, 3, 4]),
    ]

    print("| N | Required (2/3) | Malicious f | Honest votes | Malicious votes | Outcome |")
    print("|---|---|---|---|---|---|")

    results = []
    for n, f_list in configs:
        req = required_votes(n)
        for f in f_list:
            r = run_committee(n, f)
            results.append(r)
            print(
                f"| {r['n']} | {r['required']} | {r['f']} | "
                f"{r['honest_votes']} | {r['bad_votes']} | {r['outcome']} |"
            )
    print()
    return results


def summarize(results):
    """
    Print a short summary of how many malicious nodes each committee size tolerates.
    """
    print("=== Trust boundary summary ===")
    by_n = {}
    for r in results:
        by_n.setdefault(r["n"], []).append(r)

    for n in sorted(by_n):
        entries = by_n[n]
        max_tolerated = 0
        first_failure = None
        for r in entries:
            if r["outcome"] == "ACCEPT_HONEST":
                max_tolerated = max(max_tolerated, r["f"])
            elif r["outcome"] == "ACCEPT_WRONG" and first_failure is None:
                first_failure = r["f"]
        print(
            f"N={n}: tolerates up to {max_tolerated} malicious node(s); "
            f"first failure at f={first_failure}."
        )

    print()
    print("BFT bound n >= 3f + 1 check:")
    for n in sorted(by_n):
        # maximum f satisfying n >= 3f + 1
        f_max = (n - 1) // 3
        print(f"  N={n}: theoretical max tolerable f = {f_max}")


def main():
    print("=== Parameterized committee simulation ===")
    print("Threshold rule: required votes = ceil(2N/3)")
    print("Outcomes: ACCEPT_HONEST / ACCEPT_WRONG / REJECT\n")

    results = run_matrix()
    summarize(results)

    print("\nAll simulations completed.")


if __name__ == "__main__":
    main()
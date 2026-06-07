# Design: Custom Router Policies Guide

## Overview
This document outlines the design for a new guide in `docs/guides/custom-router-policies.md`. The goal is to teach developers how to implement custom routing logic in both Rust (for core engine extension) and Python (for client-side control/understanding).

## Target Audience
*   **Core Contributors**: Developers wanting to implement new `RoutingStrategy` implementations in Rust.
*   **Platform Engineers**: Users wanting to understand how to control traffic routing via configuration and how to implement sophisticated, stateful routing patterns.

## Key Topics

### 1. Conceptual Overview
*   **The Router Engine**: How `RouterEngine` acts as the orchestrator.
*   **The Strategy Pattern**: Decoupling the "how to pick" (`RoutingStrategy`) from the "what is available" (`DeploymentPool`).
*   **The Feedback Loop**: The critical role of `RoutingState`. How the engine uses `record_success` and `record_failure` to update the strategy's knowledge in real-time.

### 2. Rust Implementation (Deep Dive)
*   **The `RoutingStrategy` Trait**:
    *   `name()`: Unique identifier.
    *   `select(...)`: The core logic function.
*   **Deep Dive: State-Aware Routing**:
    *   Explain how `RoutingState` provides access to deployment performance metrics.
    *   **Code Example**: `LatencyBasedStrategy`
        *   Demonstrates implementing the `select` method.
        *   Shows how to read latency/error data from `RoutingState`.
        *   Provides a complete, compilable example with a `MockState`.

### 3. Python Implementation
*   **Client-Side Routing Control**: How to influence routing using `model_aliases` and `routing_groups` via the Python `Client`.
*   **Understanding the Mapping**: A guide on how a Python `client.chat(model="my-alias", ...)` call is resolved by the Rust `RouterEngine`.

### 4. Comparison Table: Rust vs. Python
| Rust Component | Python Client Aspect | Description |
| :--- | :--- | :--- |
| `RoutingStrategy` | `routing_groups` | The logic used to pick a deployment |
| `Deployment` | `model` / `alias` | The target endpoint being routed to |
| `RoutingState` | (Automatic/Internal) | Real-time performance metrics used by strategies |

## Implementation Plan
1.  **Write Design Doc**: Save to `docs/superpowers/specs/2026-06-06-custom-router-policies-design.md`.
2.  **Write Guide**: Create `docs/guides/custom-router-policies.md` with full dual-language examples.
3.  **Verify**: Ensure the Rust code compiles and the Python usage is accurate to the current `client.py`.

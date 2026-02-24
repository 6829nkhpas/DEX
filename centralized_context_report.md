# Centralized Context Report v1.0

## What was found automatically
1. **Repository Structure**: Identified `/spec`, `/libs/types`, `/services/gateway`, and `/chain/contracts` as primary sources of truth.
2. **Canonical Specifications**: Scanned deterministic rules, timestamp policies, order lifecycles, and service boundaries. Extracted strict monotonic rules and timezone requirements (always UTC).
3. **Data Types**: Extracted `Order`, `Price`, `Quantity`, `TimeInForce`, and `OrderStatus` types spanning Rust structs to explicitly string-serialized fixed-point JSON arrays.
4. **WebSocket Definitions**: Read protocol constraints, rate limits (10 per account), and snapshot/delta message structures.

## Which definitions were inferred
1. **REST Endpoints**: Endpoints like `GET /v1/orders/:id` and `GET /v1/accounts/:id` were inferred from the `09-service-boundaries.md` definitions, as they weren't explicitly wired into the current `gateway/src/router.rs` file.
2. **Smart Contract ABI**: Vault contract (`chain/contracts/src/vault.rs`) read/write methods were extracted manually from Rust signatures as compiled CosmWasm/Solidity JSON ABIs were not natively present in the artifacts checked.

## Missing artifacts or ambiguities
- **TODO**: *gateway/src/router.rs, line 10-22* - `GET /v1/orders/:id` and `GET /v1/accounts/:id` are missing in implementation.
  - *Suggested Resolution*: Implement these handler routes in the Rust API Gateway to align with `09-service-boundaries.md`.
- **TODO**: *chain/contracts/src/vault.rs* - Missing explicit compiled JSON ABI.
  - *Suggested Resolution*: Add a compilation step (`cargo wasm`) to emit `.json` ABI schema files automatically into `/chain/contracts/artifacts`.

## Validation results
- **JSON linting**: Passed. The `/centralized_context.json` is structurally sound.
- **OpenAPI validation**: Passed. Proper reference linkages for `CreateOrderRequest` and `OrderResponse`.
- **TypeScript compilation**: Provided valid interfaces, Enums, and custom type aliases avoiding `any`. Mock TS check passed.
- **Example Validation**: Example payloads match the definitions for Order APIs and WebSocket Deltas.

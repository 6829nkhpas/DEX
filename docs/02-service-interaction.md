# Service Interaction Document

**Phase**: Launch & Operational Docs  
**Component**: System Architecture

## 1. Overview

This document details the interactions between the distributed exchange's core microservices throughout key flows, specifically order creation, matching, and settlement. These interactions strictly adhere to the Service Boundaries Specification (Spec 09) and Lifecycle Specifications (Specs 01 and 03).

## 2. Order Creation Flow

The order creation flow is latency-critical and relies on synchronous (gRPC) communication to ensure immediate feedback to the client.

```mermaid
sequenceDiagram
    participant Client
    participant Gateway as API Gateway
    participant OS as Order Service
    participant AS as Account Service
    participant RS as Risk Service
    participant ME as Matching Engine
    
    Client->>Gateway: POST /v1/orders
    Gateway->>Gateway: Validate Auth & Rate Limits
    Gateway->>OS: CreateOrder(OrderRequest)
    
    opt Validations
        OS->>OS: Schema & Rule Check
        OS->>AS: CheckBalance(AccountID)
        AS-->>OS: OK (Margin Reserved)
        OS->>RS: CheckRiskLimits(AccountID, Order)
        RS-->>OS: OK
    end
    
    OS->>OS: Assign OrderID & Sequence
    OS->>OS: Persist to Journal
    OS->>ME: SubmitOrder(Order)
    
    OS-->>Gateway: Order Accepted (OrderID)
    Gateway-->>Client: 201 Created (OrderID)
```

## 3. Trade Execution & Settlement Flow

Trade execution happens continuously in the Matching Engine. When a match occurs, the settlement pipeline takes over asynchronously using event streams.

```mermaid
sequenceDiagram
    participant ME as Matching Engine
    participant EventBus as Event Stream
    participant SS as Settlement Service
    participant AS as Account Service
    participant MD as Market Data Service
    
    Note over ME: Trade Matched
    ME->>ME: Calculate Quantities & Prices
    ME->>EventBus: Emit TradeExecuted Event
    
    par Asynchronous Processing
        EventBus->>MD: Consume TradeExecuted
        MD->>MD: Update OHLCV & Order Book Mirror
        
        EventBus->>SS: Consume TradeExecuted
        SS->>SS: Calculate Fees (Maker/Taker)
        SS->>AS: Update Position (Maker)
        AS-->>SS: Ack
        SS->>AS: Update Position (Taker)
        AS-->>SS: Ack
        
        SS->>SS: Persist Settlement Status
        SS->>EventBus: Emit TradeSettled Event
    end
    
    EventBus->>MD: Consume TradeSettled
```

## 4. Failure Isolation Mechanisms

Inter-service communication utilizes circuit breakers to prevent catastrophic cascading failures.

### 4.1 Degraded Operational Modes

- **Order Service -> Matching Engine**: If the Matching Engine becomes unreachable (Circuit Breaker opens after 5 failures), Order Service queues orders or returns a 503 Service Unavailable depending on the queue capacity.
- **Settlement Service -> Account Service**: If the Account Service is unreachable, Settlement Service retries indefinitely (Settlement is idempotent via `trade_id`).
- **Market Data Service Extraction**: Market Data Service can serve stale data if its internal clock or upstream feed is lagging (up to a tolerance limit, commonly 10 seconds).

## 5. Risk & Liquidation Monitoring

Risk Service continually polls or consumes position updates to monitor account health in real time.

```mermaid
sequenceDiagram
    participant AS as Account Service
    participant MD as Market Data Service
    participant RS as Risk Service
    participant LS as Liquidation Service
    
    loop Every Second
        RS->>MD: GetMarkPrice(Symbol)
        MD-->>RS: Mark Price
        RS->>AS: GetPositions()
        AS-->>RS: Positions List
        
        RS->>RS: Calculate Margin Ratios
        
        opt Margin Ratio < Maintenance Margin
            RS->>LS: TriggerLiquidation(AccountID)
            LS->>AS: Takeover Position
        end
    end
```

## 6. Interface Contracts

All gRPC communications are defined by strict Protobuf definitions located in `/spec/proto/`. Versioning is handled at the URI level (e.g., `/v1/orders`), with a backward compatibility guarantee of 6 months for any deprecated features.

# Risk Disclosure

**Phase**: Launch & Operational Docs  
**Component**: Legal & Compliance

## 1. Overview

Trading on a decentralized, high-performance exchange involves significant risks. This document outlines the key mechanics traders must understand before interacting with the system, specifically regarding liquidations and catastrophic failure safeguards.

## 2. Leverage and Liquidation Risk

Trading with leverage amplifies both potential profits and potential losses. 

- **Maintenance Margin**: Positions are continuously evaluated. If your Account Margin Ratio drops below `1.10`, your position will be immediately liquidated (Spec 06).
- **Grace Periods**: Generally, there are NO grace periods. Liquidations occur within milliseconds of the Mark Price crossing your liquidation threshold.
- **No Negative Balances**: The liquidation engine and Insurance Fund are designed to prevent your account balance from dropping below zero. However, under extreme volatility, you may lose 100% of your initial margin.

## 3. Auto-Deleveraging (ADL) Risk

In the event of catastrophic market collapse where the Insurance Fund is depleted and an underwater position cannot be liquidated on the open market, the exchange employs **Auto-Deleveraging (ADL)**.

If you hold a highly profitable position during such an event, your position may be force-closed (at the bankruptcy price of the liquidated counterparty) to offset system losses.
- **Selection**: The ADL queue targets traders with the highest profit and highest leverage first.

## 4. Technical Risks

While the exchange guarantees high availability (99.99%) and deterministic state recovery, the following technical risks remain:
1. **API Latency and Rate Limiting**: Automated scraping or spamming will trigger 429 errors. Stop-loss orders executed via API might suffer from network latency if triggered during a DDOS attack.
2. **Maintenance Mode**: The Governance team (Spec 17) retains the right to halt trading or place the exchange in 'Cancel-Only' mode during emergency events or identified exploits. You may be unable to close positions during these halts.
3. **Blockchain Settlement Finality**: External deposits/withdrawals are subject to the underlying blockchain's consensus rules and network congestion.

## 5. Acknowledgment

By creating an API Key or placing an order, the User programmatically acknowledges that they have read and comprehended the Lifecycle, Fee, and Liquidation specifications governing this environment.

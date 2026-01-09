
![Radiant](https://github.com/user-attachments/assets/58917a90-9730-448a-90a6-8f39b5b955b8)

# Radiant CA:

**Radiant** is a **Solana-based lending and borrowing protocol** built with **Anchor**, designed to enable efficient on-chain credit markets with real-time risk monitoring and automated interest accrual.

The protocol allows users to **deposit collateral**, **borrow supported assets**, and **liquidate unhealthy positions** in a transparent and permissionless manner, leveraging Solana’s high throughput and low latency.

---

## 🌐 Overview

Radiant provides a non-custodial framework for decentralized lending on Solana. Users interact directly with on-chain programs to manage collateralized debt positions while the protocol continuously monitors account health to maintain system solvency.

Key objectives of Radiant include:

* Capital efficiency
* Predictable risk management
* Fast settlement and low transaction costs
* Transparent on-chain accounting

---

## 🧩 Core Features

* **Collateralized Lending & Borrowing**
  Users deposit supported assets as collateral and borrow against them based on protocol-defined loan-to-value (LTV) ratios.

* **Automated Interest Accrual**
  Interest is accrued on outstanding borrows in real time using on-chain rate models.

* **Health Factor Monitoring**
  Each position maintains a health factor that updates dynamically based on collateral value, debt, and market conditions.

* **Liquidation Mechanism**
  Under-collateralized positions can be liquidated by third parties to ensure protocol stability.

* **Non-Custodial Design**
  Users retain full control of funds through Solana programs without intermediaries.

---

## 🔗 Solana & Anchor Architecture

Radiant is implemented using **Anchor**, providing:

* Strong account validation
* Predictable program interfaces
* Secure PDA-based state management
* Composable integration with the Solana ecosystem

On-chain programs handle core lending logic, while off-chain components may be used for indexing and analytics.

---

## 🏗️ Protocol Components

* **Lending Pool** – Manages deposits, borrows, and interest calculations
* **Collateral Vaults** – Securely store deposited assets
* **Oracle Integration** – Provides asset price feeds for risk evaluation
* **Liquidation Engine** – Enforces solvency through incentive-aligned liquidations

---

## 🛠️ Tech Stack

* **Blockchain:** Solana
* **Smart Contracts:** Anchor (Rust)
* **Client:** TypeScript
* **Wallets:** Phantom, Solflare
* **Oracles:** Pyth / Switchboard (planned)

---

## 📍 Development Status

Radiant is under active development.
The codebase is experimental and subject to change as protocol parameters, interest models, and risk controls are refined.

This repository serves as the reference implementation for the protocol.

---

## ⚠️ Disclaimer

Radiant is experimental software.
Interacting with decentralized lending protocols involves financial risk.
Use at your own discretion.

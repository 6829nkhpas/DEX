// ---------------------------------------------------------------------------
// AccountPanel — live balance display with deposit / withdraw stubs
// ---------------------------------------------------------------------------
//
// Reads account state from the store and displays:
//   - per-asset balances
//   - approximate total portfolio value (optional/stub)
//   - deposit / withdraw buttons that open modal stubs
//
// Balances update reactively from WS account snapshot/delta events.
// ---------------------------------------------------------------------------

import React, { useState, useCallback } from "react";
import { useDexStore } from "../../state/StoreProvider";
import { useWallet } from "../../wallet/WalletProvider";
import { DepositModal } from "./DepositModal";
import { WithdrawModal } from "./WithdrawModal";

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

export const AccountPanel: React.FC = () => {
  const { state } = useDexStore();
  const { address, accountId } = useWallet();
  const [showDeposit, setShowDeposit] = useState(false);
  const [showWithdraw, setShowWithdraw] = useState(false);

  const account = state.account;

  if (!address) {
    return (
      <div className="bg-gray-900 border border-gray-800 rounded p-4 text-gray-500 text-sm">
        Connect a wallet to view account balances.
      </div>
    );
  }

  const balances = account?.balances ?? {};
  const assets = Object.keys(balances).sort();

  return (
    <div className="bg-gray-900 border border-gray-800 rounded p-4">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-sm font-bold text-white uppercase tracking-wide">
          Account
        </h2>
        <span
          className="text-xs text-gray-500 font-mono truncate max-w-[180px]"
          title={accountId ?? ""}
        >
          {accountId ? `${accountId.slice(0, 8)}…` : "—"}
        </span>
      </div>

      {/* Balance table */}
      {assets.length === 0 ? (
        <p className="text-xs text-gray-500 mb-3">No balances yet.</p>
      ) : (
        <table className="w-full text-xs mb-3">
          <thead>
            <tr className="text-gray-500 border-b border-gray-800">
              <th className="text-left py-1">Asset</th>
              <th className="text-right py-1">Balance</th>
            </tr>
          </thead>
          <tbody>
            {assets.map((asset) => (
              <tr
                key={asset}
                className="border-b border-gray-800/50 hover:bg-gray-800/30"
              >
                <td className="py-1 font-mono text-white">{asset}</td>
                <td className="py-1 text-right font-mono text-gray-300">
                  {balances[asset]}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {/* Actions */}
      <div className="flex gap-2">
        <button
          onClick={() => setShowDeposit(true)}
          className="flex-1 py-1.5 text-xs font-medium bg-green-700 hover:bg-green-600 text-white rounded transition-colors"
        >
          Deposit
        </button>
        <button
          onClick={() => setShowWithdraw(true)}
          className="flex-1 py-1.5 text-xs font-medium bg-red-700 hover:bg-red-600 text-white rounded transition-colors"
        >
          Withdraw
        </button>
      </div>

      {/* Modals */}
      {showDeposit && (
        <DepositModal onClose={() => setShowDeposit(false)} />
      )}
      {showWithdraw && (
        <WithdrawModal onClose={() => setShowWithdraw(false)} />
      )}
    </div>
  );
};
